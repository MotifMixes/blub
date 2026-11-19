//! The emulator on its own thread, so emulation gets a whole core and the
//! window stays at the display's frame rate even when emulation runs slower
//! than real time. The thread runs the piano a frame at a time, paced to
//! real time (times the speed setting); the UI thread locks the machine
//! briefly for input, save states and rewind snapshots, and takes the
//! latest picture.

use crate::audio_out::{AudioSink, Resampler};
use blub_piano::{Piano, SCREEN_H, SCREEN_W};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The console's frame rate (NTSC).
pub const FPS: f64 = 60.0;
/// Drop lag beyond this (the game then runs slower than real time rather
/// than catching up in a burst).
const MAX_LAG: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Control {
    pub running: bool,
    /// 1.0 = real time; infinity = as fast as possible.
    pub speed: f64,
    /// Volume (0 = muted).
    pub gain: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Emulation speed over the last second, percent of real time.
    pub speed_percent: f64,
    /// The program turned the console off.
    pub powered_off: bool,
    /// The hands' lights on the panel: (left, right).
    pub hands: (bool, bool),
}

struct Shared {
    piano: Mutex<Piano>,
    control: Mutex<(Control, bool)>, // (control, resync pacing)
    wake: Condvar,
    /// Sound goes straight from this thread to the output queue, so it does
    /// not depend on the window being redrawn.
    sink: Mutex<Option<AudioSink>>,
    stats: Mutex<Stats>,
    /// The latest picture (SCREEN_W x SCREEN_H, 0x00RRGGBB).
    frame: Mutex<Vec<u32>>,
    quit: AtomicBool,
    /// UI-thread lock requests in progress. std's mutex is not fair: the
    /// emulation thread, re-locking right after each frame, could otherwise
    /// keep the UI waiting.
    ui_waiting: AtomicUsize,
}

pub struct Emu {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Emu {
    pub fn start(piano: Piano) -> Self {
        let shared = Arc::new(Shared {
            piano: Mutex::new(piano),
            control: Mutex::new((Control { running: false, speed: 1.0, gain: 1.0 }, true)),
            wake: Condvar::new(),
            sink: Mutex::new(None),
            stats: Mutex::new(Stats::default()),
            frame: Mutex::new(vec![0; SCREEN_W * SCREEN_H]),
            quit: AtomicBool::new(false),
            ui_waiting: AtomicUsize::new(0),
        });
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("blub-emu".into())
            .spawn(move || {
                crate::perf::raise_thread_priority();
                run(&worker)
            })
            .expect("spawning the emulation thread");
        Self { shared, thread: Some(thread) }
    }

    /// The machine, for the UI thread (held briefly: the emulation thread
    /// waits meanwhile).
    pub fn lock(&self) -> MutexGuard<'_, Piano> {
        self.shared.ui_waiting.fetch_add(1, Ordering::SeqCst);
        let g = self.shared.piano.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.ui_waiting.fetch_sub(1, Ordering::SeqCst);
        g
    }

    /// Copies the latest picture into `out`.
    pub fn frame(&self, out: &mut [u32]) {
        out.copy_from_slice(&self.shared.frame.lock().unwrap());
    }

    pub fn set_control(&self, c: Control) {
        let mut g = self.shared.control.lock().unwrap();
        if g.0 != c {
            // Reset pacing when speed changes so the machine does not stall
            // waiting for real time to catch up to where it was at the old
            // speed.
            if g.0.speed != c.speed {
                g.1 = true;
            }
            g.0 = c;
            self.shared.wake.notify_all();
        }
    }

    /// Restart real-time pacing (after a reset, a loaded state or a pause).
    pub fn resync(&self) {
        self.shared.control.lock().unwrap().1 = true;
        self.shared.wake.notify_all();
    }

    pub fn set_audio_sink(&self, sink: Option<AudioSink>) {
        *self.shared.sink.lock().unwrap() = sink;
    }

    pub fn stats(&self) -> Stats {
        self.shared.stats.lock().unwrap().clone()
    }

    /// Stops the thread.
    pub fn stop(mut self) {
        self.join();
    }

    fn join(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Emu {
    fn drop(&mut self) {
        self.join();
    }
}

fn run(s: &Shared) {
    // Pacing: when frame 0 of this stretch ran, and frames run since.
    let mut pace: Option<(Instant, u64)> = None;
    // The speed meter: since when, frames run.
    let mut meter = (Instant::now(), 0u64);
    let mut resample = Resampler::new(blub_piano::OUTPUT_RATE);
    let mut stereo = Vec::new();
    let mut picture = vec![0u32; SCREEN_W * SCREEN_H];
    while !s.quit.load(Ordering::Relaxed) {
        let control = {
            let mut g = s.control.lock().unwrap();
            if !g.0.running {
                pace = None;
                s.stats.lock().unwrap().speed_percent = 0.0;
                let _ = s.wake.wait_timeout(g, Duration::from_millis(50)).unwrap();
                meter = (Instant::now(), 0);
                continue;
            }
            if std::mem::take(&mut g.1) {
                pace = None;
            }
            g.0
        };
        // Real time times the speed: wait for this frame's moment.
        if control.speed.is_finite() {
            let (start, frames) = *pace.get_or_insert((Instant::now(), 0));
            let due = start + Duration::from_secs_f64(frames as f64 / (FPS * control.speed));
            let now = Instant::now();
            if due > now {
                let g = s.control.lock().unwrap();
                let _ = s.wake.wait_timeout(g, due - now).unwrap();
                // Woken early (a new control, quit): look again.
                if Instant::now() < due {
                    continue;
                }
            } else if now - due > MAX_LAG {
                pace = Some((now, 0));
            }
        }
        // Let a waiting UI thread have the machine first.
        while s.ui_waiting.load(Ordering::SeqCst) > 0 {
            std::thread::yield_now();
        }
        let (audio, off) = {
            let mut p = s.piano.lock().unwrap_or_else(|e| e.into_inner());
            p.run_frame();
            p.render(&mut picture);
            (p.take_audio(), (p.powered_off(), p.hand_lights()))
        };
        s.frame.lock().unwrap().copy_from_slice(&picture);
        if let Some((_, frames)) = &mut pace {
            *frames += 1;
        }
        // Sound (stereo, 32 kHz) plays at real-time speed only; at other
        // speeds it is dropped.
        stereo.clear();
        resample.push(&audio, &mut stereo);
        if control.speed == 1.0
            && let Some(sink) = s.sink.lock().unwrap().as_mut()
        {
            sink.push(&stereo, control.gain);
        }
        meter.1 += 1;
        let elapsed = meter.0.elapsed().as_secs_f64();
        let mut stats = s.stats.lock().unwrap();
        stats.powered_off |= off.0;
        stats.hands = off.1;
        if elapsed >= 1.0 {
            stats.speed_percent = meter.1 as f64 / elapsed / FPS * 100.0;
            meter = (Instant::now(), 0);
        }
    }
}
