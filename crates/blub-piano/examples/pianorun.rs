//! Runs an I Can Play Piano or I Can Play Guitar cartridge (told apart by
//! `blub_piano::detect`) for a number of frames and saves the last picture.
//!
//! pianorun <cart.bin> <frames> <out.ppm>
//!
//! PIANO_INPUT=frame:what[:u];... presses (or with `u` releases) an input
//! from that frame. On the piano: a key (k0-k35) or a button (enter, home,
//! scrollup, ...). On the guitar: a fret (f<string>.<fret>, f1.1-f6.12;
//! string 1 is the high E), a pluck of a string (s1-s6) or a button
//! (up, down, left, right, enter, home, pause). On either, a raw scan line
//! (a<scan>.<bit> on port A, c<scan>.<bit> on port C).
//! PIANO_PPM_AT=frame,... also saves the picture at those frames
//! (<out>.<frame>.ppm). PIANO_WAV=file.wav saves the sound. PIANO_KEYBOARD=1
//! starts with the slider on Keyboard Mode (no TV picture). PIANO_SOUNDLOG=1
//! prints sound channel starts and stops with their frame, PIANO_PORTLOG=1
//! changes to the ports' outputs. PIANO_SPRITES=1 prints the sprite table
//! at each PIANO_PPM_AT frame.

use blub_piano::{Button, GuitarButton, Model, Piano, SCREEN_H, SCREEN_W};
use std::io::Write;
use std::time::Instant;

fn button(name: &str) -> Option<Button> {
    Button::ALL.into_iter().find(|b| format!("{b:?}").eq_ignore_ascii_case(name))
}

fn guitar_button(name: &str) -> Option<GuitarButton> {
    GuitarButton::ALL.into_iter().find(|b| format!("{b:?}").eq_ignore_ascii_case(name))
}

/// A guitar fret, `f<string>.<fret>` (string 1-6): (string 0-5, fret).
fn fret(what: &str) -> Option<(usize, usize)> {
    let (s, f) = what.strip_prefix('f')?.split_once('.')?;
    let (s, f): (usize, usize) = (s.parse().ok()?, f.parse().ok()?);
    (1..=6).contains(&s).then_some((s - 1, f))
}

/// A raw scan line: `a<group>.<bit>` or `c<group>.<bit>` (port A or C).
fn raw_line(what: &str) -> Option<(char, (usize, u16))> {
    let port = what.chars().next()?;
    let (g, bit) = what.get(1..)?.split_once('.')?;
    let (g, bit): (usize, u32) = (g.parse().ok()?, bit.parse().ok()?);
    ((port == 'a' || port == 'c') && g < 6 && bit < 16).then_some((port, (g, 1 << bit)))
}

fn save_ppm(path: &str, frame: &[u32]) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("create the picture"));
    write!(f, "P6\n{SCREEN_W} {SCREEN_H}\n255\n").unwrap();
    for p in frame {
        f.write_all(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]).unwrap();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cart = std::fs::read(&args[1]).expect("read the cartridge");
    let frames: u64 = args[2].parse().expect("a frame count");
    let out = &args[3];
    let mut piano = Piano::new(&cart);
    if std::env::var("PIANO_KEYBOARD").is_ok_and(|v| v == "1") {
        piano.set_keyboard_mode(true);
        piano.reset();
    }
    let guitar = piano.model() == Model::Guitar;
    println!("{}; reset: pc {:06x}", piano.model().name(), piano.cpu.lpc());
    let mut input: Vec<(u64, String, bool)> = std::env::var("PIANO_INPUT")
        .unwrap_or_default()
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let p: Vec<&str> = s.split(':').collect();
            (p[0].parse().unwrap(), p[1].to_string(), p.get(2) != Some(&"u"))
        })
        .collect();
    input.sort_by_key(|i| i.0);
    let shots: Vec<u64> = std::env::var("PIANO_PPM_AT")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let wav = std::env::var("PIANO_WAV").ok();
    let mut audio = Vec::new();
    let sound_log = std::env::var("PIANO_SOUNDLOG").is_ok_and(|v| v == "1");
    if sound_log {
        piano.trace_sound();
    }
    let port_log = std::env::var("PIANO_PORTLOG").is_ok_and(|v| v == "1");
    if port_log {
        piano.trace_ports();
    }
    let mut raw = [(0u16, 0u16); 6];
    let mut frame = vec![0u32; SCREEN_W * SCREEN_H];
    let start = Instant::now();
    for n in 0..frames {
        for (_, what, down) in input.iter().filter(|i| i.0 == n) {
            if let Some((port, line)) = raw_line(what) {
                let (g, bit) = line;
                let (a, c) = &mut raw[g];
                let v = if port == 'a' { a } else { c };
                if *down { *v |= bit } else { *v &= !bit }
                piano.set_scan_lines(g, raw[g].0, raw[g].1);
            } else if guitar && let Some((string, f)) = fret(what) {
                piano.fret(string, f, *down);
            } else if guitar && let Some(s) = what.strip_prefix('s').and_then(|s| s.parse::<usize>().ok()) {
                if *down {
                    piano.pluck(s.wrapping_sub(1));
                }
            } else if guitar && let Some(b) = guitar_button(what) {
                piano.guitar_button(b, *down);
            } else if !guitar && let Some(k) = what.strip_prefix('k').and_then(|k| k.parse().ok()) {
                piano.key(k, *down);
            } else if !guitar && let Some(b) = button(what) {
                piano.button(b, *down);
            } else {
                eprintln!("unknown input {what}");
            }
        }
        piano.run_frame();
        audio.extend(piano.take_audio());
        for line in piano.take_sound_log() {
            println!("{n:5} {line}");
        }
        for line in piano.take_port_log() {
            println!("{n:5} {line}");
        }
        if shots.contains(&n) {
            piano.render(&mut frame);
            save_ppm(&format!("{out}.{n}.ppm"), &frame);
            if std::env::var("PIANO_SPRITES").is_ok_and(|v| v == "1") {
                let (sprites, regs) = piano.video_state();
                println!("{n:5} sprite ctrl {:04x} base {:04x}", regs[0x42], regs[0x22]);
                for (i, s) in sprites.chunks(4).enumerate().filter(|(_, s)| s[0] != 0) {
                    println!("{n:5}   #{i:3} tile {:04x} x {:04x} y {:04x} attr {:04x}", s[0], s[1], s[2], s[3]);
                }
            }
        }
        if piano.powered_off() {
            println!("powered off at frame {n}");
            break;
        }
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{frames} frames in {secs:.2}s ({:.0} fps), pc {:06x}, {} instructions",
        frames as f64 / secs,
        piano.cpu.lpc(),
        piano.instructions()
    );
    if sound_log {
        let io = piano.io_regs();
        println!("io 3d10..3d2f: {:04x?}", &io[0x10..0x30]);
        let v = piano.video_regs();
        println!("video 10..2f: {:04x?}", &v[0x10..0x30]);
        for (n, sp) in piano.sprite_regs().chunks(4).enumerate().filter(|(_, s)| s[0] != 0) {
            println!("sprite {n:3}: {:04x?}", sp);
        }
        println!("video 30..43: {:04x?} 62/63 {:04x} {:04x}", &v[0x30..0x44], v[0x62], v[0x63]);
    }
    piano.render(&mut frame);
    save_ppm(out, &frame);
    if let Some(path) = wav {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let rate = blub_piano::OUTPUT_RATE as u32;
        let bytes = audio.len() as u32 * 2;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + bytes).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        for v in [16u32, 0x0002_0001, rate, rate * 4, 0x0010_0004] {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        f.write_all(b"data").unwrap();
        f.write_all(&bytes.to_le_bytes()).unwrap();
        for s in audio {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }
}
