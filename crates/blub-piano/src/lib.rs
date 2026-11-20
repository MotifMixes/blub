//! The Fisher-Price I Can Play Piano (2006), a TV keyboard with 36
//! light-up keys (three octaves, F to E), built on a Sunplus SPG24x: a
//! 16-bit unSP CPU at 27 MHz with the SPG2xx video, sound and I/O units.
//! The console has no ROM of its own: each cartridge (an 8 MB ROM) fills the
//! CPU's whole 22-bit ROM space and holds everything, menus included (as in
//! MAME's icanpian driver).
//!
//! The same model runs the I Can Play Guitar (2007), the same chip with a
//! guitar's controls (44 fret buttons, six strings, seven buttons) scanned
//! on six lines instead of three: `Piano::new` tells the cartridges apart
//! (`detect`) and the guitar's inputs are `fret`, `pluck` and
//! `guitar_button`.
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

/// The key a MIDI note plays: None outside the keyboard's three octaves
/// (F2 to E5), which have no key.
pub fn midi_key(note: u8) -> Option<usize> {
    let n = (note as usize).checked_sub(MIDI_KEY0 as usize)?;
    (n < KEYS).then_some(n)
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

/// The guitar's six strings: 0 is string 1 (the high E), 5 string 6 (the
/// low E).
pub const STRINGS: usize = 6;
/// Frets on the guitar's neck: 12 on the outer strings (1 and 6), 5 on
/// the four between (see `has_fret`).
pub const FRETS: usize = 12;
/// The guitar's fret buttons in all.
pub const FRET_BUTTONS: usize = 44;
/// The open strings' notes (MIDI), string 1 to 6: the toy plays in a
/// guitar's standard tuning (the intervals are measured from the notes the
/// cartridges play; the octave is assumed).
pub const GUITAR_OPEN: [u8; STRINGS] = [64, 59, 55, 50, 45, 40];

/// Whether the guitar's neck has a button for string `string` (0-5) at
/// fret `fret` (1-12): the outer strings have all twelve, the four
/// between them the first five (44 buttons).
pub fn has_fret(string: usize, fret: usize) -> bool {
    fret_line(string, fret).is_some()
}

/// A fret button's scan line: (scan, port A bit). Measured: scan n reads
/// string n+1's frets on port A, fret f on bit f-1, but for the outer
/// strings' frets 7 and 8, which sit on the spare bit 5 of the inner
/// strings' scans, and their frets 9-12, on bits 6-9.
fn fret_line(string: usize, fret: usize) -> Option<(usize, u16)> {
    let bit = |b: usize| 1u16 << b;
    match (string, fret) {
        (0 | 5, 1..=6) => Some((string, bit(fret - 1))),
        (0, 7) => Some((1, bit(5))),
        (0, 8) => Some((2, bit(5))),
        (5, 7) => Some((4, bit(5))),
        (5, 8) => Some((3, bit(5))),
        (0 | 5, 9..=12) => Some((string, bit(fret - 3))),
        (1..=4, 1..=5) => Some((string, bit(fret - 1))),
        _ => None,
    }
}

/// The guitar's buttons (measured; see docs/hardware.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GuitarButton {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Home,
    Pause,
}

impl GuitarButton {
    pub const ALL: [GuitarButton; 7] = [
        GuitarButton::Up,
        GuitarButton::Down,
        GuitarButton::Left,
        GuitarButton::Right,
        GuitarButton::Enter,
        GuitarButton::Home,
        GuitarButton::Pause,
    ];

    /// (scan, port C bit). Left and right are a guess: both lines move a
    /// cursor like the arrows do, and no screen found yet tells them apart.
    fn line(self) -> (usize, u16) {
        match self {
            GuitarButton::Up => (2, 0x0002),
            GuitarButton::Down => (4, 0x0002),
            GuitarButton::Left => (1, 0x0001),
            GuitarButton::Right => (1, 0x0004),
            GuitarButton::Enter => (3, 0x0002),
            GuitarButton::Home => (3, 0x0001),
            GuitarButton::Pause => (2, 0x0001),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GuitarButton::Up => "Up",
            GuitarButton::Down => "Down",
            GuitarButton::Left => "Left",
            GuitarButton::Right => "Right",
            GuitarButton::Enter => "Enter",
            GuitarButton::Home => "Home",
            GuitarButton::Pause => "Pause",
        }
    }
}

/// The guitar's strings: scan 5's port C bits 3-8 (string 1 to 6).
const STRUM_BIT: u16 = 0x0008;
/// How long a plucked string's line stays up.
const PLUCK_FRAMES: u8 = 2;
/// The guitar's switch for playing without the TV: scan 5's port C bit 10
/// (set: no picture, the strings still sound).
const GUITAR_ONLY_BIT: u16 = 0x0400;

/// The power switch: octave 0's port C bit 11 (MAME: "Power Switch").
const POWER_BIT: u16 = 0x0800;
/// The slider: octave 2's port C bit 0 set is Keyboard Mode (no TV
/// picture), clear TV Mode.
const KEYBOARD_MODE_BIT: u16 = 0x0001;

/// The hands' lights: port C bits 7 (left) and 8 (right), lit while
/// that hand plays a song (seen in Piano Favorites: a song starts with
/// the right hand's, and the hands button steps right, left, both).
const LEFT_HAND_LIGHT: u16 = 0x0080;
const RIGHT_HAND_LIGHT: u16 = 0x0100;

/// The save state format's version (bumped when the layout changes).
const STATE_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
pub struct Piano {
    pub cpu: Cpu,
    bus: Bus,
    instructions: u64,
    /// Frames left in each plucked string's pulse (see `pluck`).
    #[serde(skip)]
    plucked: [u8; STRINGS],
}

/// The console a cartridge is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Model {
    /// I Can Play Piano (2006).
    Piano,
    /// I Can Play Guitar (2007).
    Guitar,
}

impl Model {
    pub fn name(self) -> &'static str {
        match self {
            Model::Piano => "I Can Play Piano",
            Model::Guitar => "I Can Play Guitar",
        }
    }
}

/// Which console cartridge `cart` is for. The two consoles' cartridges have
/// the same size and layout, but their programs set the input scan up
/// differently at power-on: the piano drives port A bits 12-14 (its three
/// octaves), the guitar bits 10-15 (its six scan lines). So the cartridge
/// runs (on a piano) until it first sets port A's direction register.
pub fn detect(cart: &[u8]) -> Model {
    let mut rom = words_le(cart);
    rom.resize(CART_SIZE / 2, 0);
    let mut p = Piano { cpu: Cpu::new(), bus: Bus::new(rom), instructions: 0, plucked: [0; STRINGS] };
    p.reset();
    for _ in 0..200_000 {
        p.step();
        match p.bus.io[3] {
            0 => {}
            0xfc00 => return Model::Guitar,
            _ => return Model::Piano,
        }
    }
    Model::Piano
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
    /// A console with cartridge `cart` (its ROM dump) in, switched on: a
    /// piano or a guitar, whichever the cartridge is for (see `detect`).
    pub fn new(cart: &[u8]) -> Self {
        let model = detect(cart);
        Self::with_model(cart, model)
    }

    /// A console of model `model` with cartridge `cart` in, switched on.
    pub fn with_model(cart: &[u8], model: Model) -> Self {
        let mut rom = words_le(cart);
        rom.resize(CART_SIZE / 2, 0);
        let mut p = Piano { cpu: Cpu::new(), bus: Bus::new(rom), instructions: 0, plucked: [0; STRINGS] };
        p.bus.guitar = model == Model::Guitar;
        p.reset();
        p
    }

    pub fn model(&self) -> Model {
        if self.bus.guitar { Model::Guitar } else { Model::Piano }
    }

    /// Research: what the guitar's scan `group` (0-5) reads on ports A and C.
    pub fn set_scan_lines(&mut self, group: usize, a: u16, c: u16) {
        self.bus.keys[group] = a;
        self.bus.buttons[group] = c;
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
        self.pluck_tick();
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

    /// The sprite table (4 words per sprite) and the video registers, for
    /// debugging tools.
    pub fn video_state(&self) -> (&[u16], &[u16]) {
        (&self.bus.video.sprites, &self.bus.video.regs)
    }

    /// The picture: SCREEN_W x SCREEN_H pixels, 0x00RRGGBB.
    pub fn render(&self, out: &mut [u32]) {
        out.copy_from_slice(&self.bus.video.frame);
    }

    /// Sets or clears scan line (`group`, `bit`) on port A (`port_a`) or C.
    fn set_line(&mut self, port_a: bool, (group, bit): (usize, u16), down: bool) {
        let lines = if port_a { &mut self.bus.keys } else { &mut self.bus.buttons };
        if down {
            lines[group] |= bit;
        } else {
            lines[group] &= !bit;
        }
    }

    /// Key `key` (0-35, see KEYS) pressed or released (the piano's).
    pub fn key(&mut self, key: usize, down: bool) {
        if key >= KEYS || self.bus.guitar {
            return;
        }
        self.set_line(true, (key / 12, 1u16 << (key % 12)), down);
    }

    /// All keys (or frets) up.
    pub fn release_keys(&mut self) {
        self.bus.keys = [0; 6];
    }

    /// One of the piano's buttons.
    pub fn button(&mut self, button: Button, down: bool) {
        if !self.bus.guitar {
            self.set_line(false, button.line(), down);
        }
    }

    /// The guitar's fret button on string `string` (0-5, see STRINGS) at
    /// fret `fret` (1-12; see `has_fret`), pressed or released.
    pub fn fret(&mut self, string: usize, fret: usize, down: bool) {
        if let (true, Some(line)) = (self.bus.guitar, fret_line(string, fret)) {
            self.set_line(true, line, down);
        }
    }

    /// The guitar's string `string` (0-5) plucked: its line pulses for
    /// PLUCK_FRAMES frames. The cartridges play the note when the line
    /// drops again, and only after a short pulse (under about 24 frames
    /// held, nothing plays: measured).
    pub fn pluck(&mut self, string: usize) {
        if self.bus.guitar && string < STRINGS {
            self.set_line(false, (5, STRUM_BIT << string), true);
            self.plucked[string] = PLUCK_FRAMES;
        }
    }

    /// Lets go of the plucked strings whose pulse is over.
    fn pluck_tick(&mut self) {
        for string in 0..STRINGS {
            if self.plucked[string] > 0 {
                self.plucked[string] -= 1;
                if self.plucked[string] == 0 {
                    self.set_line(false, (5, STRUM_BIT << string), false);
                }
            }
        }
    }

    /// One of the guitar's buttons.
    pub fn guitar_button(&mut self, button: GuitarButton, down: bool) {
        if self.bus.guitar {
            self.set_line(false, button.line(), down);
        }
    }

    /// The power switch's line (MAME holds it low while on; UNVERIFIED).
    /// The piano's only.
    pub fn set_power_line(&mut self, high: bool) {
        if !self.bus.guitar {
            self.set_line(false, (0, POWER_BIT), high);
        }
    }

    /// The TV / Keyboard Mode slider (Keyboard Mode: no TV picture, the
    /// keys play notes through the speaker). On the guitar, its switch for
    /// playing without the TV (no picture; the strings still sound).
    pub fn set_keyboard_mode(&mut self, keyboard: bool) {
        let line = if self.bus.guitar { (5, GUITAR_ONLY_BIT) } else { (2, KEYBOARD_MODE_BIT) };
        self.set_line(false, line, keyboard);
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
        b.guitar = old.guitar;
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

    /// The hands' lights on the panel: (left, right) lit.
    pub fn hand_lights(&self) -> (bool, bool) {
        let out = self.bus.port_c_out;
        (out & LEFT_HAND_LIGHT != 0, out & RIGHT_HAND_LIGHT != 0)
    }

    /// Research: logs changes to what the program drives out of the I/O
    /// ports (taken with `take_port_log`).
    pub fn trace_ports(&mut self) {
        self.bus.port_log = Some(Vec::new());
    }

    pub fn take_port_log(&mut self) -> Vec<String> {
        self.bus.port_log.as_mut().map(std::mem::take).unwrap_or_default()
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

    pub fn sprite_regs(&self) -> &[u16] {
        &self.bus.video.sprites
    }

    pub fn line(&self) -> u32 {
        self.bus.line
    }
}
