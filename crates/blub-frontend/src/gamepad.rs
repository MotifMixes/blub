//! Game controllers on desktop (gilrs: XInput / DirectInput / evdev / IOKit).
//!
//! Fixed layout, positional (Xbox names), for the I Can Play Piano's
//! buttons: D-pad up / down (or the left stick) = Up / Down, A = Enter,
//! B = Home, LB / RB = Tempo down / up, Start = Pause, X = Change
//! instrument, Y = Hands. Guide opens the menu.

use crate::piano::Input;
use blub_piano::Button;
use gilrs::{Axis, Button as Pad, EventType, Gilrs};

/// Stick deflection that counts as a direction.
const STICK: f32 = 0.5;

pub struct Gamepads {
    gilrs: Gilrs,
    /// Pad-held state of each button, so the stick and the D-pad don't
    /// send the same press twice.
    held: [bool; Button::ALL.len()],
}

/// One change for the app to apply.
pub enum PadEvent {
    Button(Input, bool),
    /// The Guide button: opens the menu.
    Menu,
    Connected(String),
    Disconnected(String),
}

impl Gamepads {
    pub fn open() -> Option<Self> {
        match Gilrs::new() {
            Ok(gilrs) => Some(Self { gilrs, held: Default::default() }),
            Err(e) => {
                tracing::warn!(target: "blub::input", "no gamepad support: {e}");
                None
            }
        }
    }

    /// Applies pending controller events through `emit`.
    pub fn poll(&mut self, mut emit: impl FnMut(PadEvent)) {
        while let Some(ev) = self.gilrs.next_event() {
            let name = || self.gilrs.gamepad(ev.id).name().to_string();
            match ev.event {
                EventType::Connected => emit(PadEvent::Connected(name())),
                EventType::Disconnected => emit(PadEvent::Disconnected(name())),
                EventType::ButtonPressed(Pad::Mode, _) => emit(PadEvent::Menu),
                EventType::ButtonPressed(b, _) | EventType::ButtonReleased(b, _) => {
                    let pressed = matches!(ev.event, EventType::ButtonPressed(..));
                    if let Some(button) = map_button(b) {
                        self.set(button, pressed, &mut emit);
                    }
                }
                EventType::AxisChanged(Axis::LeftStickY, v, _) => {
                    self.set(Button::ScrollDown, v < -STICK, &mut emit);
                    self.set(Button::ScrollUp, v > STICK, &mut emit);
                }
                _ => {}
            }
        }
    }

    fn set(&mut self, b: Button, pressed: bool, emit: &mut impl FnMut(PadEvent)) {
        let i = Button::ALL.iter().position(|x| *x == b).unwrap();
        if self.held[i] != pressed {
            self.held[i] = pressed;
            emit(PadEvent::Button(Input::Button(b), pressed));
        }
    }
}

/// The piano button a controller button is.
fn map_button(b: Pad) -> Option<Button> {
    Some(match b {
        Pad::DPadUp => Button::ScrollUp,
        Pad::DPadDown => Button::ScrollDown,
        Pad::South => Button::Enter,
        Pad::East => Button::Home,
        Pad::LeftTrigger => Button::TempoDown,
        Pad::RightTrigger => Button::TempoUp,
        Pad::Start => Button::Pause,
        Pad::West => Button::ChangeInstrument,
        Pad::North => Button::CycleHands,
        _ => return None,
    })
}
