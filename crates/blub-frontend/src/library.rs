//! The game library: cartridges found in the player's ROM folders, with
//! each game's own title screen as its art.
//!
//! A background thread scans the folders, then makes any missing art by
//! running the game unseen (no sound, nothing saved) and keeping its most
//! colourful screen of the first seconds, which is the game's title screen.
//! Art is cached as PNG in the data folder, so each game is run once.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

const EXTENSIONS: [&str; 3] = ["bin", "rom", "zip"];
/// Folder depth scanned below each ROM folder.
const MAX_DEPTH: usize = 3;
/// Art for unknown cartridges: frames searched for the title screen (20 s),
/// after the console's own logo (the same on every cartridge), and how often
/// to look.
const ART_FRAMES: u64 = 1200;
const ART_AFTER_LOGO: u64 = 450;
const ART_EVERY: u64 = 20;
/// Games' art made at once (each runs a machine on its own thread): all
/// cores but two, so the first library scan is quick.
fn art_threads() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get().saturating_sub(2).max(1))
}

/// Where a game's art is cached (the suffix changes whenever the way art
/// is made changes, so old pictures are made again).
const ART_SUFFIX: &str = "-2.png";

fn art_file(art_dir: &Path, key: &str) -> PathBuf {
    art_dir.join(format!("{key}{ART_SUFFIX}"))
}

/// Deletes art made the old way.
fn remove_stale_art(art_dir: &Path) {
    let Ok(rd) = std::fs::read_dir(art_dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".png") && !name.ends_with(ART_SUFFIX) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The consoles Blub plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hardware {
    /// The Fisher-Price I Can Play Piano (2006).
    Piano,
}

impl Hardware {
    pub fn name(self) -> &'static str {
        match self {
            Hardware::Piano => "I Can Play Piano",
        }
    }
}

/// Which console a game runs on (and the library's tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// An I Can Play Piano cartridge.
    Piano,
}

impl Family {
    pub const ALL: [Family; 1] = [Family::Piano];

    /// Book readers' "games" (none yet: kept for the library's Books side).
    pub fn is_book(self) -> bool {
        false
    }

    pub fn name(self) -> &'static str {
        self.console().name()
    }

    pub fn console(self) -> Hardware {
        match self {
            Family::Piano => Hardware::Piano,
        }
    }

    /// Whether this family's page lists `game`.
    pub fn lists(self, game: Family) -> bool {
        self == game
    }
}

/// Known cartridges, by CRC32 (the MAME software list's): title, and the
/// frame of the boot where its title screen is fully shown (for its art).
const KNOWN: &[(u32, &str, u64)] = &[
    (0x908a_965d, "American Idol - Take the Stage", 840),
    (0x393b_eea8, "Barbie Princess - Enchanted Sounds", 720),
    (0x9136_a225, "Dora the Explorer - Musical Adventure", 840),
    (0x12ef_ca09, "Holiday Wonderland", 960),
    (0x6408_abb6, "Jammin' Jungle Safari", 720),
    (0x632a_b0a3, "Jungle Boogie", 900),
    (0xc5f4_c7eb, "Magical Melodies", 720),
    (0xcca3_1d78, "Piano Favorites", 840),
    (0xc15c_cdad, "Pop Hits", 780),
    (0x00d5_97d3, "Rockin' Dance Party", 900),
    (0xe16c_715e, "Scooby-Doo! Monster Mayhem", 780),
    (0xc958_36fd, "Show Tunes", 840),
    (0x4c00_4177, "The Wizard of Oz", 960),
];

pub struct Game {
    pub path: PathBuf,
    pub family: Family,
    pub title: String,
    /// One line under the title: the console and size.
    pub subtitle: String,
    /// Label, value pairs for the details tooltip.
    pub details: Vec<(&'static str, String)>,
    /// Badges shown with the game.
    pub tags: Vec<std::borrow::Cow<'static, str>>,
    pub(crate) key: String,
    pub art: Art,
}

impl Game {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

pub enum Art {
    /// Not made yet (or being made).
    Pending,
    /// Can't be made (the game showed nothing).
    Missing,
    Image(egui::ColorImage),
    Texture(egui::TextureHandle),
}

enum Msg {
    Game(Game),
    Art(String, Option<egui::ColorImage>),
    Scanned,
    Done,
}

pub struct Library {
    pub games: Vec<Game>,
    pub scanning: bool,
    pub making_art: bool,
    art_dir: PathBuf,
    rx: Option<Receiver<Msg>>,
    /// A rescan's games, collected while the previous list stays on show
    /// (so the library never goes blank); swapped in once listed.
    incoming: Option<Vec<Game>>,
}

impl Library {
    pub fn new(data_dir: &Path) -> Self {
        Self { games: Vec::new(), scanning: false, making_art: false, art_dir: data_dir.join("art"), rx: None, incoming: None }
    }

    /// Rescans `folders` (plus `extra` files, e.g. recent games) in the
    /// background.
    pub fn scan(&mut self, folders: &[PathBuf], extra: &[PathBuf]) {
        let (tx, rx) = channel();
        // The first scan fills the list as it goes; later ones replace it whole.
        self.incoming = (!self.games.is_empty()).then(Vec::new);
        self.scanning = true;
        self.making_art = false;
        self.rx = Some(rx); // a running older scan sees its receiver gone and stops
        let (folders, extra, art_dir) = (folders.to_vec(), extra.to_vec(), self.art_dir.clone());
        let spawned = std::thread::Builder::new().name("blub-library".into()).spawn(move || worker(&folders, &extra, &art_dir, &tx));
        if spawned.is_err() {
            self.scanning = false;
        }
    }

    /// Takes in what the background thread found; call every frame.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Game(g) => {
                    let list = self.incoming.as_mut().unwrap_or(&mut self.games);
                    let at = list.partition_point(|o| o.title.to_lowercase() <= g.title.to_lowercase());
                    list.insert(at, g);
                }
                Msg::Art(key, image) => {
                    let lists = self.games.iter_mut().chain(self.incoming.iter_mut().flatten());
                    for g in lists.filter(|g| g.key == key) {
                        g.art = image.clone().map_or(Art::Missing, Art::Image);
                    }
                }
                Msg::Scanned => {
                    if let Some(list) = self.incoming.take() {
                        self.games = list;
                    }
                    self.scanning = false;
                    self.making_art = self.games.iter().any(|g| matches!(g.art, Art::Pending));
                }
                Msg::Done => {
                    if let Some(list) = self.incoming.take() {
                        self.games = list;
                    }
                    self.scanning = false;
                    self.making_art = false;
                    for g in &mut self.games {
                        if matches!(g.art, Art::Pending) {
                            g.art = Art::Missing;
                        }
                    }
                    self.rx = None;
                    return;
                }
            }
        }
    }

    /// Whether this console can start (every console Blub plays needs
    /// nothing but its cartridges).
    pub fn has_console(&self, _hw: Hardware) -> bool {
        true
    }

    /// Forgets a game's cached art, so the next scan makes it again.
    pub fn forget_art(&mut self, path: &Path) {
        for g in self.games.iter_mut().filter(|g| g.path == path) {
            let _ = std::fs::remove_file(art_file(&self.art_dir, &g.key));
            g.art = Art::Pending;
        }
    }
}

fn worker(folders: &[PathBuf], extra: &[PathBuf], art_dir: &Path, tx: &Sender<Msg>) {
    remove_stale_art(art_dir);
    let mut files = Vec::new();
    for f in folders {
        collect(f, 0, &mut files);
    }
    files.extend(extra.iter().filter(|p| p.is_file()).cloned());
    let mut seen_paths = HashSet::new();
    let mut seen_games = HashSet::new();
    let mut todo = Vec::new();
    for path in files {
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen_paths.insert(canonical) {
            continue;
        }
        let Some(cart) = read_cartridge(&path) else {
            continue;
        };
        let key = format!("{:08x}", crc32fast::hash(&cart));
        // The same game twice (a copy, or zipped and not): listed once.
        if !seen_games.insert(key.clone()) {
            continue;
        }
        let art = match load_png(&art_file(art_dir, &key)) {
            Some(img) => Art::Image(img),
            None => {
                todo.push((key.clone(), path.clone()));
                Art::Pending
            }
        };
        if tx.send(Msg::Game(describe(&path, &cart, key, art))).is_err() {
            return;
        }
    }
    if tx.send(Msg::Scanned).is_err() {
        return;
    }
    // Art, several games at once.
    let todo = std::sync::Mutex::new(todo);
    std::thread::scope(|scope| {
        for _ in 0..art_threads() {
            scope.spawn(|| {
                loop {
                    let Some((key, path)) = todo.lock().unwrap().pop() else {
                        return;
                    };
                    let image = read_cartridge(&path).and_then(|cart| make_art(&cart)).map(|shot| {
                        let _ = std::fs::create_dir_all(art_dir);
                        if let Err(e) = save_png(&art_file(art_dir, &key), &shot) {
                            tracing::warn!(target: "blub::library", "saving art: {e:#}");
                        }
                        to_image(&shot)
                    });
                    if tx.send(Msg::Art(key, image)).is_err() {
                        // The scan was replaced: stop.
                        todo.lock().unwrap().clear();
                        return;
                    }
                }
            });
        }
    });
    let _ = tx.send(Msg::Done);
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if depth < MAX_DEPTH {
                collect(&p, depth + 1, out);
            }
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e))) {
            out.push(p);
        }
    }
}

/// A cartridge's ROM (raw or zipped), if `path` is one: an I Can Play
/// Piano cartridge is 8 MB (a larger file is not taken: no cartridge is).
pub fn read_cartridge(path: &Path) -> Option<Vec<u8>> {
    let size = std::fs::metadata(path).ok()?.len();
    let zipped = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !zipped && size != blub_piano::CART_SIZE as u64 {
        return None;
    }
    let data = crate::zip::unzip_image(std::fs::read(path).ok()?).ok()?;
    (data.len() == blub_piano::CART_SIZE && blub_piano::is_cartridge(&data)).then_some(data)
}

/// A game's title: from the known list, else its file name.
pub fn title_of(path: &Path, cart: &[u8]) -> String {
    let crc = crc32fast::hash(cart);
    match KNOWN.iter().find(|(c, ..)| *c == crc) {
        Some((_, name, _)) => name.to_string(),
        None => path.file_stem().map_or_else(|| "Cartridge".into(), |s| s.to_string_lossy().replace('_', " ")),
    }
}

fn describe(path: &Path, cart: &[u8], key: String, art: Art) -> Game {
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let known = KNOWN.iter().any(|(c, ..)| format!("{c:08x}") == key);
    let mut details = vec![("File", file), ("CRC32", key.clone())];
    if !known {
        details.push(("Dump", "Not in the known list".into()));
    }
    Game {
        path: path.to_path_buf(),
        family: Family::Piano,
        title: title_of(path, cart),
        subtitle: format!("I Can Play Piano · {} MB", cart.len() >> 20),
        details,
        tags: Vec::new(),
        key,
        art,
    }
}

/// A picture for the library (SCREEN_W x SCREEN_H).
struct Shot {
    pixels: Vec<u32>,
}

/// Runs the game unseen to its title screen: a known cartridge's frame,
/// else the most colourful screen of the first `ART_FRAMES` after the
/// console's logo (the title screen, with its art; menus are plainer).
fn make_art(cart: &[u8]) -> Option<Shot> {
    use blub_piano::{SCREEN_H, SCREEN_W};
    let crc = crc32fast::hash(cart);
    let title_frame = KNOWN.iter().find(|(c, ..)| *c == crc).map(|k| k.2);
    let mut piano = blub_piano::Piano::new(cart);
    let mut frame = vec![0u32; SCREEN_W * SCREEN_H];
    let mut best: Option<(usize, Vec<u32>)> = None;
    for n in 1..=title_frame.map_or(ART_FRAMES, |f| f + 1) {
        piano.run_frame();
        // Its sound is not wanted (and would pile up).
        let _ = piano.take_audio();
        if let Some(f) = title_frame {
            if n == f + 1 {
                piano.render(&mut frame);
                return Some(Shot { pixels: frame });
            }
            continue;
        }
        if n < ART_AFTER_LOGO || n % ART_EVERY != 0 {
            continue;
        }
        piano.render(&mut frame);
        let c = colourfulness(&frame);
        if best.as_ref().is_none_or(|(b, _)| c > *b) {
            best = Some((c, frame.clone()));
        }
    }
    // A screen of a few colours is no title screen (a blank or fading one).
    best.filter(|(c, _)| *c >= 8).map(|(_, pixels)| Shot { pixels })
}

fn colourfulness(frame: &[u32]) -> usize {
    frame.iter().map(|p| p & 0x00FF_FFFF).collect::<HashSet<_>>().len()
}

fn to_image(shot: &Shot) -> egui::ColorImage {
    let rgb: Vec<u8> = shot.pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    egui::ColorImage::from_rgb([blub_piano::SCREEN_W, blub_piano::SCREEN_H], &rgb)
}

fn save_png(path: &Path, shot: &Shot) -> anyhow::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, blub_piano::SCREEN_W as u32, blub_piano::SCREEN_H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = shot.pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    enc.write_header()?.write_image_data(&rgb)?;
    Ok(())
}

fn load_png(path: &Path) -> Option<egui::ColorImage> {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let size = [info.width as usize, info.height as usize];
    let data = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgb => Some(egui::ColorImage::from_rgb(size, data)),
        png::ColorType::Rgba => Some(egui::ColorImage::from_rgba_unmultiplied(size, data)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("blub-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn title_screens_beat_flat_screens() {
        let flat = vec![0x0020_4060u32; 320 * 240];
        let busy: Vec<u32> = (0..320 * 240).map(|i| (i as u32 * 2654435761) & 0xFF_FFFF).collect();
        assert!(colourfulness(&busy) > colourfulness(&flat));
        assert_eq!(colourfulness(&flat), 1);
    }

    #[test]
    fn art_survives_a_png_round_trip() {
        let dir = temp("png");
        let shot = Shot { pixels: (0..320 * 240).map(|i| (i as u32 * 77) & 0xFF_FFFF).collect() };
        let path = dir.join("a.png");
        save_png(&path, &shot).unwrap();
        let back = load_png(&path).unwrap();
        assert_eq!(back.size, [320, 240]);
        assert_eq!(back, to_image(&shot));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scanning_takes_cartridges_only() {
        let dir = temp("scan");
        std::fs::write(dir.join("small.bin"), vec![0u8; 1024]).unwrap();
        std::fs::write(dir.join("notes.txt"), b"hi").unwrap();
        std::fs::write(dir.join("Piano Song.bin"), vec![0u8; blub_piano::CART_SIZE]).unwrap();
        let mut files = Vec::new();
        collect(&dir, 0, &mut files);
        assert_eq!(files.len(), 2, "by extension: {files:?}");
        let carts: Vec<_> = files.iter().filter(|p| read_cartridge(p).is_some()).collect();
        assert_eq!(carts.len(), 1);
        let cart = read_cartridge(carts[0]).unwrap();
        assert_eq!(title_of(carts[0], &cart), "Piano Song", "unknown: its file name");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
