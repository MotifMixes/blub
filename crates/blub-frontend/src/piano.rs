//! The Fisher-Price I Can Play Piano's controls as Blub sees them: its 36
//! keys and 15 buttons as one list of inputs (`Input`), with the PC keys
//! they start on (MAME's icanpian layout), their names and the notes a MIDI
//! keyboard plays (`MidiKeys`). The machine itself is `blub_piano`; it runs
//! on its own thread (emu.rs).

use blub_piano::{Button, KEYS};
use winit::keyboard::KeyCode;

/// One of the piano's inputs: a key (0 = the lowest F, 35 the highest E)
/// or a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(u8),
    Button(Button),
}

impl Input {
    /// Inputs in all: the keys, then the buttons.
    pub const COUNT: usize = KEYS + Button::ALL.len();

    /// Its place in `all` (0..COUNT).
    pub fn index(self) -> usize {
        match self {
            Input::Key(k) => k as usize,
            Input::Button(b) => KEYS + Button::ALL.iter().position(|&x| x == b).unwrap_or(0),
        }
    }

    pub fn from_index(i: usize) -> Option<Input> {
        if i < KEYS {
            Some(Input::Key(i as u8))
        } else {
            Button::ALL.get(i - KEYS).map(|&b| Input::Button(b))
        }
    }

    /// Every input: the keys low to high, then the buttons.
    pub fn all() -> impl Iterator<Item = Input> {
        (0..Self::COUNT).filter_map(Self::from_index)
    }

    /// The buttons alone.
    pub fn buttons() -> impl Iterator<Item = Input> {
        Button::ALL.into_iter().map(Input::Button)
    }

    /// Its name in settings.toml ("Key0".."Key35", "Enter", "TempoUp"...).
    pub fn name(self) -> String {
        match self {
            Input::Key(k) => format!("Key{k}"),
            Input::Button(b) => format!("{b:?}"),
        }
    }

    pub fn from_name(name: &str) -> Option<Input> {
        Self::all().find(|i| i.name() == name)
    }

    /// As the player knows it: the note ("F2", "C#4") or the button's name.
    pub fn label(self) -> String {
        match self {
            Input::Key(k) => note_name(k as usize),
            Input::Button(b) => b.name().to_string(),
        }
    }
}

const NOTES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Key `key`'s note: key 0 is F2, the middle octave's C middle C (C4).
pub fn note_name(key: usize) -> String {
    let midi = blub_piano::MIDI_KEY0 as usize + key;
    format!("{}{}", NOTES[midi % 12], midi / 12 - 1)
}

/// Whether key `key` is a black key.
pub fn is_black(key: usize) -> bool {
    NOTES[(blub_piano::MIDI_KEY0 as usize + key) % 12].ends_with('#')
}

/// The PC keys the inputs start on, from MAME's icanpian driver: the three
/// octaves on three rows of letters (the lowest F on the key left of Z, on
/// ISO keyboards), the buttons on the number pad, the arrows, Enter, Home
/// and Backspace.
pub fn default_keys() -> Vec<(KeyCode, Input)> {
    use KeyCode as K;
    let keys = [
        // Keys 0-11, F to E.
        K::IntlBackslash, K::KeyA, K::KeyZ, K::KeyS, K::KeyX, K::KeyD, K::KeyC, K::KeyV, K::KeyG, K::KeyB, K::KeyH, K::KeyN,
        // 12-23.
        K::KeyM, K::KeyK, K::Comma, K::KeyL, K::Period, K::Semicolon, K::KeyQ, K::KeyW, K::Digit3, K::KeyE, K::Digit4, K::KeyR,
        // 24-35.
        K::KeyT, K::Digit6, K::KeyY, K::Digit7, K::KeyU, K::Digit8, K::KeyI, K::KeyO, K::Digit0, K::KeyP, K::Minus, K::BracketLeft,
    ];
    let buttons = [
        (K::Numpad5, Button::ChangeInstrument),
        (K::Numpad6, Button::CycleHands),
        (K::Numpad1, Button::Display1),
        (K::Numpad2, Button::Display2),
        (K::Numpad3, Button::Display3),
        (K::Numpad4, Button::Display4),
        (K::NumpadAdd, Button::TempoUp),
        (K::NumpadMultiply, Button::TempoDefault),
        (K::NumpadSubtract, Button::TempoDown),
        (K::Numpad7, Button::Pause),
        (K::Numpad8, Button::Metronome),
        (K::ArrowUp, Button::ScrollUp),
        (K::ArrowDown, Button::ScrollDown),
        (K::Enter, Button::Enter),
        (K::NumpadEnter, Button::Enter),
        (K::Home, Button::Home),
        (K::Backspace, Button::Home),
    ];
    keys.into_iter()
        .enumerate()
        .map(|(i, k)| (k, Input::Key(i as u8)))
        .chain(buttons.into_iter().map(|(k, b)| (k, Input::Button(b))))
        .collect()
}

/// The keys held by a MIDI keyboard: notes outside the piano's three
/// octaves fold onto its keys (`blub_piano::midi_key`), so a key stays down
/// while any note on it is held.
#[derive(Debug, Clone)]
pub struct MidiKeys {
    held: [u8; KEYS],
}

impl Default for MidiKeys {
    fn default() -> Self {
        Self { held: [0; KEYS] }
    }
}

impl MidiKeys {
    /// Note `note` pressed or let go, moved by `octaves` first; the key and
    /// whether it is now down, when that changed.
    pub fn note(&mut self, note: u8, on: bool, octaves: i32) -> Option<(usize, bool)> {
        let note = (note as i32 + 12 * octaves).clamp(0, 127) as u8;
        let key = blub_piano::midi_key(note);
        let n = &mut self.held[key];
        let before = *n > 0;
        if on {
            *n = n.saturating_add(1);
        } else {
            *n = n.saturating_sub(1);
        }
        (before != (*n > 0)).then_some((key, *n > 0))
    }

    /// Every note let go: the keys that were down.
    pub fn release_all(&mut self) -> Vec<usize> {
        let down = (0..KEYS).filter(|&k| self.held[k] > 0).collect();
        self.held = [0; KEYS];
        down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_have_names_and_places() {
        assert_eq!(Input::all().count(), Input::COUNT);
        for (i, input) in Input::all().enumerate() {
            assert_eq!(input.index(), i);
            assert_eq!(Input::from_name(&input.name()), Some(input));
        }
        assert_eq!(note_name(0), "F2");
        assert_eq!(note_name(7), "C3");
        assert_eq!(note_name(19), "C4", "middle C");
        assert_eq!(note_name(35), "E5");
        assert!(is_black(1) && !is_black(0) && !is_black(2) && is_black(5) && !is_black(6));
        assert_eq!(Input::Button(Button::Enter).label(), "Enter");
    }

    #[test]
    fn the_default_keys_follow_mame() {
        let keys = default_keys();
        let find = |k: KeyCode| keys.iter().find(|(c, _)| *c == k).map(|(_, i)| *i);
        assert_eq!(find(KeyCode::IntlBackslash), Some(Input::Key(0)));
        assert_eq!(find(KeyCode::KeyN), Some(Input::Key(11)));
        assert_eq!(find(KeyCode::KeyM), Some(Input::Key(12)));
        assert_eq!(find(KeyCode::KeyR), Some(Input::Key(23)));
        assert_eq!(find(KeyCode::BracketLeft), Some(Input::Key(35)));
        assert_eq!(find(KeyCode::Backspace), Some(Input::Button(Button::Home)));
        // Every key and button has a key, and no key does two things.
        for input in Input::all() {
            assert!(keys.iter().any(|(_, i)| *i == input), "{input:?}");
        }
        let mut codes: Vec<String> = keys.iter().map(|(c, _)| format!("{c:?}")).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), keys.len());
    }

    #[test]
    fn midi_notes_folding_onto_one_key_hold_it_together() {
        let mut m = MidiKeys::default();
        // Middle C (60) is key 19; C an octave up and down fold onto keys
        // 31 and 7, but C6 (84) folds onto 31 too.
        assert_eq!(m.note(60, true, 0), Some((19, true)));
        assert_eq!(m.note(72, true, 0), Some((31, true)));
        assert_eq!(m.note(84, true, 0), None, "already down");
        assert_eq!(m.note(72, false, 0), None, "C6 still holds it");
        assert_eq!(m.note(84, false, 0), Some((31, false)));
        assert_eq!(m.note(60, true, 1), Some((31, true)), "an octave up");
        assert_eq!(m.release_all(), vec![19, 31]);
        assert_eq!(m.note(60, false, 0), None, "already let go");
    }
}
