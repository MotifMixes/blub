//! MIDI keyboards for the I Can Play Piano (desktop): a MIDI input port
//! (midir: WinMM on Windows, CoreMIDI on macOS, ALSA on Linux) opened while
//! a game runs, its notes passed on as `Event`s. The port is the one chosen
//! in Input > MIDI input, or the first one there is ("Automatic"); while
//! none is open the ports are looked at again every couple of seconds, so a
//! keyboard plugged in later is found.

use midir::{Ignore, MidiInput, MidiInputConnection};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// How often the ports are listed again.
const RESCAN: Duration = Duration::from_secs(2);
/// The choice that opens the first port there is.
pub const AUTO: &str = "auto";
/// The choice that opens none.
pub const NONE: &str = "none";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A note (MIDI number) pressed (true) or let go.
    Note(u8, bool),
    /// All notes off (controller 123, or 120: all sound off).
    AllOff,
}

pub struct Midi {
    /// The port open now (its name) and its connection.
    conn: Option<(String, MidiInputConnection<()>)>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    /// The ports seen at the last look, and when that was.
    ports: Vec<String>,
    scanned: Option<Instant>,
}

impl Default for Midi {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { conn: None, tx, rx, ports: Vec::new(), scanned: None }
    }
}

/// What one MIDI message means for the keys (None: nothing).
pub fn decode(msg: &[u8]) -> Option<Event> {
    let (&status, data) = msg.split_first()?;
    match (status & 0xF0, data) {
        (0x90, [note, vel, ..]) => Some(Event::Note(*note & 0x7F, *vel > 0)),
        (0x80, [note, ..]) => Some(Event::Note(*note & 0x7F, false)),
        (0xB0, [120 | 123, ..]) => Some(Event::AllOff),
        _ => None,
    }
}

impl Midi {
    /// The ports seen at the last look (for the menu).
    pub fn ports(&self) -> &[String] {
        &self.ports
    }

    /// The port open now.
    pub fn connected(&self) -> Option<&str> {
        self.conn.as_ref().map(|(n, _)| n.as_str())
    }

    /// Lists the ports again now (the menu is being opened).
    pub fn rescan(&mut self) {
        self.scanned = None;
    }

    fn list(input: &MidiInput) -> Vec<String> {
        input.ports().iter().filter_map(|p| input.port_name(p).ok()).collect()
    }

    /// Call every frame: keeps the chosen port (`want`: `AUTO`, `NONE` or a
    /// port name) open while `active` (a game runs), closed otherwise.
    /// Returns the notes played since the last call, and the name of a port
    /// just opened.
    pub fn poll(&mut self, want: &str, active: bool, events: &mut Vec<Event>) -> Option<String> {
        events.extend(self.rx.try_iter());
        if !active || want == NONE {
            if let Some((name, c)) = self.conn.take() {
                c.close();
                tracing::info!(target: "blub::midi", "closed {name}");
            }
            if !active {
                return None;
            }
        }
        if self.scanned.is_some_and(|t| t.elapsed() < RESCAN) {
            return None;
        }
        self.scanned = Some(Instant::now());
        let input = match MidiInput::new("Blub") {
            Ok(i) => i,
            Err(e) => {
                tracing::debug!(target: "blub::midi", "no MIDI: {e}");
                return None;
            }
        };
        self.ports = Self::list(&input);
        // The open port went away, or another one is chosen now.
        let keep = self.conn.as_ref().is_some_and(|(n, _)| self.ports.contains(n) && (want == AUTO || want == n));
        if !keep && let Some((name, c)) = self.conn.take() {
            c.close();
            tracing::info!(target: "blub::midi", "closed {name}");
        }
        if self.conn.is_some() || want == NONE {
            return None;
        }
        let target = if want == AUTO { self.ports.first().cloned() } else { self.ports.iter().find(|p| *p == want).cloned() };
        let name = target?;
        let mut input = input;
        input.ignore(Ignore::All);
        let port = input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(name.as_str()))?;
        let tx = self.tx.clone();
        let opened = input.connect(
            &port,
            "blub-in",
            move |_, msg, _| {
                if let Some(e) = decode(msg) {
                    let _ = tx.send(e);
                }
            },
            (),
        );
        match opened {
            Ok(c) => {
                tracing::info!(target: "blub::midi", "opened {name}");
                self.conn = Some((name.clone(), c));
                Some(name)
            }
            Err(e) => {
                tracing::warn!(target: "blub::midi", "opening {name}: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_all_notes_off() {
        assert_eq!(decode(&[0x90, 60, 100]), Some(Event::Note(60, true)));
        assert_eq!(decode(&[0x93, 60, 0]), Some(Event::Note(60, false)), "velocity 0 is a release");
        assert_eq!(decode(&[0x80, 61, 64]), Some(Event::Note(61, false)));
        assert_eq!(decode(&[0xB0, 123, 0]), Some(Event::AllOff));
        assert_eq!(decode(&[0xB2, 120, 0]), Some(Event::AllOff));
        assert_eq!(decode(&[0xB0, 7, 100]), None, "volume");
        assert_eq!(decode(&[0xF8]), None, "clock");
        assert_eq!(decode(&[]), None);
    }
}
