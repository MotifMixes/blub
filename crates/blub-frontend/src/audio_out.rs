//! Host audio output: plays the console's sound, resampled to 32 kHz stereo
//! by its emulation thread, through the default output device (cpal).
//! Frontend only: nothing here affects emulation. The queued 32 kHz frames
//! (a left and a right sample each) are resampled linearly to the device
//! rate in the device callback, onto its first two channels.
//!
//! Samples arrive in bursts (emulation chunks, UI frames), so the queue is a
//! jitter buffer: playback starts once `START` is queued, and if the queue
//! runs dry it fades out and waits to refill instead of playing a stream of
//! one-sample gaps (which is what crackles).
//!
//! The emulator is paced by the system clock and the device plays by its
//! own crystal; the two differ slightly (typically 0.01-0.1%), so at a fixed
//! ratio the queue would slowly drain (a gap every few minutes) or overfill
//! (dropped samples). `RateControl` nudges the resampling ratio, by at most
//! `MAX_RATE_ADJUST` (inaudible), to hold the queue at `TARGET`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// Optional host-side "remaster" of the sound: effects, like the picture
/// upscaler, that change nothing emulated.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Enhance {
    /// As the console plays it (stereo).
    #[default]
    Off,
    /// Harmonic exciter: overtones above the 4 kHz the source can carry.
    Crisp,
    /// Crisp plus mono-compatible stereo widening (on the mid signal, so
    /// the console's own panning stays).
    CrispStereo,
}

impl Enhance {
    pub const ALL: [Enhance; 3] = [Enhance::Off, Enhance::Crisp, Enhance::CrispStereo];

    pub fn label(self) -> &'static str {
        match self {
            Enhance::Off => "Original (stereo)",
            Enhance::Crisp => "Crisp",
            Enhance::CrispStereo => "Crisp + wide stereo",
        }
    }
}

/// Rate of the queued audio (the emulation threads resample to it), in
/// stereo frames a second.
pub const SOURCE_RATE: u32 = 32_000;
const SOURCE_HZ: f64 = SOURCE_RATE as f64;
/// Queue limit (a quarter second, in frames) before old frames are dropped.
const MAX_QUEUE: usize = SOURCE_RATE as usize / 4;
/// Queued frames needed before playback (re)starts: 100 ms, enough to ride
/// out short emulation stalls (e.g. other programs loading the CPU).
const START: usize = SOURCE_RATE as usize * 100 / 1000;
/// Queue level the rate control holds while playing.
const TARGET: f64 = START as f64;
/// Largest resampling-ratio correction (0.5%: about 9 cents, below what
/// listeners notice on program material).
const MAX_RATE_ADJUST: f64 = 0.005;

/// Keeps the queue near `TARGET` by adjusting the resampling ratio in
/// proportion to the (smoothed) distance from it.
#[derive(Debug, Clone, Copy)]
struct RateControl {
    /// Queue level, low-pass filtered (callbacks are bursty).
    level: f64,
}

impl RateControl {
    fn new() -> Self {
        Self { level: TARGET }
    }

    /// Restart from the current level (playback (re)started).
    fn reset(&mut self, queued: usize) {
        self.level = queued as f64;
    }

    /// The ratio multiplier for one device callback, given the queue level
    /// and the callback length in seconds.
    fn factor(&mut self, queued: usize, seconds: f64) -> f64 {
        // About a 0.5 s time constant.
        let a = (seconds / 0.5).min(1.0);
        self.level += (queued as f64 - self.level) * a;
        // Full correction when the queue is 50% off target.
        let error = (self.level - TARGET) / TARGET;
        1.0 + (error * 2.0 * MAX_RATE_ADJUST).clamp(-MAX_RATE_ADJUST, MAX_RATE_ADJUST)
    }
}
/// Fade length (device frames) when the queue runs dry.
const FADE: f32 = 64.0;

#[derive(Default)]
struct Queue {
    /// Stereo frames (left, right).
    frames: VecDeque<[i16; 2]>,
    playing: bool,
    /// Times the queue ran dry while playing.
    underruns: u64,
    /// Times frames were dropped because the queue was full (the sound
    /// skips: the emulator ran ahead of the device, e.g. catching up).
    overflows: u64,
    /// The longest time between two device callbacks since last asked (a
    /// starved output thread shows here).
    longest_gap: std::time::Duration,
    /// While recording (`AudioOut::start_record`): every 32 kHz frame
    /// queued (interleaved left, right), before the volume.
    record: Option<Vec<i16>>,
}

impl Queue {
    /// Drops the oldest frames beyond `MAX_QUEUE`.
    fn trim(&mut self) {
        if self.frames.len() > MAX_QUEUE {
            self.overflows += 1;
            let extra = self.frames.len() - MAX_QUEUE;
            self.frames.drain(..extra);
        }
    }

    /// Queues interleaved stereo `samples` at `gain` (0..=1).
    fn push(&mut self, samples: &[i16], gain: f32) {
        if let Some(r) = &mut self.record {
            r.extend_from_slice(&samples[..samples.len() & !1]);
        }
        let g = |s: i16| (s as f32 * gain) as i16;
        self.frames.extend(samples.chunks_exact(2).map(|f| [g(f[0]), g(f[1])]));
        self.trim();
    }
}

pub struct AudioOut {
    queue: Arc<Mutex<Queue>>,
    enhance: Arc<AtomicU8>,
    _stream: cpal::Stream,
}

/// The producer end of the output queue, for the emulation thread (the
/// device stream itself stays on the UI thread).
pub struct AudioSink {
    queue: Arc<Mutex<Queue>>,
}

impl AudioSink {
    /// Queues 32 kHz stereo (interleaved left, right) at `gain` (0..=1).
    pub fn push(&mut self, samples: &[i16], gain: f32) {
        self.queue.lock().unwrap().push(samples, gain);
    }
}

impl AudioOut {
    /// Drops queued audio (pause, reset).
    pub fn clear(&mut self) {
        let mut q = self.queue.lock().unwrap();
        q.frames.clear();
        q.playing = false;
    }

    /// Keeps what the console plays from now (for a recording).
    pub fn start_record(&self) {
        self.queue.lock().unwrap().record = Some(Vec::new());
    }

    /// Moves what was kept since the last call to `out` (interleaved
    /// stereo).
    pub fn drain_record(&self, out: &mut Vec<i16>) {
        if let Some(r) = &mut self.queue.lock().unwrap().record {
            out.append(r);
        }
    }

    pub fn stop_record(&self, out: &mut Vec<i16>) {
        if let Some(mut r) = self.queue.lock().unwrap().record.take() {
            out.append(&mut r);
        }
    }

    pub fn set_enhance(&self, e: Enhance) {
        self.enhance.store(e as u8, Ordering::Relaxed);
    }

    /// A producer for this output (one per emulation thread).
    pub fn sink(&self) -> AudioSink {
        AudioSink { queue: self.queue.clone() }
    }

    /// Times playback ran out of samples (each is an audible gap), and
    /// times queued samples were dropped (each an audible skip).
    pub fn glitches(&self) -> (u64, u64) {
        let q = self.queue.lock().unwrap();
        (q.underruns, q.overflows)
    }

    /// The longest time between device callbacks since the last call.
    pub fn take_longest_gap(&self) -> std::time::Duration {
        std::mem::take(&mut self.queue.lock().unwrap().longest_gap)
    }

    /// Frames waiting and whether the device is playing them (for the log).
    pub fn queue_state(&self) -> (usize, bool) {
        let q = self.queue.lock().unwrap();
        (q.frames.len(), q.playing)
    }

    /// Opens the default output device; None (with a log line) if there is
    /// no usable device, in which case the emulator simply runs silently.
    pub fn open() -> Option<Self> {
        match Self::try_open() {
            Ok(a) => Some(a),
            Err(e) => {
                tracing::warn!(target: "blub::audio", "no audio output: {e:#}");
                None
            }
        }
    }

    fn try_open() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("no default output device"))?;
        let supported = device.default_output_config()?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let device_hz = config.sample_rate;
        let queue = Arc::new(Mutex::new(Queue::default()));
        let enhance = Arc::new(AtomicU8::new(Enhance::default() as u8));
        let (q, e) = (queue.clone(), enhance.clone());
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, q, e)?,
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, q, e)?,
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, q, e)?,
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, q, e)?,
            other => anyhow::bail!("unsupported device sample format {other}"),
        };
        stream.play()?;
        tracing::info!(target: "blub::audio", device_hz, channels = config.channels, "audio output open");
        Ok(Self { queue, enhance, _stream: stream })
    }
}

/// First-order high-pass filter.
struct HighPass {
    a: f32,
    x: f32,
    y: f32,
}

impl HighPass {
    fn new(cutoff_hz: f32, rate_hz: f32) -> Self {
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        Self { a: rc / (rc + 1.0 / rate_hz), x: 0.0, y: 0.0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        self.y = self.a * (self.y + x - self.x);
        self.x = x;
        self.y
    }
}

/// The `Enhance` effects at the device rate: stereo in and out. The
/// exciter runs on each channel; the widening on the mid signal, so what
/// the console pans stays where it is.
struct Remaster {
    band: [HighPass; 2],
    air: [HighPass; 2],
    delay: Vec<f32>,
    at: usize,
}

impl Remaster {
    fn new(rate_hz: f32) -> Self {
        Self {
            band: [HighPass::new(2_000.0, rate_hz), HighPass::new(2_000.0, rate_hz)],
            air: [HighPass::new(4_000.0, rate_hz), HighPass::new(4_000.0, rate_hz)],
            // 12 ms: wide without an audible echo.
            delay: vec![0.0; ((rate_hz * 0.012) as usize).max(1)],
            at: 0,
        }
    }

    fn run(&mut self, (l, r): (f32, f32), mode: u8) -> (f32, f32) {
        if mode == Enhance::Off as u8 {
            return (l, r);
        }
        // Exciter: saturate the upper band, keep only the new overtones
        // above 4 kHz, mix them in.
        let mut crisp = [l, r];
        for (c, v) in crisp.iter_mut().enumerate() {
            let band = self.band[c].run(*v);
            let sat = (band * 4.0).tanh() * 0.25;
            *v += 0.8 * self.air[c].run(sat);
        }
        let [l, r] = crisp;
        if mode != Enhance::CrispStereo as u8 {
            return (l, r);
        }
        // Complementary combs on the mid signal: left + right stays the
        // crisp left + right.
        let d = self.delay[self.at];
        self.delay[self.at] = 0.5 * (l + r);
        self.at = (self.at + 1) % self.delay.len();
        ((l + 0.35 * d) / 1.35, (r - 0.35 * d) / 1.35)
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<Mutex<Queue>>,
    enhance: Arc<AtomicU8>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let step = SOURCE_HZ / config.sample_rate as f64;
    // Resampler state: the two source frames being interpolated and the
    // position between them; `level` fades in/out around gaps.
    let (mut prev, mut next, mut frac) = ([0.0f32; 2], [0.0f32; 2], 0.0f64);
    let mut level = 0.0f32;
    let mut rate = RateControl::new();
    let mut remaster = Remaster::new(config.sample_rate as f32);
    // This callback's source frames, taken out of the queue at its start.
    let mut taken: Vec<[i16; 2]> = Vec::new();
    let mut last_callback: Option<std::time::Instant> = None;
    let stream = device.build_output_stream(
        *config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mode = enhance.load(Ordering::Relaxed);
            let frames = data.len() / channels.max(1);
            // The queue is locked only to take this callback's frames:
            // the emulation thread pushes to it, and a callback preempted
            // while holding it (a busy system) would hold that thread up.
            let now = std::time::Instant::now();
            let gap = last_callback.map(|t| now - t);
            last_callback = Some(now);
            let playing = {
                let mut q = queue.lock().unwrap();
                if let Some(g) = gap {
                    q.longest_gap = q.longest_gap.max(g);
                }
                if !q.playing && q.frames.len() >= START {
                    q.playing = true;
                    rate.reset(q.frames.len());
                }
                let step = step * rate.factor(q.frames.len(), frames as f64 * step / SOURCE_HZ);
                taken.clear();
                if q.playing {
                    // How many the resampler will step over in `frames`.
                    let (mut f, mut need) = (frac, 0usize);
                    for _ in 0..frames {
                        f += step;
                        need += f as usize;
                        f -= f.floor();
                    }
                    let n = need.min(q.frames.len());
                    taken.extend(q.frames.drain(..n));
                    if n < need {
                        // Dry: hold the last value, fade out, refill.
                        q.playing = false;
                        q.underruns += 1;
                    }
                }
                (q.playing || !taken.is_empty(), step)
            };
            let (mut playing, step) = playing;
            let mut source = taken.iter();
            for frame in data.chunks_mut(channels) {
                if playing {
                    frac += step;
                    while frac >= 1.0 {
                        frac -= 1.0;
                        prev = next;
                        match source.next() {
                            Some(&[l, r]) => next = [l as f32 / 32768.0, r as f32 / 32768.0],
                            None => {
                                playing = false;
                                break;
                            }
                        }
                    }
                }
                level = if playing { (level + 1.0 / FADE).min(1.0) } else { (level - 1.0 / FADE).max(0.0) };
                let at = |c: usize| (prev[c] + (next[c] - prev[c]) * frac as f32) * level;
                let (l, r) = remaster.run((at(0), at(1)), mode);
                let (l, r) = (l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0));
                // Left and right on the first two channels; a mono device
                // gets their average, any further channels silence.
                match frame {
                    [left, right, rest @ ..] => {
                        *left = T::from_sample(l);
                        *right = T::from_sample(r);
                        rest.iter_mut().for_each(|s| *s = T::from_sample(0.0f32));
                    }
                    [only] => *only = T::from_sample(0.5 * (l + r)),
                    [] => {}
                }
            }
        },
        |err| tracing::warn!(target: "blub::audio", "audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

/// Stereo at a sound unit's rate to stereo at the output queue's rate
/// (`SOURCE_RATE`), interleaved left, right in and out, each channel on its
/// own: low-passed below the output's half rate first (a windowed sinc),
/// then linearly interpolated. Without the filter, sound above 16 kHz
/// folded back as hiss.
pub struct Resampler {
    /// Input frames per output frame.
    step: f64,
    /// Position of the next output frame, in input frames.
    pos: f64,
    prev: [f32; 2],
    /// The low-pass filter's taps (empty when the input is not faster than
    /// the output), and each channel's last input samples it needs.
    taps: Vec<f32>,
    history: [Vec<f32>; 2],
}

/// The filter: passes up to `PASS_HZ`, its Blackman window's transition
/// reaching about 17 kHz with `TAPS` taps at 72 kHz.
const TAPS: usize = 95;
const PASS_HZ: f64 = 13_000.0;

impl Resampler {
    pub fn new(from_hz: f64) -> Self {
        let taps = if from_hz > SOURCE_HZ * 1.1 {
            let fc = PASS_HZ / from_hz;
            let mid = (TAPS - 1) as f64 / 2.0;
            let mut t: Vec<f64> = (0..TAPS)
                .map(|i| {
                    let x = i as f64 - mid;
                    let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x) };
                    let w = 2.0 * std::f64::consts::PI * i as f64 / (TAPS - 1) as f64;
                    sinc * (0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos())
                })
                .collect();
            let sum: f64 = t.iter().sum();
            t.iter_mut().for_each(|v| *v /= sum);
            t.into_iter().map(|v| v as f32).collect()
        } else {
            Vec::new()
        };
        let keep = taps.len().saturating_sub(1);
        Self { step: from_hz / SOURCE_HZ, pos: 0.0, prev: [0.0; 2], taps, history: [vec![0.0; keep], vec![0.0; keep]] }
    }

    /// Adds interleaved stereo input; appends the interleaved output it makes.
    pub fn push(&mut self, stereo: &[i16], out: &mut Vec<i16>) {
        let frames = stereo.as_chunks::<2>().0;
        let mut ch: [Vec<f32>; 2] = [0, 1].map(|c| frames.iter().map(|f| f[c] as f32).collect());
        if !self.taps.is_empty() {
            // The filter runs over the kept history and the new samples (a
            // fixed delay of half the taps, the same for everything).
            for (c, samples) in ch.iter_mut().enumerate() {
                let keep = self.history[c].len();
                let mut all = std::mem::take(&mut self.history[c]);
                all.extend_from_slice(samples);
                for (i, m) in samples.iter_mut().enumerate() {
                    *m = all[i..i + self.taps.len()].iter().zip(&self.taps).map(|(a, b)| a * b).sum();
                }
                self.history[c] = all.split_off(all.len() - keep);
            }
        }
        let n = frames.len() as f64;
        while self.pos < n {
            let i = self.pos.floor() as usize;
            let f = (self.pos - i as f64) as f32;
            for (c, samples) in ch.iter().enumerate() {
                let a = if i == 0 { self.prev[c] } else { samples[i - 1] };
                let b = samples[i];
                out.push((a + (b - a) * f).clamp(-32768.0, 32767.0) as i16);
            }
            self.pos += self.step;
        }
        self.pos -= n;
        for (c, samples) in ch.iter().enumerate() {
            if let Some(&last) = samples.last() {
                self.prev[c] = last;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_filters_what_would_fold_back() {
        let rate = blub_piano::OUTPUT_RATE;
        let level = |hz: f64| {
            let mut r = Resampler::new(rate);
            let input: Vec<i16> = (0..rate as usize)
                .flat_map(|i| {
                    let v = (10000.0 * (2.0 * std::f64::consts::PI * hz * i as f64 / rate).sin()) as i16;
                    [v, v]
                })
                .collect();
            let mut out = Vec::new();
            for chunk in input.chunks(2 * 720) {
                r.push(chunk, &mut out);
            }
            let tail = &out[out.len() / 2 & !1..];
            (tail.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / tail.len() as f64).sqrt()
        };
        let (low, high) = (level(1000.0), level(20000.0));
        assert!(low > 6500.0, "1 kHz passes: {low}");
        assert!(high < 100.0, "20 kHz (it would fold to 12 kHz) is gone: {high}");
    }

    #[test]
    fn resampler_rate() {
        let mut r = Resampler::new(blub_piano::OUTPUT_RATE);
        let mut out = Vec::new();
        // One second of input gives about one second of output.
        let input = vec![0i16; 2 * blub_piano::OUTPUT_RATE as usize];
        r.push(&input, &mut out);
        assert!((out.len() as f64 / 2.0 - SOURCE_HZ).abs() < 2.0, "{}", out.len());
    }

    /// The console's panning survives: a left-only sound stays on the left
    /// through the resampler and the output queue.
    #[test]
    fn left_stays_left() {
        let rate = blub_piano::OUTPUT_RATE;
        let mut r = Resampler::new(rate);
        let input: Vec<i16> = (0..rate as usize / 10)
            .flat_map(|i| [(8000.0 * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / rate).sin()) as i16, 0])
            .collect();
        let mut out = Vec::new();
        r.push(&input, &mut out);
        assert_eq!(out.len() % 2, 0);
        assert!(out.iter().skip(1).step_by(2).all(|&v| v == 0), "nothing on the right");
        assert!(out.iter().step_by(2).any(|&v| v.abs() > 4000), "the left plays");
        let mut q = Queue { record: Some(Vec::new()), ..Default::default() };
        q.push(&out, 0.5);
        assert_eq!(q.frames.len(), out.len() / 2, "counted in frames");
        assert!(q.frames.iter().all(|f| f[1] == 0) && q.frames.iter().any(|f| f[0].abs() > 2000));
        assert_eq!(q.record.as_deref(), Some(&out[..]), "recorded before the volume");
        let mut off = Remaster::new(48_000.0);
        assert_eq!(off.run((0.25, 0.0), Enhance::Off as u8), (0.25, 0.0));
    }


    /// A producer and a device whose clocks differ by 0.2%, with bursty
    /// delivery (2 ms chunks in, 10 ms callbacks out): the queue settles
    /// near the target and never runs dry or overflows.
    #[test]
    fn rate_control_absorbs_clock_drift() {
        for drift in [-0.002, 0.0, 0.002] {
            let device_hz = 48_000.0;
            let base = SOURCE_HZ / device_hz;
            let mut rate = RateControl::new();
            let mut queued = START as f64;
            let (mut lo, mut hi) = (f64::MAX, 0.0f64);
            // 10 minutes of 10 ms callbacks.
            for n in 0..60_000 {
                // Five 2 ms producer chunks per callback, at the drifted rate.
                queued += SOURCE_HZ * (1.0 + drift) * 0.010;
                let step = base * rate.factor(queued as usize, 480.0 * base / SOURCE_HZ);
                queued -= 480.0 * step;
                if n > 6_000 {
                    lo = lo.min(queued);
                    hi = hi.max(queued);
                }
            }
            assert!(lo > 0.5 * TARGET && hi < 1.5 * TARGET && hi < MAX_QUEUE as f64, "drift {drift}: {lo}..{hi}");
        }
    }

    #[test]
    fn remaster_is_bounded_and_mono_compatible() {
        let (mut crisp, mut wide) = (Remaster::new(48_000.0), Remaster::new(48_000.0));
        for n in 0..48_000 {
            // A loud 1 kHz tone with a square-ish edge, louder on the left:
            // worst case for the exciter.
            let v = (n as f32 * 2.0 * std::f32::consts::PI * 1_000.0 / 48_000.0).sin().signum() * 0.9;
            let (l, r) = crisp.run((v, 0.5 * v), Enhance::Crisp as u8);
            assert!(l.is_finite() && r.is_finite() && l.abs() < 1.5 && r.abs() < l.abs() + 1e-6, "{l} {r}");
            let (wl, wr) = wide.run((v, 0.5 * v), Enhance::CrispStereo as u8);
            assert!(wl.is_finite() && wr.is_finite());
            // L + R is the (scaled) crisp L + R: nothing cancels in mono.
            assert!(((wl + wr) * 1.35 - (l + r)).abs() < 1e-4, "{wl} {wr} {l} {r}");
        }
        let mut off = Remaster::new(48_000.0);
        assert_eq!(off.run((0.25, -0.5), Enhance::Off as u8), (0.25, -0.5));
    }
}
