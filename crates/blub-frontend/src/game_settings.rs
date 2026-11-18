//! A game's own settings, made by the player ("Custom settings for this
//! game" in the Emulation menu): while it is on and that game runs, changes
//! to the settings below are the game's and come back each time it starts;
//! when it closes, the player's usual settings are back. Kept in
//! settings.toml (`game_settings`, by cartridge file name) as only the
//! settings that differ from the usual ones.

use crate::settings::Settings;
use std::path::Path;

/// The settings a game can have its own of (`Settings` field names): the
/// picture, sound, speed, controls and the console's switches. Library,
/// folders and the like stay the player's own.
pub const PER_GAME: &[&str] = &[
    "display",
    "ai",
    "window_scale",
    "volume",
    "muted",
    "audio_enhance",
    "speed_percent",
    "keys",
    "midi_octave_shift",
    "keyboard_mode",
];

/// The name a game's settings are kept under: its cartridge's file name, so
/// they follow the game into another folder.
pub fn key(cart: &Path) -> Option<String> {
    cart.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn table(s: &Settings) -> toml::Table {
    toml::Table::try_from(s).expect("settings convert to TOML")
}

/// `usual` with a game's own settings on top.
pub fn apply(usual: &Settings, own: &toml::Table) -> Settings {
    let mut t = table(usual);
    for (k, v) in own {
        if PER_GAME.contains(&k.as_str()) {
            t.insert(k.clone(), v.clone());
        }
    }
    // A value an older or newer Blub wrote that doesn't fit: the usual
    // settings, rather than none.
    t.try_into().unwrap_or_else(|e| {
        tracing::warn!(target: "blub::settings", "ignoring a game's own settings: {e}");
        usual.clone()
    })
}

/// Splits `current` (the usual settings with a game's own on top, as changed
/// since) into the usual settings and the game's own: a per-game setting
/// that differs from `usual` is the game's; every other change is usual.
pub fn split(usual: &Settings, current: &Settings) -> (Settings, toml::Table) {
    let base = table(usual);
    let mut cur = table(current);
    let mut own = toml::Table::new();
    for &k in PER_GAME {
        let theirs = cur.remove(k);
        if theirs.as_ref() != base.get(k)
            && let Some(v) = theirs
        {
            own.insert(k.to_string(), v);
        }
        if let Some(v) = base.get(k) {
            cur.insert(k.to_string(), v.clone());
        }
    }
    let usual = cur.try_into().unwrap_or_else(|_| usual.clone());
    (usual, own)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_games_own_settings_come_and_go() {
        let mut usual = Settings::default();
        usual.volume = 0.5;
        // Played with its own settings: HD on, and a favourite added.
        let mut playing = apply(&usual, &toml::Table::new());
        playing.speed_percent = 200;
        playing.favorites.push("Show Tunes.bin".into());
        let (back, own) = split(&usual, &playing);
        assert_eq!(back.speed_percent, 100, "other games keep the usual speed");
        assert_eq!(back.favorites, playing.favorites, "a favourite is the player's");
        assert_eq!(back.volume, 0.5);
        assert_eq!(own.keys().collect::<Vec<_>>(), ["speed_percent"], "only what differs is kept");
        // Next start: the game's own on top of the usual settings.
        let again = apply(&back, &own);
        assert_eq!(again.speed_percent, 200);
        assert_eq!(again.volume, 0.5);
    }

    #[test]
    fn own_settings_survive_settings_toml() {
        let mut s = Settings::default();
        let mut own = toml::Table::new();
        own.insert("keyboard_mode".into(), toml::Value::Boolean(true));
        own.insert("speed_percent".into(), toml::Value::Integer(150));
        s.game_settings.insert("Piano Favorites.bin".into(), own.clone());
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.game_settings["Piano Favorites.bin"], own);
        let playing = apply(&back, &own);
        assert!(playing.keyboard_mode);
        assert_eq!(playing.speed_percent, 150);
    }

    #[test]
    fn unknown_or_unfitting_values_are_ignored() {
        let usual = Settings::default();
        let mut own = toml::Table::new();
        own.insert("rom_folders".into(), toml::Value::Array(vec![toml::Value::String("elsewhere".into())]));
        assert_eq!(apply(&usual, &own), usual, "not a per-game setting");
        own.insert("volume".into(), toml::Value::String("loud".into()));
        assert_eq!(apply(&usual, &own), usual, "not a volume");
    }
}
