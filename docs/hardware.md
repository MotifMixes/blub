# I Can Play Piano hardware notes

What Blub knows about the Fisher-Price I Can Play Piano (2006), and where each fact comes from. "MAME" means
MAME's `icanpian` driver (`src/mame/tvgames/spg2xx_ican.cpp`) and its SPG2xx devices; "measured" means found by
running the cartridges in Blub; "reference" means compared against recordings or photos of the real toy.

## The console

- **Chip:** a Sunplus SPG24x (unSP 16-bit CPU at 27 MHz, with the SPG2xx video, sound and I/O units) under a
  single epoxy blob. The cartridges are epoxy blobs too, so the chip can't be read off the board (reference: a
  teardown video).
- **No BIOS:** each cartridge is an 8 MB ROM (stored low byte first) that fills the CPU's whole 22-bit ROM space
  and holds everything, menus included (MAME). Without a cartridge the real console only flashes its power light,
  and Piano mode needs a cartridge too (reference).
- **Sprites:** the SPG24x draws 256 sprites (the SPG28x 64). Some title screens use more than 64: Magical
  Melodies' title is drawn with sprites 64-114 (measured).

## Keys and buttons (MAME)

- Port A bits 12-14 pick an octave to scan: 0x4000 the lowest, 0x2000 the middle, 0x1000 the top; 0 clears the
  latches. Port A then reads that octave's 12 keys (bit 0 F ... bit 11 E) and port C its buttons.
- The buttons by octave scan and port C bit: see `Button::line` in `crates/blub-piano/src/lib.rs`. Octave 0's
  bit 11 is the power switch, octave 2's bit 0 the TV / Keyboard Mode slider.
- Keyboard Mode (no TV picture): every key plays its note through the speaker. The cartridge fades a held note
  out about 0.65 s after it starts (measured; the real toy's notes are short too: reference).

## Lights (measured)

MAME doesn't emulate these. What the cartridges drive out of port C (bits 7-10 and 12 are outputs):

- **Bit 8 and bit 7:** the hands' lights. A song starts with bit 8 lit; each press of the hands button steps
  bit 8, bit 7, both. Blub shows bit 8 as the right hand and bit 7 as the left (unconfirmed).
- **Bit 9:** lit in the menus, dark during songs (what it lights is unknown).
- **Bit 12:** always lit (perhaps the power light; unconfirmed).

Port B bits 0-2 are bit-banged at power-on only, which MAME takes for an unused serial EEPROM.

## Sound tempo (reference)

The cartridges time their music with the sound unit's beat interrupt (beat base 0x160, counts of 1 to 5). MAME
ticks the beat counter once per output sample (281.25 kHz / 4). A recording of a real console playing Piano
Favorites' menu music has the same pitch but a 3.88% longer loop (961.6 ms against 925.7 ms); ticking the beat
counter 26 times every 27 samples matches it to 0.05%, and two other passages agree. The cause in the chip is
unknown.

## Shared with the LeapFrog ClickStart

The CPU, video, sound and I/O models came from the ClickStart model in Blub's sister project
[Ribbit](https://github.com/MotifMixes/ribbit), the same chip family; its `docs/clickstart.md` has the notes on the
unSP and the SPG2xx units.
