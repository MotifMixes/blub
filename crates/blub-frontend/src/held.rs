//! The consoles' keys and buttons held by each input source (keyboard, the
//! console view's controls, gamepads, a MIDI keyboard). An input is down
//! while any source holds it, so letting go of a virtual key never releases
//! a PC key still held, or the other way round.

use crate::piano::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Keyboard,
    Pointer,
    Gamepad,
    Midi,
}

#[derive(Debug, Default, Clone)]
pub struct Held {
    masks: [u128; 4],
}

fn bit(i: Input) -> u128 {
    1 << i.index()
}

const _: () = assert!(Input::COUNT <= 128, "an input a bit");

impl Held {
    fn any(&self) -> u128 {
        self.masks.iter().fold(0, |a, m| a | m)
    }

    /// Sets `i` held or not by `source`; returns its new state when that
    /// changed what the machine sees.
    pub fn set(&mut self, source: Source, i: Input, down: bool) -> Option<bool> {
        let before = self.any() & bit(i) != 0;
        let m = &mut self.masks[source as usize];
        if down {
            *m |= bit(i);
        } else {
            *m &= !bit(i);
        }
        let after = self.any() & bit(i) != 0;
        (before != after).then_some(after)
    }

    pub fn pressed(&self, i: Input) -> bool {
        self.any() & bit(i) != 0
    }

    /// Whether anything is held at all.
    pub fn any_pressed(&self) -> bool {
        self.any() != 0
    }

    /// Lets go of everything `source` holds; returns what is now up.
    pub fn release(&mut self, source: Source) -> Vec<Input> {
        let before = self.any();
        self.masks[source as usize] = 0;
        let up = before & !self.any();
        Input::all().filter(|&i| up & bit(i) != 0).collect()
    }

    pub fn clear(&mut self) {
        self.masks = [0; 4];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blub_piano::Button;

    #[test]
    fn an_input_stays_down_while_any_source_holds_it() {
        let mut h = Held::default();
        let enter = Input::Button(Button::Enter);
        assert_eq!(h.set(Source::Keyboard, enter, true), Some(true));
        assert_eq!(h.set(Source::Pointer, enter, true), None, "already down");
        assert_eq!(h.set(Source::Pointer, enter, false), None, "the key still holds it");
        assert!(h.pressed(enter));
        assert_eq!(h.set(Source::Keyboard, enter, false), Some(false));
        h.set(Source::Pointer, Input::Key(3), true);
        h.set(Source::Keyboard, Input::Key(35), true);
        h.set(Source::Midi, Input::Key(35), true);
        assert_eq!(h.release(Source::Pointer), [Input::Key(3)]);
        assert_eq!(h.release(Source::Keyboard), Vec::<Input>::new(), "MIDI still holds the top key");
        assert!(h.pressed(Input::Key(35)) && h.any_pressed());
        h.clear();
        assert!(!h.pressed(Input::Key(35)) && !h.any_pressed());
    }
}
