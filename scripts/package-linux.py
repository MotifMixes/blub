"""Builds a Linux (x86-64) release of Blub, and optionally uploads it.

Run on Linux:   python3 -I scripts/package-linux.py [--skip-build] [--upload]
Run on Windows: python -I scripts/package-linux.py [--skip-build] [--upload]
        (builds inside WSL, Ubuntu-22.04 by default or --distro; uploads
        with the Windows gh)
Output: target/dist/Blub-<version>-linux-x64.tar.gz
        (the app, built as blub-gui and packaged as `blub`, a .desktop
        file, a read-me and the third-party licence notices; the
        command-line tool is for development and stays out)

Needs on the Linux side: Rust, pkg-config, libasound2-dev and libudev-dev.
Built on Ubuntu 22.04, the programs run on distributions with glibc 2.35 or
later. --upload attaches the archive to the GitHub release v<version> (gh
CLI, logged in); the release must exist already. An archive already on the
release with the same name is replaced.

When the repository is on a Windows drive (/mnt/c/...), the build goes to
~/.cache/blub/target-linux in the Linux home folder (much faster than the
Windows drive, and kept apart from the Windows build); BLUB_LINUX_TARGET
overrides it.

As with the Windows package, the archive is assembled in a fresh staging
folder from named files, so no ROM, save or other local file can end
up in it.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / 'target' / 'dist'
# Packaged name: built name.
PROGRAMS = {'blub': 'blub-gui'}

DESKTOP = """[Desktop Entry]
Type=Application
Name=Blub
Comment=Emulator for the Fisher-Price I Can Play Piano
Exec=blub
Terminal=false
Categories=Game;Emulator;
"""

README = """Blub {version} for Linux - an emulator for the Fisher-Price I Can Play Piano
=============================================================================

Blub includes no games: play dumps of your own cartridges (8 MB .bin files;
zipped is fine). The piano has no BIOS, so nothing else is needed.

Linux support is less tested than Windows.

Running
-------
  ./blub

Nothing to install: `blub` is the whole app (no Rust needed). To update
later, unpack the new version over this folder; your settings and save
states are kept (they live elsewhere).

Needs ALSA (libasound2) and libudev, present on almost every desktop
distribution, and a Vulkan or OpenGL driver. Works on X11 and Wayland.
To add it to your application menu, copy blub somewhere on your PATH
(e.g. ~/.local/bin) and blub.desktop to ~/.local/share/applications.

Controllers: if a controller is not found, your user may need read access
to /dev/input/event* (most distributions give it to the logged-in user).

Starting
--------
Add your games folder in the library, or File > Open Cartridge... (or drop
a cartridge on the window), then click a game to play it.

Controls (Input > Key bindings... to change)
--------------------------------------------
The piano's keys are on three rows of the PC keyboard (MAME's layout):
  low octave  \\ A Z S X D C V G B H N   (\\ = the key left of Z)
  middle      M K , L . ; Q W 3 E 4 R
  high        T 6 Y 7 U 8 I O 0 P - [
Buttons: Enter, Home/Backspace, arrows up/down, number pad (see Help >
Controls). A MIDI keyboard plays the keys too (Input > MIDI input), and the
mouse plays the drawn piano (F7, the console view).

Blub's keys: Esc pause, Tab fast-forward, ` rewind, F5/F9 quick save/load,
F8 record, F11 fullscreen, F12 screenshot.

Where things go
---------------
Settings, save states and screenshots: ~/.config/blub (or
$XDG_CONFIG_HOME/blub). If Blub closes or shows an error, send
~/.config/blub/blub.log with your report.
Help > Check for updates tells you when a new version is out.
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


def run(*cmd, **kw):
    print('+', ' '.join(str(c) for c in cmd), flush=True)
    subprocess.run([str(c) for c in cmd], check=True, **kw)


def upload(ver, out):
    run('gh', 'release', 'upload', f'v{ver}', out, '--clobber', cwd=ROOT)
    print(f'uploaded to release v{ver}')


def from_windows(a, ver):
    """Runs this script inside WSL, then uploads from Windows."""
    script = subprocess.run(
        ['wsl', '-d', a.distro, '--', 'wslpath', '-a', str(Path(__file__).resolve()).replace('\\', '/')],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    args = ['--skip-build'] if a.skip_build else []
    # A login shell, so the Linux user's Rust (~/.cargo/bin) is on PATH.
    run('wsl', '-d', a.distro, '--', 'bash', '-lc', ' '.join(['python3', '-I', f"'{script}'", *args]))
    out = DIST / f'Blub-{ver}-linux-x64.tar.gz'
    if not out.exists():
        sys.exit(f'missing {out}')
    if a.upload:
        upload(ver, out)


def release_rustflags(*extra):
    """Compiler flags for release builds, as CARGO_ENCODED_RUSTFLAGS (paths
    with spaces stay whole): the build machine's home folder and the
    repository's place on it are not kept in the programs (rustc keeps
    source paths for panic messages; they would show the account name).
    When several prefixes match, rustc uses the last one."""
    home = Path.home().resolve()
    flags = [f'--remap-path-prefix={home}=~', f'--remap-path-prefix={ROOT}=blub', *extra]
    return {'CARGO_ENCODED_RUSTFLAGS': ''.join(flags)}


def anonymous(info):
    """A tar entry owned by nobody in particular (tar records the packing
    user's name and ids otherwise)."""
    info.uid = info.gid = 0
    info.uname = info.gname = ''
    return info


def target_dir():
    if os.environ.get('BLUB_LINUX_TARGET'):
        return Path(os.environ['BLUB_LINUX_TARGET'])
    if str(ROOT).startswith('/mnt/'):
        return Path.home() / '.cache' / 'blub' / 'target-linux'
    return ROOT / 'target' / 'linux'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--skip-build', action='store_true')
    ap.add_argument('--upload', action='store_true', help='attach the archive to the GitHub release v<version>')
    ap.add_argument('--distro', default='Ubuntu-22.04', help='WSL distribution (when run on Windows)')
    a = ap.parse_args()
    ver = version()
    if sys.platform == 'win32':
        return from_windows(a, ver)
    if not sys.platform.startswith('linux'):
        sys.exit('this script builds the Linux programs and must run on Linux (or Windows with WSL)')

    tdir = target_dir()
    if not a.skip_build:
        env = dict(os.environ, **release_rustflags())
        run('cargo', 'build', '--release', '-p', 'blub-desktop', '--target-dir', tdir, cwd=ROOT, env=env)

    name = f'Blub-{ver}-linux-x64'
    stage = DIST / name
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    for prog, built in PROGRAMS.items():
        src = tdir / 'release' / built
        if not src.exists():
            sys.exit(f'missing {src} (build first)')
        shutil.copy2(src, stage / prog)
        (stage / prog).chmod(0o755)
    (stage / 'blub.desktop').write_text(DESKTOP, encoding='utf-8')
    (stage / 'README.txt').write_text(README.format(version=ver), encoding='utf-8')
    shutil.copy2(ROOT / 'THIRD-PARTY-NOTICES.txt', stage / 'THIRD-PARTY-NOTICES.txt')

    allowed = set(PROGRAMS) | {'blub.desktop', 'README.txt', 'THIRD-PARTY-NOTICES.txt'}
    found = {p.name for p in stage.iterdir()}
    if found != allowed:
        sys.exit(f'unexpected files in the package: {sorted(found - allowed)}')

    refuse_personal_info(stage)

    out = DIST / f'{name}.tar.gz'
    with tarfile.open(out, 'w:gz', compresslevel=9) as tar:
        for f in sorted(stage.iterdir()):
            # (In a folder named Blub, without the version: unpacked where the
            # last one was, a new release replaces it.)
            tar.add(f, f'Blub/{f.name}', filter=anonymous)
    print(f'{out} ({out.stat().st_size // 1024} KiB)')
    if a.upload:
        if shutil.which('gh') is None:
            sys.exit('gh is not installed here; upload the archive with gh release upload from another machine')
        upload(ver, out)


if __name__ == '__main__':
    main()
