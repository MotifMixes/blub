//! The Fisher-Price I Can Play Piano (2006), a TV keyboard with 36
//! light-up keys (three octaves, F to E), built on a Sunplus SPG24x: a
//! 16-bit unSP CPU at 27 MHz with the SPG2xx video, sound and I/O units.
//! The console has no ROM of its own: each cartridge (an 8 MB ROM) fills the
//! CPU's whole 22-bit ROM space and holds everything, menus included (as in
//! MAME's icanpian driver).
//!
//! ```no_run
//! # let cart = Vec::new();
//! let mut piano = blub_piano::Piano::new(&cart);
//! let mut frame = vec![0u32; blub_piano::SCREEN_W * blub_piano::SCREEN_H];
//! loop {
//!     piano.run_frame();
//!     piano.render(&mut frame);
//! }
//! ```

mod bus;
pub mod cpu;
#[allow(dead_code)]
mod eeprom;
mod sio_audio;
mod sound;
mod video;

pub use bus::{CLOCK, FRAME};
pub use sound::OUTPUT_RATE;
pub use video::{SCREEN_H, SCREEN_W};

use bus::Bus;
use cpu::Cpu;
use serde::{Deserialize, Serialize};

/// A cartridge ROM's size in bytes (smaller dumps are refused, as MAME
/// does: every known cartridge is 8 MB).
pub const CART_SIZE: usize = 0x80_0000;

/// The keyboard's keys: 0 is the lowest F, 35 the highest E.
pub const KEYS: usize = 36;
/// The MIDI note of key 0: the middle octave's C is middle C (60), so the
/// lowest F is F2 (41).
pub const MIDI_KEY0: u8 = 41;

/// The key a MIDI note plays: notes outside the keyboard's three octaves
/// are moved by whole octaves into it.
pub fn midi_key(note: u8) -> usize {
    let mut n = note as i32 - MIDI_KEY0 as i32;
    while n < 0 {
        n += 12;
    }
    while n >= KEYS as i32 {
        n -= 12;
    }
    n as usize
}

/// The console's buttons, as (octave scan, port C bit) from MAME's
/// icanpian driver. MAME's names; what each does is UNVERIFIED here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Button {
    ChangeInstrument,
    CycleHands,
    /// The four display mode buttons (1-4).
    Display1,
    Display2,
    Display3,
    Display4,
    TempoUp,
    TempoDefault,
    TempoDown,
    Pause,
    Metronome,
    ScrollUp,
    ScrollDown,
    Enter,
    Home,
}

impl Button {
    pub const ALL: [Button; 15] = [
        Button::ChangeInstrument,
        Button::CycleHands,
        Button::Display1,
        Button::Display2,
        Button::Display3,
        Button::Display4,
        Button::TempoUp,
        Button::TempoDefault,
        Button::TempoDown,
        Button::Pause,
        Button::Metronome,
        Button::ScrollUp,
        Button::ScrollDown,
        Button::Enter,
        Button::Home,
    ];

    fn line(self) -> (usize, u16) {
        match self {
            Button::ChangeInstrument => (0, 0x0001),
            Button::CycleHands => (0, 0x0002),
            Button::Display1 => (0, 0x0004),
            Button::Display2 => (0, 0x0008),
            Button::Display3 => (0, 0x0010),
            Button::Display4 => (0, 0x0020),
            Button::TempoUp => (1, 0x0001),
            Button::TempoDefault => (1, 0x0002),
            Button::TempoDown => (1, 0x0004),
            Button::Pause => (1, 0x0008),
            Button::Metronome => (1, 0x0010),
            Button::ScrollUp => (2, 0x0002),
            Button::ScrollDown => (2, 0x0004),
            Button::Enter => (2, 0x0008),
            Button::Home => (2, 0x0010),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Button::ChangeInstrument => "Change instrument",
            Button::CycleHands => "Hands",
            Button::Display1 => "Display 1",
            Button::Display2 => "Display 2",
            Button::Display3 => "Display 3",
            Button::Display4 => "Display 4",
            Button::TempoUp => "Tempo up",
            Button::TempoDefault => "Tempo normal",
            Button::TempoDown => "Tempo down",
            Button::Pause => "Pause",
            Button::Metronome => "Metronome",
            Button::ScrollUp => "Up",
            Button::ScrollDown => "Down",
            Button::Enter => "Enter",
            Button::Home => "Home",
        }
    }
}

/// The power switch: octave 0's port C bit 11 (MAME: "Power Switch").
const POWER_BIT: u16 = 0x0800;
/// The slider: octave 2's port C bit 0 set is Keyboard Mode (no TV
/// picture), clear TV Mode.
const KEYBOARD_MODE_BIT: u16 = 0x0001;

/// The save state format's version (bumped when the layout changes).
const STATE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Piano {
    pub cpu: Cpu,
    bus: Bus,
    instructions: u64,
}

/// Words from a ROM dump (stored low byte first).
fn words_le(data: &[u8]) -> Vec<u16> {
    data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

/// Whether `data` looks like an I Can Play Piano cartridge: 8 MB or more.
pub fn is_cartridge(data: &[u8]) -> bool {
    data.len() >= CART_SIZE
}

impl Piano {
    /// A piano with cartridge `cart` (its ROM dump) in, switched on.
    pub fn new(cart: &[u8]) -> Self {
        let mut rom = words_le(cart);
        rom.resize(CART_SIZE / 2, 0);
        let mut p = Piano { cpu: Cpu::new(), bus: Bus::new(rom), instructions: 0 };
        p.reset();
        p
    }

    pub fn reset(&mut self) {
        self.cpu.reset(&mut self.bus);
    }

    pub fn instructions(&self) -> u64 {
        self.instructions
    }

    pub fn cycles(&self) -> u64 {
        self.bus.now
    }

    pub fn frames(&self) -> u64 {
        self.bus.frames
    }

    /// The program turned the console off.
    pub fn powered_off(&self) -> bool {
        self.bus.powered_off
    }

    /// Runs one instruction (taking an interrupt after it if one is due).
    #[inline]
    fn step(&mut self) -> bool {
        self.bus.ds = self.cpu.ds();
        let cycles = self.cpu.step(&mut self.bus);
        if self.bus.ds_written {
            self.bus.ds_written = false;
            self.cpu.set_ds(self.bus.ds);
        }
        self.instructions += 1;
        self.bus.now += cycles as u64;
        if self.bus.now >= self.bus.next_event {
            return self.bus.service();
        }
        false
    }

    /// Research: runs one instruction.
    pub fn step_one(&mut self) {
        self.step();
    }

    /// Runs until the next vertical blank starts (one frame).
    pub fn run_frame(&mut self) {
        let limit = self.bus.now + 2 * FRAME;
        while !self.step() {
            if self.bus.now > limit {
                break;
            }
        }
    }

    /// Runs `cycles` CPU cycles.
    pub fn run(&mut self, cycles: u64) {
        let end = self.bus.now + cycles;
        while self.bus.now < end {
            self.step();
        }
    }

    /// The picture: SCREEN_W x SCREEN_H pixels, 0x00RRGGBB.
    pub fn render(&self, out: &mut [u32]) {
        out.copy_from_slice(&self.bus.video.frame);
    }

    /// Key `key` (0-35, see KEYS) pressed or released.
    pub fn key(&mut self, key: usize, down: bool) {
        if key >= KEYS {
            return;
        }
        let (g, bit) = (key / 12, 1u16 << (key % 12));
        if down {
            self.bus.keys[g] |= bit;
        } else {
            self.bus.keys[g] &= !bit;
        }
    }

    /// All keys up.
    pub fn release_keys(&mut self) {
        self.bus.keys = [0; 3];
    }

    pub fn button(&mut self, button: Button, down: bool) {
        let (g, bit) = button.line();
        if down {
            self.bus.buttons[g] |= bit;
        } else {
            self.bus.buttons[g] &= !bit;
        }
    }

    /// The power switch's line (MAME holds it low while on; UNVERIFIED).
    pub fn set_power_line(&mut self, high: bool) {
        if high {
            self.bus.buttons[0] |= POWER_BIT;
        } else {
            self.bus.buttons[0] &= !POWER_BIT;
        }
    }

    /// The TV / Keyboard Mode slider (Keyboard Mode: no TV picture, the
    /// keys play notes through the speaker).
    pub fn set_keyboard_mode(&mut self, keyboard: bool) {
        if keyboard {
            self.bus.buttons[2] |= KEYBOARD_MODE_BIT;
        } else {
            self.bus.buttons[2] &= !KEYBOARD_MODE_BIT;
        }
    }

    /// The machine's state (without the cartridge).
    pub fn save_state(&self) -> Vec<u8> {
        let mut out = STATE_VERSION.to_le_bytes().to_vec();
        out.extend(bincode::serialize(self).expect("serialise the machine"));
        out
    }

    /// Restores a state from `save_state`, keeping the cartridge and what
    /// the player holds.
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
        let version = state.get(..4).map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
        if version != Some(STATE_VERSION) {
            return Err("this state is from another version of the piano model".into());
        }
        let mut p: Piano = bincode::deserialize(&state[4..]).map_err(|e| e.to_string())?;
        let (b, old) = (&mut p.bus, &mut self.bus);
        std::mem::swap(&mut b.rom, &mut old.rom);
        std::mem::swap(&mut b.video.frame, &mut old.video.frame);
        b.keys = old.keys;
        b.buttons = old.buttons;
        *self = p;
        Ok(())
    }

    /// The sound made since the last call: stereo, interleaved, at
    /// OUTPUT_RATE samples a second.
    pub fn take_audio(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.bus.sound.out)
    }

    /// Research: logs sound channel starts (taken with `take_sound_log`).
    pub fn trace_sound(&mut self) {
        self.bus.sound.trace = Some(Vec::new());
    }

    pub fn take_sound_log(&mut self) -> Vec<String> {
        self.bus.sound.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Research: word `addr` as the CPU would read it, without side effects
    /// for RAM and ROM.
    pub fn peek(&self, addr: u32) -> u16 {
        self.bus.peek(addr)
    }

    pub fn io_regs(&self) -> &[u16] {
        &self.bus.io
    }

    pub fn video_regs(&self) -> &[u16] {
        &self.bus.video.regs
    }

    pub fn line(&self) -> u32 {
        self.bus.line
    }
}
