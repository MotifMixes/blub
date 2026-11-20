# I Can Play Piano and I Can Play Guitar hardware notes

What Blub knows about the Fisher-Price I Can Play Piano (2006) and I Can Play Guitar (2007), and where each fact
comes from (the guitar's are in [their own section](#the-i-can-play-guitar)). "MAME" means
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

## The I Can Play Guitar

The guitar is the piano's sibling: MAME runs both in the same driver family (`spg2xx_ican.cpp`; the guitar is
`icanguit`), and Blub runs the guitar's cartridges on the piano's CPU, video and sound models unchanged. "Measured"
again means found by running the seven cartridges below in Blub (with the input probes kept out of the repository).

- **Cartridges:** 8 MB ROMs laid out as the piano's (stored low byte first, filling the CPU's ROM space, no BIOS).
  The seven: Guitar Favorites (K7357, CRC32 c804822e), Barbie Guitar Party (K9901, 0355a72a), Hot Wheels - Road
  Tunes (K9902, d2c4da6b), SpongeBob SquarePants - Surf's Up (K9903, c6ead0fd), Nashville Hits (K9905, b3615978),
  Rock 101 (K9906, 74b81353), American Idol - Guitar Greats (K9907, 5a4daf19). Each starts on the same "I Can
  Play Guitar" title screen and shows its own after a button press, then a Last Game / New Game menu (measured).
- **Telling the two apart:** nothing in the ROMs names the console, but their programs set the input scan up
  differently at once: the piano sets port A's direction to 0x7000 (bits 12-14 out), the guitar to 0xfc00 (bits
  10-15 out). Blub runs a cartridge until that first write to choose the console (`blub_piano::detect`; measured on
  all 13 piano and 7 guitar cartridges).
- **The scan (measured):** the program drives one of port A bits 10-15 at a time (0x0400 first, 0x8000 last, with
  0 between) and reads port A bits 0-9 and port C with it. Blub calls these scans 0-5. Port C bits 11-12 are
  outputs (driven 0 after power-on; no lights are known), port B bits 0-2 and 5 are outputs too (bit-banged at
  power-on, as the piano's; perhaps a save memory for the Last Game choice, unconfirmed).
- **The neck:** 44 fret buttons, read on port A. Scan n reads string n+1 (string 1 is the high E, 6 the low E):
  fret f on bit f-1. The outer strings (1 and 6) have frets 1-12, the four between frets 1-5, which leaves bit 5
  of scans 1-4 free: those carry the outer strings' frets 7 and 8 (string 1: fret 7 on scan 1, fret 8 on scan 2;
  string 6: fret 7 on scan 4, fret 8 on scan 3), and the outer strings' frets 9-12 are on bits 6-9 of their own
  scans. Found by fretting each line and plucking each string in Jam Session: every fret raises its string's note a
  semitone (a sample's rate going 9437, 9995, 10582, 11211, 11881, 12594, with a higher sample from fret 5 or 6 on).
  The songs' on-screen neck agrees: it lights the fret held, column by fret, low E on the top row.
- **Tuning:** the open strings are a standard guitar's: E A D G B E, low to high (the intervals between the
  strings' notes are measured; the octave is assumed).
- **Strings (measured):** scan 5's port C bits 3-8 are the strings, bit 3 string 1 (high E) to bit 8 string 6. A
  string's note plays when its line drops again after a short pulse; a line held up for about 24 frames or more
  plays nothing. Blub's strings are plucks: a 2-frame pulse (the note then starts a frame later).
- **Buttons (measured):** port C, with the scans: up scan 2 bit 1, down scan 4 bit 1, Enter scan 3 bit 1, Home scan
  3 bit 0, Pause scan 2 bit 0 (held, it freezes the picture and silences the song). Scan 1 bits 0 and 2 move the
  cursor as the arrows do and skip the Road to Rock tutorial; Blub takes them for left and right (which is which is
  unconfirmed: no screen found yet uses them).
- **The no-TV switch (measured):** scan 5's port C bit 10 set at power-on gives no picture, but the strings still
  sound, as the piano's Keyboard Mode (Blub's Keyboard Mode setting sets it on the guitar). Set later, it blanks
  the picture.
- **Still unknown:** scan 0's port C bits 0-2 are read (they leave the title screen, as any button does) but no
  screen found yet reacts to them (the cartridges' tutorial picture shows a small white button, a round switch, a
  star and the whammy bar on the body besides the arrows and two blue buttons); the whammy bar; what the volume
  control is (no line changes the level); what port C bits 11-12 and port B drive; the real console's light and
  power switch. The real toy's tempo was not compared against recordings as the piano's was (the beat clock is the
  piano's).

## Shared with the LeapFrog ClickStart

The CPU, video, sound and I/O models came from the ClickStart model in Blub's sister project
[Ribbit](https://github.com/MotifMixes/ribbit), the same chip family; its `docs/clickstart.md` has the notes on the
unSP and the SPG2xx units.
