"""Builds a Windows release zip of Blub.

Run:    python -I scripts/package-release.py [--skip-build]
Output: target/dist/Blub-<version>-windows-x64.zip

The zip holds only the app (blub-gui.exe, as Blub.exe), a short read-me
written here and the third-party licence notices; the command-line tool
(blub.exe) is for development and stays out. It is
assembled in a fresh staging folder from named files, so no BIOS, ROM, save
or other local file can end up inside; the script also refuses any file that
is not on its list.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / 'target' / 'dist'
# Packaged name: built name.
PROGRAMS = {'Blub.exe': 'blub-gui.exe'}

README = """Blub {version} - an emulator for the Fisher-Price I Can Play Piano

Double-click Blub.exe to start. Blub includes no games: play dumps of your
own cartridges (8 MB .bin files; zipped is fine).

1. Choose your games folder in the library (or File > Open Cartridge, or
   drop a cartridge on the window).
2. Click a game to play it.

The piano's keys are on three rows of the PC keyboard (MAME's layout):
  low octave  \\ A Z S X D C V G B H N   (\\ = the key left of Z)
  middle      M K , L . ; Q W 3 E 4 R
  high        T 6 Y 7 U 8 I O 0 P - [
Buttons: Enter, Home/Backspace, arrows up/down, number pad (see Help >
Controls). A MIDI keyboard plays the keys too (Input > MIDI input), and the
mouse plays the drawn piano (F7, the console view).

Blub's keys: Esc pause, Tab fast-forward, ` rewind, F5/F9 quick save/load,
F8 record, F11 fullscreen, F12 screenshot.

Settings, save states and the log (blub.log) are in %APPDATA%\\Blub.
"""


def personal_info(stage):
    """Files in `stage` holding the build machine's home folder, any user
    folder path or the git email: (file, what) pairs. A release must have
    none (the names are found on the machine running this, never stored)."""
    home = str(Path.home().resolve())
    needles = {home, home.replace('\\', '/'), 'C:\\Users\\', '/Users/', '/home/'}
    email = subprocess.run(['git', 'config', 'user.email'], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    if email and not email.endswith('@users.noreply.github.com'):
        needles.add(email)
    found = []
    for f in sorted(p for p in Path(stage).rglob('*') if p.is_file()):
        data = f.read_bytes()
        found += [(f.relative_to(stage), n) for n in sorted(needles) if n.encode() in data]
    return found


def refuse_personal_info(stage):
    found = personal_info(stage)
    if found:
        for f, n in found:
            print(f'  {f}: contains {n!r}')
        sys.exit('the package holds personal information (paths or email); not packaged')


def version():
    text = (ROOT / 'Cargo.toml').read_text(encoding='utf-8')
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        sys.exit('no workspace version in Cargo.toml')
    return m.group(1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--skip-build', action='store_true')
    a = ap.parse_args()
    ver = version()
    if not a.skip_build:
        subprocess.run(['cargo', 'build', '--release', '-j', '8', '-p', 'blub-desktop'], cwd=ROOT, check=True)
    name = f'Blub-{ver}-windows-x64'
    stage = DIST / name
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    for exe, built in PROGRAMS.items():
        src = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'release' / built
        if not src.exists():
            sys.exit(f'missing {src} (build first)')
        shutil.copy2(src, stage / exe)
    (stage / 'README.txt').write_bytes(README.format(version=ver).replace('\n', '\r\n').encode('utf-8'))

    shutil.copy2(ROOT / 'THIRD-PARTY-NOTICES.txt', stage / 'THIRD-PARTY-NOTICES.txt')

    allowed = set(PROGRAMS) | {'README.txt', 'THIRD-PARTY-NOTICES.txt'}
    found = {p.name for p in stage.iterdir()}
    if found != allowed:
        sys.exit(f'unexpected files in the package: {sorted(found - allowed)}')

    refuse_personal_info(stage)

    out = DIST / f'{name}.zip'
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for f in sorted(stage.iterdir()):
            # Times in UTC: zip stores the zipping PC's local time, which
            # would give away its time zone.
            # (In a folder named Blub, without the version: unpacked where the
            # last one was, a new release replaces it.)
            info = zipfile.ZipInfo(f'Blub/{f.name}', date_time=time.gmtime(f.stat().st_mtime)[:6])
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (f.stat().st_mode & 0xFFFF) << 16
            z.writestr(info, f.read_bytes(), compresslevel=9)
    print(f'{out} ({out.stat().st_size // 1024} KiB)')


if __name__ == '__main__':
    main()
