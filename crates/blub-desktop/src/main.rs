//! `blub` command-line entry point: the window (no command, or `run`), or
//! a cartridge run without one for tests (`headless`).
//!
//! Exit codes: 0 = done, 1 = usage or load error.

use anyhow::{Context, Result, bail};
use blub_piano::{Button, GuitarButton, Piano, SCREEN_H, SCREEN_W};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "blub", version, about = "Fisher-Price I Can Play Piano emulator")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Identify a cartridge dump (size, checksum, known title).
    Info { cart: PathBuf },
    /// Run a cartridge without a window for a number of frames (for tests).
    Headless {
        /// Cartridge dump (8 MB .bin, or zipped).
        cart: PathBuf,
        /// Frames to run (60 a second).
        #[arg(long, default_value_t = 600)]
        frames: u64,
        /// Save the last picture here (.png, or .ppm).
        #[arg(long)]
        screenshot: Option<PathBuf>,
        /// Also save the picture at these frames (`<screenshot>.<frame>.png`).
        #[arg(long, value_delimiter = ',')]
        shot_at: Vec<u64>,
        /// Presses: `frame:what[:u]`, `;`-separated, where what is, on the
        /// piano, a key (k0-k35) or a button (enter, home, scrollup,
        /// tempoup...); on the guitar, a fret (f1.1-f6.12: string 1, the
        /// high E, to 6, fret 1 to 12), a string plucked (s1-s6) or a button
        /// (up, down, left, right, enter, home, pause); and `u` lets it go.
        #[arg(long, default_value = "")]
        press: String,
        /// The TV / Keyboard Mode slider at Keyboard Mode (the guitar: its
        /// switch for playing without the TV).
        #[arg(long)]
        keyboard_mode: bool,
        /// Save the sound here (.wav, stereo).
        #[arg(long)]
        audio_out: Option<PathBuf>,
        /// Start from this save state.
        #[arg(long)]
        load_state: Option<PathBuf>,
        /// Save a state at the end.
        #[arg(long)]
        save_state: Option<PathBuf>,
    },
    /// Open the window (with a cartridge, if given).
    Run { cart: Option<PathBuf> },
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

/// A cartridge dump, raw or zipped.
fn read_cart(path: &Path) -> Result<Vec<u8>> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let data = blub_frontend::zip::unzip_image(data).with_context(|| format!("unzipping {}", path.display()))?;
    if !blub_piano::is_cartridge(&data) {
        bail!("{} is not an I Can Play Piano cartridge dump (8 MB; it is {} bytes)", path.display(), data.len());
    }
    Ok(data)
}

/// A press: when, a key or a button, down or up.
struct Press {
    frame: u64,
    what: What,
    down: bool,
}

enum What {
    Key(usize),
    Button(Button),
    /// The guitar's: a fret (string 0-5, fret), a string, a button.
    Fret(usize, usize),
    String(usize),
    Guitar(GuitarButton),
}

/// A guitar input's name: `f<string>.<fret>`, `s<string>` or a button.
fn guitar_what(name: &str) -> Result<What> {
    if let Some((string, fret)) = name.strip_prefix('f').and_then(|f| f.split_once('.')) {
        let (string, fret): (usize, usize) = (string.parse()?, fret.parse()?);
        if !(1..=blub_piano::STRINGS).contains(&string) || !blub_piano::has_fret(string - 1, fret) {
            bail!("no fret {name} (f1.1-f6.12; the middle strings have frets 1-5)");
        }
        return Ok(What::Fret(string - 1, fret));
    }
    if let Some(string) = name.strip_prefix('s').and_then(|s| s.parse::<usize>().ok()) {
        if !(1..=blub_piano::STRINGS).contains(&string) {
            bail!("no string {name} (s1-s6)");
        }
        return Ok(What::String(string - 1));
    }
    GuitarButton::ALL
        .into_iter()
        .find(|b| format!("{b:?}").eq_ignore_ascii_case(name))
        .map(What::Guitar)
        .with_context(|| format!("unknown guitar input {name:?}"))
}

fn parse_presses(s: &str, guitar: bool) -> Result<Vec<Press>> {
    let mut out = Vec::new();
    for item in s.split(';').map(str::trim).filter(|i| !i.is_empty()) {
        let parts: Vec<&str> = item.split(':').collect();
        let (Some(frame), Some(name)) = (parts.first().and_then(|f| f.parse().ok()), parts.get(1)) else {
            bail!("bad press {item:?} (want frame:what[:u])");
        };
        if guitar {
            out.push(Press { frame, what: guitar_what(name)?, down: parts.get(2) != Some(&"u") });
            continue;
        }
        let what = match name.strip_prefix('k').and_then(|k| k.parse::<usize>().ok()) {
            Some(k) if k < blub_piano::KEYS => What::Key(k),
            Some(_) => bail!("no key {name} (k0-k35)"),
            None => What::Button(
                Button::ALL
                    .into_iter()
                    .find(|b| format!("{b:?}").eq_ignore_ascii_case(name))
                    .with_context(|| format!("unknown button {name:?}"))?,
            ),
        };
        out.push(Press { frame, what, down: parts.get(2) != Some(&"u") });
    }
    out.sort_by_key(|p| p.frame);
    Ok(out)
}

fn real_main() -> Result<ExitCode> {
    let cli = Cli::parse();
    blub_frontend::logging::init("info");
    match cli.command.unwrap_or(Command::Run { cart: None }) {
        Command::Info { cart } => {
            let data = read_cart(&cart)?;
            println!("{}", cart.display());
            println!("  size   {} bytes", data.len());
            println!("  crc32  {:08x}", crc32(&data));
            Ok(ExitCode::SUCCESS)
        }
        Command::Headless { cart, frames, screenshot, shot_at, press, keyboard_mode, audio_out, load_state, save_state } => {
            let mut piano = Piano::new(&read_cart(&cart)?);
            let presses = parse_presses(&press, piano.model() == blub_piano::Model::Guitar)?;
            eprintln!("{}", piano.model().name());
            piano.set_keyboard_mode(keyboard_mode);
            if let Some(path) = &load_state {
                let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
                piano.load_state(&bytes).map_err(anyhow::Error::msg).with_context(|| format!("loading state {}", path.display()))?;
            }
            let mut frame = vec![0u32; SCREEN_W * SCREEN_H];
            let mut audio = Vec::new();
            let start = std::time::Instant::now();
            for n in 0..frames {
                for p in presses.iter().filter(|p| p.frame == n) {
                    match p.what {
                        What::Key(k) => piano.key(k, p.down),
                        What::Button(b) => piano.button(b, p.down),
                        What::Fret(string, fret) => piano.fret(string, fret, p.down),
                        What::String(string) if p.down => piano.pluck(string),
                        What::String(_) => {}
                        What::Guitar(b) => piano.guitar_button(b, p.down),
                    }
                }
                piano.run_frame();
                let sound = piano.take_audio();
                if audio_out.is_some() {
                    audio.extend(sound);
                }
                if let Some(path) = &screenshot
                    && shot_at.contains(&n)
                {
                    piano.render(&mut frame);
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
                    save_picture(&path.with_extension(format!("{n}.{ext}")), &frame)?;
                }
                if piano.powered_off() {
                    println!("the console turned itself off at frame {n}");
                    break;
                }
            }
            let took = start.elapsed().as_secs_f64();
            println!(
                "{frames} frames in {took:.2} s ({:.0} fps, {:.0}% of real time), {} instructions",
                frames as f64 / took.max(1e-9),
                frames as f64 / 60.0 / took.max(1e-9) * 100.0,
                piano.instructions()
            );
            piano.render(&mut frame);
            let colours = frame.iter().collect::<std::collections::HashSet<_>>().len();
            println!("picture: {colours} colours");
            if let Some(path) = &screenshot {
                save_picture(path, &frame)?;
                println!("picture saved to {}", path.display());
            }
            if let Some(path) = &audio_out {
                write_wav(path, &audio, blub_piano::OUTPUT_RATE.round() as u32)?;
                let loud = audio.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
                println!("audio: {} stereo samples (peak {loud}) -> {}", audio.len() / 2, path.display());
            }
            if let Some(path) = save_state {
                std::fs::write(&path, piano.save_state()).with_context(|| format!("writing {}", path.display()))?;
                println!("state saved to {}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Run { cart } => {
            let opts = blub_frontend::StartOptions { data_dir: blub_frontend::default_data_dir(), cartridge: cart, ..Default::default() };
            blub_frontend::run(opts)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// The picture as PNG (or PPM, by extension).
fn save_picture(path: &Path, frame: &[u32]) -> Result<()> {
    let rgb: Vec<u8> = frame.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ppm")) {
        let mut b = format!("P6\n{SCREEN_W} {SCREEN_H}\n255\n").into_bytes();
        b.extend_from_slice(&rgb);
        return std::fs::write(path, b).with_context(|| format!("writing {}", path.display()));
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?);
    let mut enc = png::Encoder::new(file, SCREEN_W as u32, SCREEN_H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.write_header()?.write_image_data(&rgb)?;
    Ok(())
}

/// 16-bit stereo PCM.
fn write_wav(path: &Path, samples: &[i16], hz: u32) -> Result<()> {
    let data = samples.len() as u32 * 2;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&hz.to_le_bytes());
    b.extend_from_slice(&(hz * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    samples.iter().for_each(|s| b.extend_from_slice(&s.to_le_bytes()));
    std::fs::write(path, b).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn presses_parse() {
        let p = parse_presses("120:enter; 30:k5 ;40:k5:u", false).unwrap();
        assert_eq!(p.iter().map(|p| (p.frame, p.down)).collect::<Vec<_>>(), [(30, true), (40, false), (120, true)]);
        assert!(matches!(p[2].what, What::Button(Button::Enter)));
        assert!(parse_presses("1:k36", false).is_err() && parse_presses("1:nope", false).is_err() && parse_presses("x", false).is_err());
        let g = parse_presses("1:enter;2:f6.12;3:s1;4:f2.5:u", true).unwrap();
        assert!(matches!(g[0].what, What::Guitar(GuitarButton::Enter)));
        assert!(matches!(g[1].what, What::Fret(5, 12)) && matches!(g[2].what, What::String(0)) && !g[3].down);
        assert!(parse_presses("1:f2.6", true).is_err() && parse_presses("1:s7", true).is_err() && parse_presses("1:k5", true).is_err());
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
