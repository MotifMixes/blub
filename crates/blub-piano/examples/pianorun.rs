//! Runs an I Can Play Piano cartridge for a number of frames and saves the
//! last picture.
//!
//! pianorun <cart.bin> <frames> <out.ppm>
//!
//! PIANO_INPUT=frame:what[:u];... presses (or with `u` releases) a key
//! (k0-k35) or a button (enter, home, up, down, ...) from that frame.
//! PIANO_PPM_AT=frame,... also saves the picture at those frames
//! (<out>.<frame>.ppm). PIANO_WAV=file.wav saves the sound. PIANO_KEYBOARD=1
//! starts with the slider on Keyboard Mode (no TV picture). PIANO_SOUNDLOG=1
//! prints sound channel starts and stops with their frame, PIANO_PORTLOG=1
//! changes to the ports' outputs.

use blub_piano::{Button, Piano, SCREEN_H, SCREEN_W};
use std::io::Write;
use std::time::Instant;

fn button(name: &str) -> Option<Button> {
    Button::ALL.into_iter().find(|b| format!("{b:?}").eq_ignore_ascii_case(name))
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
    println!("reset: pc {:06x}", piano.cpu.lpc());
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
    let mut frame = vec![0u32; SCREEN_W * SCREEN_H];
    let start = Instant::now();
    for n in 0..frames {
        for (_, what, down) in input.iter().filter(|i| i.0 == n) {
            if let Some(k) = what.strip_prefix('k').and_then(|k| k.parse().ok()) {
                piano.key(k, *down);
            } else if let Some(b) = button(what) {
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
