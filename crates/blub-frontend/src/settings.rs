//! User settings, kept in `settings.toml` in the app's data folder
//! (Windows: %APPDATA%\Blub; Android: the app's private storage).

use crate::gpu::Display;
use crate::piano::Input;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use winit::keyboard::KeyCode;

pub const MAX_RECENT: usize = 8;

/// A user-made folder grouping library games.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LibraryFolder {
    pub name: String,
    pub games: Vec<PathBuf>,
}

/// Screen recording encoder preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum RecordEncoder {
    /// H.264 video + AAC audio (.mp4). Widely compatible, small files.
    #[default]
    H264,
    /// H.265 video + AAC audio (.mp4). Smaller files, slower to encode.
    H265,
    /// VP9 video + Opus audio (.webm). Open formats.
    Vp9,
    /// FFV1 video + raw PCM audio (.mkv). Lossless, large files.
    Lossless,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Cartridges opened recently, newest first.
    pub recent_carts: Vec<PathBuf>,
    /// Folders the game library scans for cartridges.
    pub rom_folders: Vec<PathBuf>,
    pub display: Display,
    /// AI upscaler style, quality and resolution (Display::Ai).
    pub ai: crate::upscale::AiSettings,
    /// Window size as a multiple of the picture (0 = the largest that fits
    /// the screen).
    pub window_scale: u32,
    pub volume: f32,
    /// Host-side sound effects.
    pub audio_enhance: crate::audio_out::Enhance,
    pub muted: bool,
    /// Emulation speed in percent (0 = unlimited).
    pub speed_percent: u32,
    /// PC key name (winit `KeyCode` names, e.g. "KeyZ") -> the piano input
    /// it plays (`piano::Input::name`: "Key0".."Key35", "Enter"...). A key
    /// plays one input; an input may have several keys.
    pub keys: BTreeMap<String, String>,
    /// The MIDI input port: "auto" (the first one there is), "none", or a
    /// port's name.
    pub midi_input: String,
    /// MIDI notes moved by this many octaves (-2..=2) before they reach the
    /// piano's three octaves.
    pub midi_octave_shift: i32,
    /// The piano's TV / Keyboard Mode slider at Keyboard Mode (no TV
    /// picture; the keys play notes).
    pub keyboard_mode: bool,
    /// Show the game being played on Discord.
    pub discord_presence: bool,
    pub show_status_bar: bool,
    /// F3: the details panel (frame rates, speed, sound, GPU) over the game.
    #[serde(default)]
    pub show_details: bool,
    /// Pause the game while Blub's window is not the active one.
    pub pause_in_background: bool,
    /// The library page open last ("" all games), where the library opens
    /// next time.
    pub library_tab: String,
    /// Console view: the picture above a drawing of the piano's keyboard
    /// and buttons, which the mouse can play (see `console_view`).
    pub console_view: bool,
    /// Console view: show each control's keyboard key on it, as a keycap
    /// that lights up when pressed.
    pub console_view_keys: bool,
    /// Ask GitHub for a newer release when Blub starts.
    pub check_for_updates: bool,
    /// The Blub version that ran last: a different one backs the settings
    /// and states up first (`backup::before_update`).
    pub last_version: String,
    /// A release the player chose to skip ("v1.2.0"): not announced at
    /// start-up again (Help → Check for updates still shows it).
    pub skipped_update: String,
    /// Android: show a small menu button in the top-left corner during games
    /// instead of swiping from the left edge to open the side menu.
    pub touch_menu_button: bool,
    /// The first start's note about AI was read.
    pub ai_notice_seen: bool,
    /// Screen recording: encoder preset.
    pub record_encoder: RecordEncoder,
    /// Screen recording: output folder (None = the Videos folder).
    pub record_folder: Option<PathBuf>,
    /// Favorited games (paths), in the order they were starred.
    pub favorites: Vec<PathBuf>,
    /// Tabs where "Internal" (built-in) games are hidden; key is the tab name.
    pub hide_builtin_tabs: BTreeSet<String>,
    /// User-made folders grouping library games.
    pub folders: Vec<LibraryFolder>,
    /// Games' own settings (game_settings.rs), by cartridge file name: only
    /// those that differ from the usual ones.
    pub game_settings: BTreeMap<String, toml::Table>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recent_carts: Vec::new(),
            rom_folders: Vec::new(),
            display: Display::Pixel,
            ai: Default::default(),
            window_scale: 0,
            volume: 0.8,
            audio_enhance: Default::default(),
            muted: false,
            speed_percent: 100,
            keys: default_keys(),
            midi_input: "auto".into(),
            midi_octave_shift: 0,
            keyboard_mode: false,
            discord_presence: true,
            show_status_bar: true,
            show_details: false,
            pause_in_background: true,
            library_tab: String::new(),
            console_view: true,
            console_view_keys: true,
            check_for_updates: true,
            last_version: String::new(),
            skipped_update: String::new(),
            touch_menu_button: false,
            ai_notice_seen: false,
            record_encoder: RecordEncoder::H264,
            record_folder: None,
            favorites: Vec::new(),
            hide_builtin_tabs: BTreeSet::new(),
            folders: Vec::new(),
            game_settings: BTreeMap::new(),
        }
    }
}

/// The piano's default keys (`piano::default_keys`), by name.
pub fn default_keys() -> BTreeMap<String, String> {
    crate::piano::default_keys().into_iter().map(|(k, i)| (key_name(k), i.name())).collect()
}

/// A key's name as short as a keycap shows it ("ArrowUp" -> "↑",
/// "Numpad7" -> "7", "PageUp" -> "PgUp").
pub fn cap_text(name: &str) -> String {
    match name {
        "ArrowUp" => "↑".into(),
        "ArrowDown" => "↓".into(),
        "ArrowLeft" => "←".into(),
        "ArrowRight" => "→".into(),
        "NumpadMultiply" => "*".into(),
        "NumpadAdd" => "+".into(),
        "NumpadDivide" => "/".into(),
        "NumpadDecimal" => ".".into(),
        "NumpadSubtract" | "Minus" => "-".into(),
        "IntlBackslash" | "Backslash" => "\\".into(),
        "Equal" => "=".into(),
        "PageUp" => "PgUp".into(),
        "PageDown" => "PgDn".into(),
        "Backspace" => "Bksp".into(),
        "ShiftLeft" | "ShiftRight" => "Shift".into(),
        "ControlLeft" => "Ctrl".into(),
        "AltLeft" => "Alt".into(),
        "NumpadEnter" => "Enter".into(),
        "Semicolon" => ";".into(),
        "Quote" => "'".into(),
        "Comma" => ",".into(),
        "Period" => ".".into(),
        "Slash" => "/".into(),
        "BracketLeft" => "[".into(),
        "BracketRight" => "]".into(),
        n => n.strip_prefix("Numpad").map(str::to_string).unwrap_or_else(|| key_label(n)),
    }
}

/// Keys that can be bound, by name (not Tab and `, which fast-forward and
/// rewind, Esc and the F keys).
const KEYS: &[KeyCode] = &[
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Enter,
    KeyCode::NumpadEnter,
    KeyCode::Space,
    KeyCode::Backspace,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::Insert,
    KeyCode::Delete,
    KeyCode::IntlBackslash,
    KeyCode::Backslash,
    KeyCode::NumpadAdd,
    KeyCode::NumpadSubtract,
    KeyCode::NumpadMultiply,
    KeyCode::NumpadDivide,
    KeyCode::NumpadDecimal,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
    KeyCode::ControlLeft,
    KeyCode::AltLeft,
    KeyCode::Minus,
    KeyCode::Equal,
    KeyCode::Comma,
    KeyCode::Period,
    KeyCode::Slash,
    KeyCode::Semicolon,
    KeyCode::Quote,
    KeyCode::BracketLeft,
    KeyCode::BracketRight,
    KeyCode::Numpad0,
    KeyCode::Numpad1,
    KeyCode::Numpad2,
    KeyCode::Numpad3,
    KeyCode::Numpad4,
    KeyCode::Numpad5,
    KeyCode::Numpad6,
    KeyCode::Numpad7,
    KeyCode::Numpad8,
    KeyCode::Numpad9,
    KeyCode::KeyA,
    KeyCode::KeyB,
    KeyCode::KeyC,
    KeyCode::KeyD,
    KeyCode::KeyE,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyM,
    KeyCode::KeyN,
    KeyCode::KeyO,
    KeyCode::KeyP,
    KeyCode::KeyQ,
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
    KeyCode::KeyV,
    KeyCode::KeyW,
    KeyCode::KeyX,
    KeyCode::KeyY,
    KeyCode::KeyZ,
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

pub fn key_name(k: KeyCode) -> String {
    format!("{k:?}")
}

pub fn key_from_name(name: &str) -> Option<KeyCode> {
    KEYS.iter().copied().find(|k| key_name(*k) == name)
}

/// A friendlier label for menus ("ArrowUp" -> "Up", "KeyZ" -> "Z").
pub fn key_label(name: &str) -> String {
    match name {
        "IntlBackslash" => return "\\ (left of Z)".into(),
        "Backslash" => return "\\".into(),
        "NumpadAdd" => return "Numpad +".into(),
        "NumpadSubtract" => return "Numpad -".into(),
        "NumpadMultiply" => return "Numpad *".into(),
        "NumpadDivide" => return "Numpad /".into(),
        _ => {}
    }
    name.strip_prefix("Key")
        .or_else(|| name.strip_prefix("Digit"))
        .or_else(|| name.strip_prefix("Arrow"))
        .unwrap_or(name)
        .to_string()
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                // Kept aside, not written over (e.g. settings from a newer
                // Blub read by an older one).
                tracing::warn!(target: "blub::settings", "ignoring unreadable {}: {e}", path.display());
                crate::backup::set_aside(path, "unreadable");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The piano input a key plays, if any.
    pub fn input_for(&self, key: KeyCode) -> Option<Input> {
        self.keys.get(&key_name(key)).and_then(|i| Input::from_name(i))
    }

    /// The keys that play `input` (names), in key-name order.
    pub fn keys_for(&self, input: Input) -> Vec<&str> {
        let name = input.name();
        self.keys.iter().filter(|(_, i)| **i == name).map(|(k, _)| k.as_str()).collect()
    }

    /// `key` alone plays `input` from now (its other keys let go of it, and
    /// the key stops playing whatever it played).
    pub fn bind(&mut self, input: Input, key: KeyCode) {
        let name = input.name();
        self.keys.retain(|_, i| *i != name);
        self.keys.insert(key_name(key), name);
    }

    pub fn add_recent(&mut self, cart: &Path) {
        self.recent_carts.retain(|p| p != cart);
        self.recent_carts.insert(0, cart.to_path_buf());
        self.recent_carts.truncate(MAX_RECENT);
    }
}

/// The app's data folder (settings, saves, screenshots).
pub fn default_data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("Blub");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("blub");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("blub");
    }
    PathBuf::from("blub-data")
}

#[cfg(test)]
mod tests {
    use super::*;
    use blub_piano::Button;

    #[test]
    fn defaults_round_trip_and_bindings_move() {
        let s = Settings::default();
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        assert_eq!(s.input_for(KeyCode::KeyZ), Some(Input::Key(2)));
        assert_eq!(s.keys_for(Input::Button(Button::Home)), ["Backspace", "Home"]);
        let mut s = s;
        s.bind(Input::Button(Button::Home), KeyCode::KeyZ);
        assert_eq!(s.input_for(KeyCode::KeyZ), Some(Input::Button(Button::Home)));
        assert_eq!(s.keys_for(Input::Button(Button::Home)), ["KeyZ"], "the new key alone");
        assert!(s.keys_for(Input::Key(2)).is_empty(), "the key moved to Home");
        assert!(default_keys().keys().all(|k| key_from_name(k).is_some()));
        assert_eq!(
            (cap_text("ArrowUp").as_str(), cap_text("KeyZ").as_str(), cap_text("Numpad7").as_str()),
            ("↑", "Z", "7")
        );
        assert_eq!((cap_text("PageDown").as_str(), cap_text("IntlBackslash").as_str()), ("PgDn", "\\"));
    }

    #[test]
    fn recent_list_is_most_recent_first_and_bounded() {
        let mut s = Settings::default();
        for i in 0..10 {
            s.add_recent(Path::new(&format!("game{i}.bin")));
        }
        s.add_recent(Path::new("game5.bin"));
        assert_eq!(s.recent_carts.len(), MAX_RECENT);
        assert_eq!(s.recent_carts[0], Path::new("game5.bin"));
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let s: Settings = toml::from_str("volume = 0.25").unwrap();
        assert_eq!((s.volume, s.window_scale), (0.25, 0));
        assert!(s.console_view, "the piano's keyboard shows unless turned off");
        assert_eq!((s.midi_input.as_str(), s.midi_octave_shift), ("auto", 0));
    }

    #[test]
    fn settings_from_before_still_load() {
        // Settings that are gone (an earlier Blub's "Turn off when idle")
        // are ignored, not a reason to lose the rest.
        let s: Settings = toml::from_str("turn_off_when_idle = true\nflash_before_idle_off = false\nvolume = 0.5").unwrap();
        assert_eq!(s.volume, 0.5);
    }

    #[test]
    fn console_view_choice_is_kept() {
        let dir = std::env::temp_dir().join(format!("blub-settings-{}", std::process::id()));
        let path = dir.join("settings.toml");
        let s = Settings { console_view: false, midi_input: "none".into(), ..Default::default() };
        s.save(&path).unwrap();
        let back = Settings::load(&path);
        assert!(!back.console_view && back.midi_input == "none");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
