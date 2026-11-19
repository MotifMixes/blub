<p align="center"><img src="docs/images/blub-icon.png" alt="Blub's fish icon" width="160"></p>

# Blub

An emulator for **Fisher-Price consoles**, starting with the **Fisher-Price I Can Play Piano** (2006): the TV
keyboard with 36 light-up keys and plug-in song cartridges, built on a Sunplus SPG24x. Written in Rust, for
**Windows, macOS and Linux**. Blub is a sister of [Ribbit](https://github.com/MotifMixes/ribbit) (LeapFrog) and
looks and works the same way.

## Play

1. Build it (below), or download a release when there is one.
2. In Blub, add your games folder in the library, or open a cartridge with **File → Open Cartridge…** (or drop it on
   the window). Cartridges are 8 MB `.bin` dumps (zipped is fine too).
3. Play on your PC keyboard, a game controller, the mouse on the drawn piano (the console view, F7), or a **MIDI
   keyboard**.

**Something wrong?** Send the file `blub.log` with your report. It's in `%APPDATA%\Blub` on Windows and
`~/.config/blub` on macOS and Linux.

**→ [User guide](docs/manual/01-welcome.md)** (also in Blub: **Help → User Guide**)

> [!IMPORTANT]
> ## This project was made with AI
>
> **Blub is written almost entirely by an AI coding assistant**, working under my direction. Please judge it by its
> results. Blub is not affiliated with or endorsed by Fisher-Price or Mattel. Fisher-Price and I Can Play Piano are
> their trademarks.

## You need your own dumps

Blub includes **no games**. The I Can Play Piano has no BIOS: each cartridge holds the whole program, so a dump of
your own cartridge is all you need.

## Supported

| Console | State |
|---|---|
| **I Can Play Piano** | Plays: all 13 known cartridges reach their title screens, menus, songs and Freestyle, with music and voices; Keyboard Mode plays every key. The Games menus are less tested. The buttons follow MAME's `icanpian` driver (what some of them do is unverified). |

What Blub knows about the hardware, and how it was found: [docs/hardware.md](docs/hardware.md).

## Features

The same as Ribbit's: a game library with title-screen art, favourites and folders; save states (5 slots, F5/F9),
**rewind** (hold <code>`</code>), fast-forward (hold Tab) and speed; pause, reset; screenshots (F12) and **video
recording** (F8: FFmpeg, Windows' own H.264, or AVI); AI upscaling (ArtCNN, Anime4K, ScaleFX) and other scalers;
fullscreen (F11); per-game settings; controllers; Discord presence; the update check; and the **console view**
(F7): the picture on a TV above a drawing of the piano after the real one (colour-coded keys, the panel's buttons,
the hands' lights), its keys lined up under the songs' on-screen keyboard. The keys and buttons can be played with
the mouse and light up while held from any source.

## Controls

The piano's keys on the PC keyboard follow MAME's layout, three octaves on three rows of letters (all keys can be
changed in **Input → Key bindings…**):

| Piano | PC keys |
|---|---|
| Low octave, F2 to E3 | `\` (the key left of Z on ISO keyboards) `A Z S X D C V G B H N` |
| Middle octave, F3 to E4 | `M K , L . ; Q W 3 E 4 R` |
| High octave, F4 to E5 | `T 6 Y 7 U 8 I O 0 P - [` |

| Button | Key | Controller |
|---|---|---|
| Enter | Enter | A |
| Home | Home or Backspace | B |
| Up / Down | ↑ / ↓ | D-pad, left stick |
| Change instrument | Numpad 5 | X |
| Hands | Numpad 6 | Y |
| Display 1-4 | Numpad 1-4 | |
| Tempo up / normal / down | Numpad + / * / - | RB / - / LB |
| Pause | Numpad 7 | Start |
| Metronome | Numpad 8 | |

**MIDI:** plug in a MIDI keyboard and start a game: Blub opens the first MIDI input it finds (or the one chosen in
**Input → MIDI input**) and its keys play the piano's. Only the piano's own three octaves (F2 to E5) play: notes outside them do nothing; an
octave shift (-2 to +2) is in the same menu. **Input → Keyboard Mode** moves the console's TV / Keyboard Mode slider.

Blub's own keys: Esc pause, Tab fast-forward, <code>`</code> rewind, F5 / F9 quick save / load, Ctrl+R reset,
Ctrl+O open, F3 details, F7 console view, F8 record, F10 menu, F11 fullscreen, F12 screenshot.

## Building

Rust 1.88 or later. On Windows and macOS nothing else is needed; on Linux, the usual window and sound headers
(Ubuntu: `sudo apt install libxkbcommon-dev libwayland-dev libasound2-dev libudev-dev`).

```
cargo build --release -p blub-desktop
```

This makes `blub-gui` (the app) and `blub`, a command-line tool:

```
blub run [cartridge]                                # the app
blub info cartridge.bin                             # size, checksum
blub headless cartridge.bin --frames 900 --screenshot out.png --press "600:enter;610:enter:u"
```

`headless` runs a cartridge without a window (60 frames a second) and saves the last picture, and with
`--audio-out` its sound; presses are `frame:what[:u]` with `k0`-`k35` for the keys and button names (`enter`,
`home`, `scrollup`, `tempoup`...).

Crates: `blub-piano` (the console), `blub-frontend` (the app), `blub-desktop` (the programs), `blub-android`
(not built by default).

## Licence

Not chosen yet. The SPG2xx and unSP models follow MAME (BSD-3-Clause); see
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
