//! The controls of the consoles Blub plays, as Blub sees them: the I Can
//! Play Piano's 36 keys and 15 buttons and the I Can Play Guitar's 44 fret
//! buttons, six strings and seven buttons, as one list of inputs (`Input`),
//! with the PC keys they start on (the piano's: MAME's icanpian layout),
//! their names and the notes a MIDI keyboard plays (`MidiKeys`, the
//! piano's). The machine itself is `blub_piano`; it runs on its own thread
//! (emu.rs).

use blub_piano::{Button, FRET_BUTTONS, GuitarButton, KEYS, STRINGS};
use winit::keyboard::KeyCode;

/// One of the consoles' inputs: a piano key (0 = the lowest F, 35 the
/// highest E) or button, or a guitar fret button (string 0-5, string 0 the
/// high E; fret 1-12), string, button, or all six strings at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(u8),
    Button(Button),
    Fret(u8, u8),
    String(u8),
    StrumAll,
    Guitar(GuitarButton),
}

/// The guitar's fret buttons, string by string (high E first), fret by
/// fret.
fn fret_buttons() -> impl Iterator<Item = (u8, u8)> {
    (0..STRINGS as u8).flat_map(|s| (1..=blub_piano::FRETS as u8).map(move |f| (s, f))).filter(|&(s, f)| blub_piano::has_fret(s as usize, f as usize))
}

/// Where the guitar's inputs start in `Input::all`.
const GUITAR_AT: usize = KEYS + Button::ALL.len();

impl Input {
    /// Inputs in all: the piano's keys, then its buttons; the guitar's
    /// frets, strings, all strings and buttons.
    pub const COUNT: usize = GUITAR_AT + FRET_BUTTONS + STRINGS + 1 + GuitarButton::ALL.len();

    /// Its place in `all` (0..COUNT).
    pub fn index(self) -> usize {
        let strings = GUITAR_AT + FRET_BUTTONS;
        match self {
            Input::Key(k) => k as usize,
            Input::Button(b) => KEYS + Button::ALL.iter().position(|&x| x == b).unwrap_or(0),
            Input::Fret(s, f) => GUITAR_AT + fret_buttons().position(|x| x == (s, f)).unwrap_or(0),
            Input::String(s) => strings + s as usize,
            Input::StrumAll => strings + STRINGS,
            Input::Guitar(b) => strings + STRINGS + 1 + GuitarButton::ALL.iter().position(|&x| x == b).unwrap_or(0),
        }
    }

    pub fn from_index(i: usize) -> Option<Input> {
        let strings = GUITAR_AT + FRET_BUTTONS;
        if i < KEYS {
            Some(Input::Key(i as u8))
        } else if i < GUITAR_AT {
            Button::ALL.get(i - KEYS).map(|&b| Input::Button(b))
        } else if i < strings {
            fret_buttons().nth(i - GUITAR_AT).map(|(s, f)| Input::Fret(s, f))
        } else if i < strings + STRINGS {
            Some(Input::String((i - strings) as u8))
        } else if i == strings + STRINGS {
            Some(Input::StrumAll)
        } else {
            GuitarButton::ALL.get(i - strings - STRINGS - 1).map(|&b| Input::Guitar(b))
        }
    }

    /// Every input: the piano's keys low to high and its buttons, then the
    /// guitar's.
    pub fn all() -> impl Iterator<Item = Input> {
        (0..Self::COUNT).filter_map(Self::from_index)
    }

    /// The piano's inputs alone.
    pub fn piano() -> impl Iterator<Item = Input> {
        (0..GUITAR_AT).filter_map(Self::from_index)
    }

    /// The guitar's inputs alone.
    pub fn guitar() -> impl Iterator<Item = Input> {
        (GUITAR_AT..Self::COUNT).filter_map(Self::from_index)
    }

    /// Whether it is one of the guitar's.
    pub fn is_guitar(self) -> bool {
        self.index() >= GUITAR_AT
    }

    /// The piano's buttons alone.
    pub fn buttons() -> impl Iterator<Item = Input> {
        Button::ALL.into_iter().map(Input::Button)
    }

    /// The guitar's buttons, its strings and all strings.
    pub fn guitar_buttons() -> impl Iterator<Item = Input> {
        GuitarButton::ALL.into_iter().map(Input::Guitar).chain((0..STRINGS as u8).map(Input::String)).chain([Input::StrumAll])
    }

    /// Its name in settings.toml ("Key0".."Key35", "Enter", "TempoUp"...;
    /// the guitar's "Fret1.3" (string 1, fret 3), "String1", "StrumAll",
    /// "GuitarUp"...).
    pub fn name(self) -> String {
        match self {
            Input::Key(k) => format!("Key{k}"),
            Input::Button(b) => format!("{b:?}"),
            Input::Fret(s, f) => format!("Fret{}.{f}", s + 1),
            Input::String(s) => format!("String{}", s + 1),
            Input::StrumAll => "StrumAll".into(),
            Input::Guitar(b) => format!("Guitar{b:?}"),
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
            Input::Fret(s, f) => format!("String {} fret {f} ({})", s + 1, midi_name(blub_piano::GUITAR_OPEN[s as usize] + f)),
            Input::String(s) => format!("String {} ({})", s + 1, midi_name(blub_piano::GUITAR_OPEN[s as usize])),
            Input::StrumAll => "All strings".into(),
            Input::Guitar(b) => b.name().to_string(),
        }
    }

    /// The piano button's counterpart on the guitar, for the controllers'
    /// layout (gamepad.rs, Android): the arrows, Enter, Home and Pause as
    /// they are; tempo down and up (LB, RB) the guitar's left and right.
    pub fn on_guitar(self) -> Input {
        let b = match self {
            Input::Button(Button::ScrollUp) => GuitarButton::Up,
            Input::Button(Button::ScrollDown) => GuitarButton::Down,
            Input::Button(Button::TempoDown) => GuitarButton::Left,
            Input::Button(Button::TempoUp) => GuitarButton::Right,
            Input::Button(Button::Enter) => GuitarButton::Enter,
            Input::Button(Button::Home) => GuitarButton::Home,
            Input::Button(Button::Pause) => GuitarButton::Pause,
            other => return other,
        };
        Input::Guitar(b)
    }
}

/// A MIDI note's name ("E4").
fn midi_name(midi: u8) -> String {
    format!("{}{}", NOTES[midi as usize % 12], midi as i32 / 12 - 1)
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

/// The PC keys the guitar's inputs start on. The fret buttons of frets 1-4
/// on the left of the keyboard, a column a string (low E on the left, as
/// the strings lie seen from above): fret 1 on the number row, 2 on Q-Y,
/// 3 on A-H, 4 on Z-N. The strings to the right, low E to high E on U I O
/// P [ ], all six on Space; the arrows, Enter, Home (and Backspace) and
/// Pause (End) for the buttons. The other frets have no key to start with
/// (the console view plays them).
pub fn default_guitar_keys() -> Vec<(KeyCode, Input)> {
    use KeyCode as K;
    let rows = [
        [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6],
        [K::KeyQ, K::KeyW, K::KeyE, K::KeyR, K::KeyT, K::KeyY],
        [K::KeyA, K::KeyS, K::KeyD, K::KeyF, K::KeyG, K::KeyH],
        [K::KeyZ, K::KeyX, K::KeyC, K::KeyV, K::KeyB, K::KeyN],
    ];
    let strings = [K::KeyU, K::KeyI, K::KeyO, K::KeyP, K::BracketLeft, K::BracketRight];
    let buttons = [
        (K::ArrowUp, GuitarButton::Up),
        (K::ArrowDown, GuitarButton::Down),
        (K::ArrowLeft, GuitarButton::Left),
        (K::ArrowRight, GuitarButton::Right),
        (K::Enter, GuitarButton::Enter),
        (K::NumpadEnter, GuitarButton::Enter),
        (K::Home, GuitarButton::Home),
        (K::Backspace, GuitarButton::Home),
        (K::End, GuitarButton::Pause),
    ];
    let mut keys = Vec::new();
    for (fret, row) in rows.iter().enumerate() {
        for (column, &k) in row.iter().enumerate() {
            keys.push((k, Input::Fret((STRINGS - 1 - column) as u8, fret as u8 + 1)));
        }
    }
    for (column, &k) in strings.iter().enumerate() {
        keys.push((k, Input::String((STRINGS - 1 - column) as u8)));
    }
    keys.push((K::Space, Input::StrumAll));
    keys.extend(buttons.into_iter().map(|(k, b)| (k, Input::Guitar(b))));
    keys
}

/// The keys held by a MIDI keyboard (`blub_piano::midi_key`; notes outside
/// the piano's three octaves play nothing). A key stays down while any note
/// on it is held (the same note twice, from two sources).
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
    /// whether it is now down, when that changed (never for a note the
    /// piano doesn't have).
    pub fn note(&mut self, note: u8, on: bool, octaves: i32) -> Option<(usize, bool)> {
        let note = u8::try_from(note as i32 + 12 * octaves).ok()?;
        let key = blub_piano::midi_key(note)?;
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
        for input in Input::piano() {
            assert!(keys.iter().any(|(_, i)| *i == input), "{input:?}");
        }
        let mut codes: Vec<String> = keys.iter().map(|(c, _)| format!("{c:?}")).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), keys.len());
    }

    #[test]
    fn guitar_inputs_have_names_and_default_keys() {
        assert_eq!(Input::guitar().count(), FRET_BUTTONS + STRINGS + 1 + GuitarButton::ALL.len());
        assert!(Input::guitar().all(Input::is_guitar) && !Input::piano().any(Input::is_guitar));
        assert_eq!(Input::Fret(0, 12).name(), "Fret1.12");
        assert_eq!(Input::Fret(5, 3).label(), "String 6 fret 3 (G2)");
        assert_eq!(Input::String(0).label(), "String 1 (E4)");
        let keys = default_guitar_keys();
        let find = |k: KeyCode| keys.iter().find(|(c, _)| *c == k).map(|(_, i)| *i);
        assert_eq!(find(KeyCode::Digit1), Some(Input::Fret(5, 1)), "low E, fret 1");
        assert_eq!(find(KeyCode::KeyN), Some(Input::Fret(0, 4)), "high E, fret 4");
        assert_eq!(find(KeyCode::BracketRight), Some(Input::String(0)));
        assert_eq!(find(KeyCode::End), Some(Input::Guitar(GuitarButton::Pause)));
        assert!(keys.iter().all(|(_, i)| i.is_guitar()));
        let mut codes: Vec<String> = keys.iter().map(|(c, _)| format!("{c:?}")).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), keys.len());
        assert_eq!(Input::Button(Button::TempoUp).on_guitar(), Input::Guitar(GuitarButton::Right));
    }

    #[test]
    fn midi_notes_play_the_piano_s_keys_only() {
        let mut m = MidiKeys::default();
        // Middle C (60) is key 19, C5 (72) key 31; F2 (41) and E5 (76) are
        // the ends, and notes past them play nothing.
        assert_eq!(m.note(60, true, 0), Some((19, true)));
        assert_eq!(m.note(72, true, 0), Some((31, true)));
        assert_eq!((m.note(41, true, 0), m.note(41, false, 0)), (Some((0, true)), Some((0, false))));
        assert_eq!((m.note(76, true, 0), m.note(76, false, 0)), (Some((35, true)), Some((35, false))));
        for note in [0, 40, 77, 84, 127] {
            assert_eq!(m.note(note, true, 0), None, "{note} has no key");
        }
        assert_eq!(m.note(72, true, 0), None, "already down");
        assert_eq!(m.note(72, false, 0), None, "still held once");
        assert_eq!(m.note(72, false, 0), Some((31, false)));
        assert_eq!(m.note(60, true, 1), Some((31, true)), "an octave up");
        assert_eq!(m.note(76, true, 1), None, "an octave up, past the top");
        assert_eq!(m.release_all(), vec![19, 31]);
        assert_eq!(m.note(60, false, 0), None, "already let go");
    }
}
