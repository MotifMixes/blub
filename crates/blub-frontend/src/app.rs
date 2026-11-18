//! The Blub application: one window, the emulated I Can Play Piano, egui
//! menus.

use crate::audio_out::AudioOut;
use crate::console_view::{ConsoleView, Event as ViewEvent};
use crate::emu::{Control, Emu};
use crate::files::Pick;
use crate::gpu::Gpu;
use crate::held::{Held, Source};
use crate::library::Library;
use crate::piano::Input;
use crate::settings::Settings;
use crate::ui::{self, Action, UiState};
use anyhow::{Context, Result};
use blub_piano::{SCREEN_H, SCREEN_W};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

/// Rewind: a snapshot every half second of emulated time, 30 s kept.
const REWIND_EVERY_FRAMES: u64 = 30;
const REWIND_KEEP: usize = 60;
/// The window title: name and version.
const APP_NAME: &str = concat!("Blub v", env!("CARGO_PKG_VERSION"));
/// Frames spent sizing the window around the screen (the first frames can
/// still be settling the menu bar's height).
const FIT_TRIES: u8 = 6;
/// The smallest window: wide enough for the whole menu bar (and an update
/// button) at any display scaling, so logical, not physical, pixels.
const MIN_WINDOW: winit::dpi::LogicalSize<f64> = winit::dpi::LogicalSize::new(460.0, 360.0);
/// In the data folder: how far the last starts got setting up the graphics card (see `Gpu::connect`).
const GPU_GUARD: &str = "gpu-setup-crashed.txt";
/// What the console is called (Discord, the details panel).
const CONSOLE: &str = "I Can Play Piano";

/// How Blub was started (command line overrides).
#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    /// Settings, save states and screenshots live here.
    pub data_dir: PathBuf,
    pub cartridge: Option<PathBuf>,
    /// Android: true once the activity is destroyed (swiped away from the
    /// recent apps). Blub then saves and stops: winit 0.30 does not pass
    /// that on, and Android waits for Blub to stop before it can start the
    /// app again.
    pub destroyed: Option<fn() -> bool>,
    /// Android: opens the system's file picker (true: for a folder); false
    /// if it can't (Blub's own browser is used then).
    pub pick_start: Option<fn(bool) -> bool>,
    /// Android: the picker's answer once it is in: the path, None if
    /// cancelled, or why the choice can't be used.
    pub pick_poll: Option<fn() -> Option<Result<Option<PathBuf>, String>>>,
    /// Android: keeps the screen from sleeping (true) or lets it (false).
    pub keep_screen_on: Option<fn(bool)>,
}

struct Session {
    /// The cartridge played.
    cart_path: PathBuf,
    /// The machine, running on its own thread.
    emu: Emu,
    title: String,
    /// When play started (Unix seconds), for Discord's timer.
    started: u64,
    /// Prefix for this game's save-state files.
    state_key: String,
    rewind: std::collections::VecDeque<Vec<u8>>,
    /// Snapshots are taken under the machine lock and compressed on this
    /// worker, so taking one does not stall the picture.
    compress: Compressor,
    last_snapshot: u64,
}

pub struct App {
    opts: StartOptions,
    settings: Settings,
    settings_path: PathBuf,
    session: Option<Session>,
    /// The console view's pointer state (controls held by the mouse).
    console_view: ConsoleView,
    /// Keys and buttons held by each input source.
    held: Held,
    /// The keys a MIDI keyboard holds.
    midi_keys: crate::piano::MidiKeys,
    #[cfg(not(target_os = "android"))]
    midi: crate::midi::Midi,
    gpu: Option<Gpu>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    ui: UiState,
    /// The console's latest picture.
    frame: Vec<u32>,
    paused: bool,
    /// The window is not the active one (paused if `pause_in_background`).
    background: bool,
    fast_forward: bool,
    rewinding: bool,
    modifiers: ModifiersState,
    speed_percent: f64,
    /// For the periodic audio-gap log line.
    audio_log: (Instant, (u64, u64)),
    /// Window frame statistics for the log (`FrameDiag`).
    frame_diag: FrameDiag,
    /// F3's details panel: its frame rates (`Details`).
    details: Details,
    audio: Option<AudioOut>,
    #[cfg(not(target_os = "android"))]
    pads: Option<crate::gamepad::Gamepads>,
    lcd_px: (f32, f32, f32, f32),
    library: Library,
    discord: crate::discord::Discord,
    /// Frames left to size the window around the screen (0 = done).
    fit_window: u8,
    /// When the window was last drawn, and when egui asked to draw it
    /// again (None: not until something happens); `woken`: an event came
    /// in since (see `next_redraw`).
    drawn_at: Instant,
    repaint_at: Option<Instant>,
    woken: bool,
    /// The status line's period: since when, and frames drawn (`log_status`).
    status: (Instant, u32),
    /// The running game's own settings are on: its name and the player's
    /// usual settings (see game_settings.rs).
    game_own: Option<(String, Settings)>,
    /// macOS: the open panel showing as a sheet, until it is answered.
    #[cfg(target_os = "macos")]
    sheet: Option<crate::files::SheetPick>,
    /// Android: what the system's file picker, open now, is choosing.
    native_pick: Option<Pick>,
    /// Android: the side menu was open last frame, and whether opening it
    /// paused the game (closing it then lets it go on).
    side_menu: (bool, bool),
    /// Android: the screen is being kept on (a game is playing).
    screen_on: bool,
    fatal: Option<anyhow::Error>,
    quit: bool,
    /// Everything is saved for good: nothing more is written (after an
    /// update's restart the new version owns the settings).
    shut_down: bool,
    /// The library has opened on the page open last time (once, when its
    /// first scan is in).
    tab_shown: bool,
    /// Active screen recording, or None.
    recording: Option<crate::record::Recorder>,
    /// Background thread encoding the last finished recording, or None.
    encoding: Option<std::thread::JoinHandle<anyhow::Result<crate::record::Saved>>>,
    /// The default recordings folder (Videos), looked up once.
    record_default: PathBuf,
    /// Scratch buffer for audio samples drained each frame while recording.
    rec_audio: Vec<i16>,
    /// The recording shows the console view, drawn off screen at this size
    /// (`Gpu::capture`); None: the console's picture alone.
    rec_view: Option<(u32, u32)>,
    /// The last frame recorded and its size (repeated at the end).
    rec_frame: (Vec<u32>, (usize, usize)),
    /// No audio output: the recording's (silent) sound follows the clock.
    rec_clock: Instant,
    /// Debug: BLUB_RECORD_TEST_SECS=<n> records n seconds of the first
    /// game, from 8 s after it starts, into BLUB_RECORD_TEST_DIR (else the
    /// recordings folder): the seconds, and when the game was first seen.
    record_test: Option<(f64, Option<Instant>)>,
}

/// Runs the desktop app until the window closes.
pub fn run(opts: StartOptions) -> Result<()> {
    let event_loop = EventLoop::new().context("creating event loop")?;
    run_with(event_loop, opts)
}

/// Runs the app on an existing event loop (Android builds its own).
pub fn run_with(event_loop: EventLoop<()>, opts: StartOptions) -> Result<()> {
    crate::perf::prefer_performance_cores();
    let mut app = App::new(opts);
    event_loop.run_app(&mut app)?;
    app.shutdown();
    match app.fatal {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl App {
    pub fn new(opts: StartOptions) -> Self {
        let settings_path = opts.data_dir.join("settings.toml");
        let opts_data_dir = opts.data_dir.clone();
        let mut settings = Settings::load(&settings_path);
        // A different Blub version (an update, or an older one) copies the
        // save states and settings aside before anything can change them.
        if settings.last_version != crate::backup::VERSION {
            crate::backup::before_update(&opts.data_dir, &opts.data_dir.join("states"), &settings.last_version);
            settings.last_version = crate::backup::VERSION.to_string();
            if let Err(e) = settings.save(&settings_path) {
                tracing::warn!(target: "blub::settings", "saving settings: {e:#}");
            }
        }
        // What an in-app update moved aside.
        crate::install::remove_leftovers();
        let mut app = Self {
            opts,
            settings,
            settings_path,
            session: None,
            console_view: ConsoleView::default(),
            held: Held::default(),
            midi_keys: Default::default(),
            #[cfg(not(target_os = "android"))]
            midi: Default::default(),
            gpu: None,
            egui_ctx: egui_context(),
            egui_state: None,
            ui: UiState::new(),
            frame: vec![0; SCREEN_W * SCREEN_H],
            paused: false,
            background: false,
            fast_forward: false,
            rewinding: false,
            modifiers: ModifiersState::empty(),
            speed_percent: 100.0,
            audio_log: (Instant::now(), (0, 0)),
            frame_diag: FrameDiag::default(),
            details: Details::default(),
            audio: AudioOut::open(),
            #[cfg(not(target_os = "android"))]
            pads: crate::gamepad::Gamepads::open(),
            lcd_px: (0.0, 0.0, 0.0, 0.0),
            fit_window: 0,
            drawn_at: Instant::now(),
            repaint_at: None,
            woken: true,
            status: (Instant::now(), 0),
            game_own: None,
            #[cfg(target_os = "macos")]
            sheet: None,
            native_pick: None,
            side_menu: (false, false),
            screen_on: false,
            library: Library::new(&opts_data_dir),
            discord: crate::discord::Discord::start(),
            fatal: None,
            quit: false,
            shut_down: false,
            tab_shown: false,
            recording: None,
            encoding: None,
            record_default: crate::record::default_folder(&opts_data_dir),
            rec_audio: Vec::new(),
            rec_view: None,
            rec_frame: (Vec::new(), (0, 0)),
            rec_clock: Instant::now(),
            record_test: std::env::var("BLUB_RECORD_TEST_SECS").ok().and_then(|v| v.parse().ok()).map(|n| (n, None)),
        };
        // A game given on the command line starts at once; otherwise the
        // game library shows. The library loads meanwhile either way.
        app.rescan_library();
        if let Some(cart) = app.opts.cartridge.clone() {
            app.open_cart(cart);
        }
        app
    }

    /// Where save states are kept.
    fn states_dir(&self) -> PathBuf {
        self.opts.data_dir.join("states")
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.usual_settings().save(&self.settings_path) {
            tracing::warn!(target: "blub::settings", "saving settings: {e:#}");
        }
    }

    /// The settings as kept in settings.toml: the player's usual ones, with
    /// the running game's own (see game_settings.rs) under its name.
    fn usual_settings(&mut self) -> Settings {
        Self::usual_of(&mut self.game_own, &mut self.settings)
    }

    /// `usual_settings` from the two fields (for when the window is borrowed).
    fn usual_of(game_own: &mut Option<(String, Settings)>, settings: &mut Settings) -> Settings {
        let Some((key, usual)) = game_own else {
            return settings.clone();
        };
        let (mut back, own) = crate::game_settings::split(usual, settings);
        back.game_settings.insert(key.clone(), own);
        settings.game_settings = back.game_settings.clone();
        *usual = back.clone();
        back
    }

    /// A game with its own settings starts: they go on top of the usual ones.
    fn enter_game_settings(&mut self, cart: &Path) {
        let Some(key) = crate::game_settings::key(cart) else {
            return;
        };
        let Some(own) = self.settings.game_settings.get(&key) else {
            return;
        };
        tracing::info!(target: "blub::settings", "{key}: its own settings ({} differ)", own.len());
        let playing = crate::game_settings::apply(&self.settings, own);
        let usual = std::mem::replace(&mut self.settings, playing);
        if usual.window_scale != self.settings.window_scale {
            self.fit_window = FIT_TRIES;
        }
        self.game_own = Some((key, usual));
    }

    /// The game closes: the usual settings are back (its own are kept).
    fn leave_game_settings(&mut self) {
        if self.game_own.is_none() {
            return;
        }
        let usual = self.usual_settings();
        self.game_own = None;
        if usual.window_scale != self.settings.window_scale {
            self.fit_window = FIT_TRIES;
        }
        self.settings = usual;
        self.save_settings();
    }

    /// "Custom settings for this game" ticked or unticked.
    fn set_game_settings(&mut self, on: bool) {
        let Some(key) = self.session.as_ref().and_then(|s| crate::game_settings::key(&s.cart_path)) else {
            return;
        };
        if on {
            if self.game_own.is_none() {
                self.settings.game_settings.entry(key.clone()).or_default();
                self.game_own = Some((key, self.settings.clone()));
                self.ui.toast("Changes to the picture, sound, speed and controls are now for this game only");
            }
        } else if self.game_own.is_some() {
            self.leave_game_settings();
            self.settings.game_settings.remove(&key);
            self.apply_settings();
            self.ui.toast("This game uses your usual settings again");
        }
        self.save_settings();
    }

    /// Plays a cartridge (closing the game that runs).
    fn open_cart(&mut self, path: PathBuf) {
        let Some(cart) = crate::library::read_cartridge(&path) else {
            let explain = if path.is_file() {
                "This isn't an I Can Play Piano cartridge dump: those are 8 MB .bin files (or zipped)."
            } else {
                "The file couldn't be read. It may have been moved or deleted."
            };
            return self.ui.problem("Couldn't open the cartridge", explain, path.display().to_string());
        };
        self.end_session();
        self.enter_game_settings(&path);
        let title = crate::library::title_of(&path, &cart);
        let state_key = format!("piano-{:08x}", crc32fast::hash(&cart));
        let mut piano = blub_piano::Piano::new(&cart);
        piano.set_keyboard_mode(self.settings.keyboard_mode);
        let emu = Emu::start(piano);
        emu.set_audio_sink(self.audio.as_ref().map(|a| a.sink()));
        self.settings.add_recent(&path);
        self.save_settings();
        tracing::info!(target: "blub::boot", title = %title, "game started");
        self.session = Some(Session {
            cart_path: path,
            emu,
            title,
            started: now_unix(),
            state_key,
            rewind: Default::default(),
            compress: Compressor::start(),
            last_snapshot: 0,
        });
        self.paused = false;
        if let Some(a) = &mut self.audio {
            a.clear();
        }
        self.update_title();
    }

    fn end_session(&mut self) {
        // Nothing stays held into the next game.
        self.console_view.release_all();
        self.held.clear();
        self.midi_keys.release_all();
        // Fast-forward and rewind are held keys too.
        self.fast_forward = false;
        self.rewinding = false;
        if let Some(s) = self.session.take() {
            s.emu.stop();
            tracing::info!(target: "blub", "game closed");
        }
        self.leave_game_settings();
        self.update_title();
    }

    /// Saves everything, once: after an update's restart the new version is
    /// already running, and a second save here would overwrite its settings.
    fn shutdown(&mut self) {
        if std::mem::replace(&mut self.shut_down, true) {
            return;
        }
        self.end_session();
        self.save_settings();
    }

    /// Rescans the ROM folders (and recent games).
    fn rescan_library(&mut self) {
        self.library.scan(&self.settings.rom_folders, &self.settings.recent_carts.clone());
    }

    fn add_rom_folder(&mut self, path: PathBuf) {
        if !self.settings.rom_folders.contains(&path) {
            self.settings.rom_folders.push(path);
            self.save_settings();
        }
        self.rescan_library();
    }

    /// What changed in the settings reaches the running game.
    fn apply_settings(&mut self) {
        if let Some(s) = &self.session {
            s.emu.lock().set_keyboard_mode(self.settings.keyboard_mode);
        }
        if let Some(a) = &self.audio {
            a.set_enhance(self.settings.audio_enhance);
        }
    }

    /// When to draw the main window next. Every frame while something
    /// runs or moves (a game, a recording, a file dialog); else after
    /// input, when egui asks (an animation, a message), and now and then
    /// for work done in the background (the library scan, the update
    /// check).
    fn next_redraw(&self, now: Instant) -> Instant {
        #[cfg(target_os = "macos")]
        let picking = self.sheet.is_some() || self.native_pick.is_some();
        #[cfg(not(target_os = "macos"))]
        let picking = self.native_pick.is_some();
        let busy = self.session.is_some() || self.recording.is_some() || self.fit_window > 0 || picking;
        if busy || self.woken {
            return now;
        }
        let every = if !self.background { IDLE_REDRAW } else { BACKGROUND_REDRAW };
        let due = self.drawn_at + every;
        self.repaint_at.map_or(due, |at| at.min(due))
    }

    /// Every 30 s, a line in the log (frames drawn, a game running, in the
    /// background), written to disk at once: the last lines before a
    /// system freeze then survive the forced restart.
    fn log_status(&mut self, now: Instant) {
        let span = now.duration_since(self.status.0);
        if span < STATUS_EVERY {
            return;
        }
        tracing::info!(target: "blub::perf",
            fps = (self.status.1 as f64 / span.as_secs_f64()).round(),
            running = self.session.is_some(),
            background = self.background,
            "status (last 30 s)");
        self.status = (now, 0);
        if let Ok(f) = std::fs::OpenOptions::new().append(true).open(self.opts.data_dir.join("blub.log")) {
            let _ = f.sync_data();
        }
    }

    /// The console view's layout, when it is on and a game runs.
    fn view_layout(&self) -> Option<crate::console_view::Layout> {
        (self.settings.console_view && self.session.is_some()).then(crate::console_view::layout)
    }

    /// A key or button pressed or let go by one source (the machine sees it
    /// down while any source holds it).
    fn press(&mut self, source: Source, input: Input, down: bool) {
        let Some(state) = self.held.set(source, input, down) else {
            return;
        };
        if let Some(s) = &self.session {
            let mut p = s.emu.lock();
            match input {
                Input::Key(k) => p.key(k as usize, state),
                Input::Button(b) => p.button(b, state),
            }
        }
    }

    /// Lets go of everything `source` holds.
    fn release(&mut self, source: Source) {
        for i in self.held.release(source) {
            if let Some(s) = &self.session {
                let mut p = s.emu.lock();
                match i {
                    Input::Key(k) => p.key(k as usize, false),
                    Input::Button(b) => p.button(b, false),
                }
            }
        }
    }

    /// What the console view's controls did.
    fn apply_view_events(&mut self, events: Vec<ViewEvent>) {
        for e in events {
            match e {
                ViewEvent::Press(i, down) => self.press(Source::Pointer, i, down),
            }
        }
    }

    /// Lets go of everything held on the keyboard (the window lost the
    /// focus: its key releases go elsewhere).
    fn release_keys(&mut self) {
        self.release(Source::Keyboard);
        self.rewinding = false;
        if self.fast_forward {
            self.fast_forward = false;
            if let Some(s) = &self.session {
                s.emu.resync();
            }
        }
    }

    /// Lets go of every control the mouse holds on the console view.
    fn release_view(&mut self) {
        self.console_view.release_all();
        self.release(Source::Pointer);
    }

    /// MIDI (desktop): the notes played since the last frame, and the port
    /// kept open while a game runs.
    #[cfg(not(target_os = "android"))]
    fn poll_midi(&mut self) {
        let mut events = Vec::new();
        let opened = self.midi.poll(&self.settings.midi_input, self.session.is_some(), &mut events);
        if let Some(name) = opened {
            self.ui.toast(format!("MIDI keyboard connected: {name}"));
        }
        if self.session.is_none() {
            return;
        }
        let shift = self.settings.midi_octave_shift.clamp(-2, 2);
        for e in events {
            match e {
                crate::midi::Event::Note(note, on) => {
                    if let Some((key, down)) = self.midi_keys.note(note, on, shift) {
                        self.press(Source::Midi, Input::Key(key as u8), down);
                    }
                }
                crate::midi::Event::AllOff => {
                    self.midi_keys.release_all();
                    self.release(Source::Midi);
                }
            }
        }
        // The keyboard went away: nothing it held stays down.
        if self.midi.connected().is_none() && self.held.any_pressed() {
            self.midi_keys.release_all();
            self.release(Source::Midi);
        }
    }

    /// F7: the console view on or off.
    fn toggle_console_view(&mut self) {
        self.settings.console_view = !self.settings.console_view;
        self.console_view_changed();
        self.ui.toast(if self.settings.console_view { "Console view on" } else { "Console view off" });
    }

    /// Saved at once; turning it off lets go of what the mouse held. The
    /// game carries on (nothing in the emulation changes).
    fn console_view_changed(&mut self) {
        self.save_settings();
        self.fit_window = FIT_TRIES;
        if self.view_layout().is_none() {
            self.release_view();
        }
    }

    fn pick(&mut self, what: Pick) {
        let start = match what {
            Pick::Cartridge => self.settings.recent_carts.first().cloned(),
            Pick::RomFolder => self.settings.rom_folders.last().cloned(),
            Pick::RecordFolder => self.settings.record_folder.clone(),
        };
        #[cfg(target_os = "macos")]
        if self.sheet.is_none()
            && let Some(g) = &self.gpu
        {
            self.sheet = Some(crate::files::SheetPick::start(what, start.as_deref(), &g.window));
        }
        #[cfg(not(any(target_os = "android", target_os = "macos")))]
        if let Some(path) = crate::files::native_pick(what, start.as_deref()) {
            self.picked(what, path);
        }
        // Android: the system's own picker; Blub's browser if it can't open.
        #[cfg(target_os = "android")]
        if self.opts.pick_start.is_some_and(|open| open(what.is_folder())) {
            self.native_pick = Some(what);
        } else {
            self.ui.browser.start(what, start.as_deref());
        }
    }

    /// Android: acts on the system picker's answer once it is in.
    fn poll_native_pick(&mut self) {
        let (Some(what), Some(poll)) = (self.native_pick, self.opts.pick_poll) else {
            return;
        };
        let Some(answer) = poll() else {
            return;
        };
        self.native_pick = None;
        match answer {
            Ok(Some(path)) => self.picked(what, path),
            Ok(None) => {}
            Err(why) => self.ui.problem("Blub can't use that", why, String::new()),
        }
    }

    fn picked(&mut self, what: Pick, path: PathBuf) {
        match what {
            Pick::Cartridge => self.open_cart(path),
            Pick::RomFolder => self.add_rom_folder(path),
            Pick::RecordFolder => {
                self.settings.record_folder = Some(path);
                self.save_settings();
            }
        }
    }

    /// macOS: acts on the sheet's answer once it is in.
    #[cfg(target_os = "macos")]
    fn poll_sheet(&mut self) {
        let Some(answer) = self.sheet.as_mut().and_then(|s| s.poll()) else {
            return;
        };
        let what = self.sheet.take().map(|s| s.what);
        if let (Some(what), Some(path)) = (what, answer) {
            self.picked(what, path);
        }
    }

    fn update_title(&self) {
        if let Some(g) = &self.gpu {
            let t = match self.session.as_ref().map(|s| &s.title) {
                Some(t) if self.paused => format!("{APP_NAME} - {t} (paused)"),
                Some(t) => format!("{APP_NAME} - {t}"),
                None => APP_NAME.to_string(),
            };
            g.window.set_title(&t);
        }
    }

    fn act(&mut self, a: Action, event_loop: &ActiveEventLoop) {
        match a {
            Action::Pick(w) => self.pick(w),
            Action::OpenCartridge(p) | Action::PlayFromLibrary(p) => self.open_cart(p),
            Action::CloseCartridge => {
                self.end_session();
                // Recently played games join the library.
                self.rescan_library();
            }
            Action::AddRomFolder(p) => self.add_rom_folder(p),
            Action::RemoveRomFolder(p) => {
                self.settings.rom_folders.retain(|f| *f != p);
                self.save_settings();
                self.rescan_library();
            }
            Action::RescanLibrary => self.rescan_library(),
            Action::OpenFolder(path) => open_folder(&path),
            Action::RemakeArt(p) => {
                self.library.forget_art(&p);
                self.rescan_library();
            }
            Action::AskDeleteSave(path, title) => self.ui.ask_delete_save(path, title),
            Action::DeleteSaveData(path) => self.delete_save_data(&path),
            Action::ConsoleViewChanged => self.console_view_changed(),
            Action::SetMidiInput(choice) => {
                self.settings.midi_input = choice;
                self.save_settings();
                #[cfg(not(target_os = "android"))]
                self.midi.rescan();
            }
            Action::RescanMidi => {
                #[cfg(not(target_os = "android"))]
                self.midi.rescan();
            }
            Action::Reset => self.reset(),
            Action::TogglePause => self.toggle_pause(),
            Action::Screenshot => self.screenshot(),
            Action::ToggleFullscreen => self.toggle_fullscreen(),
            Action::SetWindowScale(s) => {
                self.settings.window_scale = s;
                self.save_settings();
                self.fit_window = FIT_TRIES;
            }
            Action::SettingsChanged => {
                self.save_settings();
                self.apply_settings();
            }
            Action::GameSettings(on) => self.set_game_settings(on),
            Action::SaveState(slot) => self.save_state(slot),
            Action::LoadState(slot) => self.load_state(slot),
            Action::RestartForUpdate => {
                // Everything saved before the new version starts and reads it.
                self.shutdown();
                match crate::install::restart() {
                    Ok(()) => {
                        self.quit = true;
                        event_loop.exit();
                    }
                    Err(e) => {
                        // Still running: closing it saves again.
                        self.shut_down = false;
                        self.ui.problem(
                            "The update is installed",
                            "Blub couldn't restart by itself: close it and open it again to use the new version.",
                            e.to_string(),
                        )
                    }
                }
            }
            Action::Quit => {
                self.quit = true;
                event_loop.exit();
            }
            Action::ToggleDrawer => self.ui.drawer = !self.ui.drawer,
            Action::ToggleRecord => self.toggle_record(),
            Action::PickRecordFolder => self.pick(Pick::RecordFolder),
            Action::SetRecordFolder(path) => {
                self.settings.record_folder = Some(path);
                self.save_settings();
            }
            Action::ResetRecordFolder => {
                self.settings.record_folder = None;
                self.save_settings();
            }
            Action::OpenRecordFolder => {
                // Made now if nothing was recorded yet (the file manager
                // would open somewhere else).
                let dir = self.record_dir();
                let _ = std::fs::create_dir_all(&dir);
                open_folder(&dir);
            }
            Action::ToggleFavorite(path) => {
                if let Some(i) = self.settings.favorites.iter().position(|p| p == &path) {
                    self.settings.favorites.remove(i);
                } else {
                    self.settings.favorites.push(path);
                }
                self.save_settings();
            }
            Action::ToggleBuiltinGames(tab) => {
                let name = tab.map_or_else(|| "all".to_string(), |f| f.name().to_string());
                if !self.settings.hide_builtin_tabs.remove(&name) {
                    self.settings.hide_builtin_tabs.insert(name);
                }
                self.save_settings();
            }
            Action::CreateFolder(games) => {
                self.settings.folders.push(crate::settings::LibraryFolder { name: "New Folder".to_string(), games });
                self.save_settings();
                let idx = self.settings.folders.len() - 1;
                self.ui.renaming_folder = Some(idx);
                self.ui.rename_text = "New Folder".to_string();
            }
            Action::OpenLibraryFolder(i) => {
                self.ui.library.open_folder = Some(i);
            }
            Action::CloseLibraryFolder => {
                self.ui.library.open_folder = None;
            }
            Action::RenameFolder { idx, name } => {
                if name.is_empty() {
                    // Empty name = open the rename dialog for this folder.
                    let current = self.settings.folders.get(idx).map_or("", |f| f.name.as_str()).to_string();
                    self.ui.renaming_folder = Some(idx);
                    self.ui.rename_text = current;
                } else {
                    if let Some(f) = self.settings.folders.get_mut(idx) {
                        f.name = name;
                    }
                    self.save_settings();
                    self.ui.renaming_folder = None;
                }
            }
            Action::DeleteFolder(idx) => {
                if idx < self.settings.folders.len() {
                    self.settings.folders.remove(idx);
                }
                self.save_settings();
                if self.ui.library.open_folder == Some(idx) {
                    self.ui.library.open_folder = None;
                }
            }
            Action::AddToFolder { folder, game } => {
                if let Some(f) = self.settings.folders.get_mut(folder)
                    && !f.games.contains(&game)
                {
                    f.games.push(game);
                }
                self.save_settings();
            }
            Action::RemoveFromFolder { folder, game } => {
                if folder == usize::MAX {
                    // Sentinel: remove from whichever folder contains the game.
                    for f in &mut self.settings.folders {
                        f.games.retain(|g| *g != game);
                    }
                } else if let Some(f) = self.settings.folders.get_mut(folder) {
                    f.games.retain(|g| *g != game);
                }
                self.save_settings();
            }
        }
    }

    /// `scale` (picture multiples; 0 the largest), reduced until the window
    /// fits on `screen`.
    fn fitting_scale(scale: u32, screen: Option<PhysicalSize<u32>>) -> u32 {
        let mut scale = if scale == 0 { 6 } else { scale };
        if let Some(screen) = screen {
            // Leave room for the title bar and the taskbar.
            while scale > 1 && (SCREEN_W as u32 * scale > screen.width || SCREEN_H as u32 * scale + 56 + 120 > screen.height) {
                scale -= 1;
            }
        }
        scale
    }

    /// Sizes the window so the screen area (`area`, physical pixels) is
    /// exactly a multiple of `unit` (half the picture, or of the console
    /// view): no bars around the picture, whatever the menu and status bars
    /// measure at this display scaling. `scale` 0 = the largest that fits
    /// the monitor, as a normal window (never maximized), centred. Returns
    /// true once the area fits.
    fn fit_window(window: &Window, scale: u32, area: (f32, f32), unit: (u32, u32)) -> bool {
        if window.fullscreen().is_some() || window.is_maximized() || area.0 < 1.0 || area.1 < 1.0 {
            return true;
        }
        let (inner, outer) = (window.inner_size(), window.outer_size());
        // Rounded up: at fractional display scaling the bars can measure
        // a fraction of a pixel, which would leave the area just short.
        let chrome_w = inner.width.saturating_sub(area.0.floor() as u32);
        let chrome_h = inner.height.saturating_sub(area.1.floor() as u32);
        let frame = (outer.width.saturating_sub(inner.width), outer.height.saturating_sub(inner.height));
        let Some(monitor) = window.current_monitor() else {
            return true;
        };
        let (mon, at) = (monitor.size(), monitor.position());
        // The taskbar (not reported by winit): 48 points plus a margin.
        let taskbar = (56.0 * window.scale_factor()) as u32;
        let room_w = mon.width.saturating_sub(frame.0 + chrome_w);
        let room_h = mon.height.saturating_sub(frame.1 + chrome_h + taskbar);
        // Fit screen leaves some breathing room (about a tenth of the height).
        let room_h = if scale == 0 { room_h * 9 / 10 } else { room_h };
        let most = (room_w / unit.0).min(room_h / unit.1).max(1);
        let times = if scale == 0 { most } else { scale.min(most) };
        let want = PhysicalSize::new(unit.0 * times + chrome_w, unit.1 * times + chrome_h);
        // Never smaller than the window's minimum (the menu bar must fit).
        let min: PhysicalSize<u32> = MIN_WINDOW.to_physical(window.scale_factor());
        let want = PhysicalSize::new(want.width.max(min.width), want.height.max(min.height));
        tracing::debug!(target: "blub::window", ?inner, ?area, chrome_w, chrome_h, times, "fitting the window");
        if want == inner {
            return true;
        }
        let _ = window.request_inner_size(want);
        if scale == 0 {
            let (w, h) = ((want.width + frame.0) as i32, (want.height + frame.1) as i32);
            let x = at.x + (mon.width as i32 - w) / 2;
            let y = at.y + ((mon.height - taskbar) as i32 - h).max(0) / 2;
            window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        }
        false
    }

    fn window_size(scale: u32) -> PhysicalSize<u32> {
        // Room for the menu and status bars.
        PhysicalSize::new(SCREEN_W as u32 * scale, SCREEN_H as u32 * scale + 56)
    }

    /// Deletes a game's save states, moved into the backups folder (the
    /// console keeps nothing of its own).
    fn delete_save_data(&mut self, game: &Path) {
        let Some(cart) = crate::library::read_cartridge(game) else {
            return self.ui.problem("Couldn't delete the save states", "The game file couldn't be read.", game.display().to_string());
        };
        let key = format!("piano-{:08x}", crc32fast::hash(&cart));
        let mut files = Vec::new();
        if let Ok(dir) = std::fs::read_dir(self.states_dir()) {
            files.extend(
                dir.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{key}-slot")))),
            );
        }
        let title = crate::library::title_of(game, &cart);
        match crate::backup::move_to_deleted(&self.opts.data_dir, &files) {
            Ok((0, _)) => self.ui.toast(format!("{title} has no save states")),
            Ok((n, dest)) => {
                tracing::info!(target: "blub::saves", kept = %dest.display(), "deleted {n} save state(s) of {title}");
                self.ui.toast(format!("Save states of {title} deleted (a copy is in the backups folder)"));
            }
            Err(e) => self.ui.problem("Couldn't delete the save states", "A file couldn't be moved.", format!("{e}")),
        }
    }

    fn state_path(&self, slot: u8) -> Option<PathBuf> {
        let key = &self.session.as_ref()?.state_key;
        Some(self.states_dir().join(format!("{key}-slot{slot}.state")))
    }

    /// When each slot was saved, for the menus.
    fn slot_times(&self) -> Vec<Option<String>> {
        (1..=ui::STATE_SLOTS)
            .map(|slot| {
                let modified = std::fs::metadata(self.state_path(slot)?).ok()?.modified().ok()?;
                let secs = modified.elapsed().ok()?.as_secs();
                Some(match secs {
                    0..=59 => "just now".to_string(),
                    60..=3599 => format!("{} min ago", secs / 60),
                    3600..=86_399 => format!("{} h ago", secs / 3600),
                    _ => format!("{} days ago", secs / 86_400),
                })
            })
            .collect()
    }

    fn save_state(&mut self, slot: u8) {
        let (Some(path), Some(s)) = (self.state_path(slot), &self.session) else {
            return;
        };
        let bytes = s.emu.lock().save_state();
        let result = (|| -> Result<()> {
            std::fs::create_dir_all(path.parent().unwrap())?;
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, &path)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.ui.toast(format!("State saved to slot {slot}")),
            Err(e) => self.ui.problem(
                "Couldn't save the state",
                format!("Slot {slot} wasn't saved. Check that the data folder still exists and the disk has room."),
                format!("{e:#}"),
            ),
        }
    }

    fn load_state(&mut self, slot: u8) {
        let (Some(path), Some(s)) = (self.state_path(slot), &self.session) else {
            return;
        };
        let result =
            std::fs::read(&path).map_err(anyhow::Error::from).and_then(|bytes| s.emu.lock().load_state(&bytes).map_err(anyhow::Error::msg));
        match result {
            Ok(()) => {
                s.emu.resync();
                if let Some(a) = &mut self.audio {
                    a.clear();
                }
                self.ui.toast(format!("State loaded from slot {slot}"));
            }
            Err(e) => self.ui.problem(
                "Couldn't load the state",
                format!("Slot {slot} couldn't be loaded. It may be empty, damaged, or saved by another version. The game carries on where it was."),
                format!("{e:#}"),
            ),
        }
    }

    fn reset(&mut self) {
        if let Some(s) = &self.session {
            s.emu.lock().reset();
            s.emu.resync();
            self.ui.toast("Reset");
        }
    }

    fn toggle_pause(&mut self) {
        if let Some(s) = &self.session {
            self.paused = !self.paused;
            s.emu.resync();
            if let Some(a) = &mut self.audio {
                a.clear();
            }
            self.update_title();
        }
    }

    /// Paused by the user, or by being in the background.
    fn halted(&self) -> bool {
        self.paused || (self.background && self.settings.pause_in_background)
    }

    /// The window became active (or not).
    fn focus_changed(&mut self, focused: bool) {
        let was = self.halted();
        self.background = !focused;
        if !focused {
            // Releases may never arrive while another window has the mouse
            // or the keys: nothing on the console view stays held.
            self.release_view();
            self.release_keys();
        }
        if was != self.halted()
            && let Some(s) = &self.session
        {
            s.emu.resync();
            if let Some(a) = &mut self.audio {
                a.clear();
            }
        }
    }

    fn toggle_fullscreen(&mut self) {
        if let Some(g) = &self.gpu {
            let fs = if g.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) };
            g.window.set_fullscreen(fs);
        }
    }

    fn screenshot(&mut self) {
        if self.session.is_none() {
            return;
        }
        let dir = self.opts.data_dir.join("screenshots");
        let path = dir.join(format!("blub-{}.png", now_unix()));
        let result = (|| -> Result<()> {
            std::fs::create_dir_all(&dir)?;
            let file = std::io::BufWriter::new(std::fs::File::create(&path)?);
            let scale = 2;
            let mut enc = png::Encoder::new(file, (SCREEN_W * scale) as u32, (SCREEN_H * scale) as u32);
            enc.set_color(png::ColorType::Rgb);
            let mut w = enc.write_header()?;
            let mut data = Vec::with_capacity(SCREEN_W * SCREEN_H * scale * scale * 3);
            for y in 0..SCREEN_H * scale {
                for x in 0..SCREEN_W * scale {
                    let p = self.frame[(y / scale) * SCREEN_W + x / scale];
                    data.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
                }
            }
            w.write_image_data(&data)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.ui.toast(format!("Screenshot saved: {}", path.display())),
            Err(e) => self.ui.problem(
                "Couldn't save the screenshot",
                format!("Blub couldn't write {}. Check that the folder exists and the disk has room.", path.display()),
                format!("{e:#}"),
            ),
        }
    }

    fn record_test(&mut self) {
        let Some((secs, seen)) = &mut self.record_test else { return };
        if self.session.is_none() {
            return;
        }
        let since = seen.get_or_insert_with(Instant::now).elapsed().as_secs_f64();
        let (start, stop) = (since >= 8.0 && self.recording.is_none(), since >= 8.0 + *secs && self.recording.is_some());
        if stop {
            self.record_test = None;
        }
        if start || stop {
            self.toggle_record();
        }
    }

    /// Where recordings go: the chosen folder, else the player's Videos folder.
    fn record_dir(&self) -> PathBuf {
        if self.record_test.is_some()
            && let Some(dir) = std::env::var_os("BLUB_RECORD_TEST_DIR")
        {
            return dir.into();
        }
        self.settings.record_folder.clone().unwrap_or_else(|| self.record_default.clone())
    }

    /// Says where a finished recording went (or why it didn't).
    fn poll_encoding(&mut self) {
        if !self.encoding.as_ref().is_some_and(|t| t.is_finished()) {
            return;
        }
        let Some(t) = self.encoding.take() else { return };
        match t.join().unwrap_or_else(|_| Err(anyhow::anyhow!("encoding thread panicked"))) {
            Ok(saved) => {
                let name = saved.path.file_name().map_or_else(|| saved.path.display().to_string(), |n| n.to_string_lossy().into_owned());
                let folder = crate::record::folder_of(&saved.path).display();
                let parts = if saved.parts > 1 { format!(" (in {} parts)", saved.parts) } else { String::new() };
                let note = saved.note.map(|n| format!("\n{n}")).unwrap_or_default();
                self.ui.toast_for(format!("Recording saved: {name}{parts} in {folder}{note}"), Duration::from_secs(10));
            }
            Err(e) => self.ui.problem("The recording couldn't be saved", format!("{e:#}"), ""),
        }
    }

    fn toggle_record(&mut self) {
        self.poll_encoding();
        if let Some(mut rec) = self.recording.take() {
            // Stop: flush remaining buffered audio and start encoding.
            let mut remaining = Vec::new();
            if let Some(a) = &self.audio {
                a.stop_record(&mut remaining);
            }
            let _ = rec.add_audio(&remaining);
            let (px, size) = &self.rec_frame;
            for _ in 0..rec.frames_due() {
                let _ = rec.add_frame(px, *size);
            }
            let encoder = self.settings.record_encoder;
            match rec.finish(encoder) {
                Ok(task) => {
                    let folder = crate::record::folder_of(&task.output_path).display();
                    self.ui.toast(format!("Recording stopped: saving it in {folder}…"));
                    self.encoding = Some(std::thread::spawn(move || task.run()));
                }
                Err(e) => self.ui.problem("Recording failed", format!("{e:#}"), ""),
            }
        } else {
            if self.session.is_none() {
                return;
            }
            // Start: the console's picture, or the whole console when its
            // view is shown (the picture at its own size).
            let picture = crate::record::PIANO_SIZE;
            self.rec_view = self.view_layout().map(|l| crate::console_view::capture_size(&l, (picture.1 as f32 / l.screen.height()).ceil() as u32));
            let size = self.rec_view.map_or(picture, |(w, h)| (w as usize, h as usize));
            let dir = self.record_dir();
            // The raw frames wait in the data folder, not the Videos folder.
            let temp = self.opts.data_dir.join("recording-temp");
            match crate::record::Recorder::start(&dir, &temp, self.settings.record_encoder, size) {
                Ok(rec) => {
                    self.ui.toast(format!("Recording… (F8 to stop). It will be saved in {}", dir.display()));
                    if let Some(a) = &self.audio {
                        a.start_record();
                    }
                    self.rec_frame = (Vec::new(), (0, 0));
                    self.rec_clock = Instant::now();
                    self.recording = Some(rec);
                }
                Err(e) => self.ui.problem("Could not start recording", format!("{e:#}"), ""),
            }
        }
    }

    /// Logs (every 5 s) how often the sound ran out.
    fn log_underruns(audio: Option<&AudioOut>, log: &mut (Instant, (u64, u64)), speed_percent: f64) {
        let now = Instant::now();
        let Some(a) = audio else {
            return;
        };
        if now.duration_since(log.0).as_secs() >= 5 {
            let n = a.glitches();
            // The longest wait between the device's callbacks.
            let callback_ms = a.take_longest_gap().as_millis() as u64;
            if n.0 > log.1.0 {
                tracing::info!(target: "blub::audio", gaps = n.0 - log.1.0, speed = speed_percent, callback_ms, "audio ran dry (last 5 s)");
            }
            if n.1 > log.1.1 {
                tracing::info!(target: "blub::audio", skips = n.1 - log.1.1, speed = speed_percent, callback_ms, "audio queue overflowed, sound skipped (last 5 s)");
            }
            let (queued, playing) = a.queue_state();
            tracing::debug!(target: "blub::audio", queued, playing, "audio queue");
            *log = (now, n);
        }
    }

    /// Per frame: tells the emulation thread whether and how fast to run,
    /// and does the UI-side work (rewind, sound, speed meter).
    fn emulate(&mut self) {
        let halted = self.halted();
        let Some(s) = &mut self.session else {
            return;
        };
        let speed = if self.fast_forward || self.settings.speed_percent == 0 {
            f64::INFINITY
        } else {
            self.settings.speed_percent as f64 / 100.0
        };
        let gain = if self.settings.muted { 0.0 } else { self.settings.volume };
        if let Some(a) = &self.audio {
            a.set_enhance(self.settings.audio_enhance);
        }
        s.emu.set_control(Control { running: !halted && !self.rewinding, speed, gain });
        let stats = s.emu.stats();
        // The program turned the console off: the game closes, as a
        // console that turned itself off would end it.
        if stats.powered_off {
            tracing::info!(target: "blub::boot", "the console turned itself off: closing the game");
            self.ui.toast("The piano turned itself off");
            self.end_session();
            self.rescan_library();
            return;
        }
        if halted {
            return;
        }
        if self.rewinding {
            // Step back one snapshot per frame.
            if let Some(state) = s.rewind.pop_back().and_then(|c| miniz_oxide::inflate::decompress_to_vec(&c).ok()) {
                let mut p = s.emu.lock();
                let _ = p.load_state(&state);
                s.last_snapshot = p.frames();
            }
            s.emu.resync();
            if let Some(a) = &mut self.audio {
                a.clear();
            }
            return;
        }
        while let Ok(state) = s.compress.done.try_recv() {
            s.rewind.push_back(state);
            if s.rewind.len() > REWIND_KEEP {
                s.rewind.pop_front();
            }
        }
        {
            let p = s.emu.lock();
            // (Back in time after a loaded state: snapshots from there.)
            s.last_snapshot = s.last_snapshot.min(p.frames());
            if p.frames() >= s.last_snapshot + REWIND_EVERY_FRAMES {
                let _ = s.compress.todo.send(p.save_state());
                s.last_snapshot = p.frames();
            }
        }
        // Sound goes from the emulation thread straight to the output (real-
        // time speed only); at other speeds drop what is queued.
        if speed != 1.0
            && let Some(a) = &mut self.audio
        {
            a.clear();
        }
        self.speed_percent = stats.speed_percent;
        Self::log_underruns(self.audio.as_ref(), &mut self.audio_log, self.speed_percent);
    }

    fn key(&mut self, code: KeyCode, pressed: bool, event_loop: &ActiveEventLoop) {
        if let Some(input) = self.ui.capturing {
            if pressed {
                if code != KeyCode::Escape {
                    if crate::settings::key_from_name(&crate::settings::key_name(code)).is_some() {
                        self.settings.bind(input, code);
                        self.save_settings();
                    } else {
                        self.ui.toast("That key is Blub's own (or can't be used): choose another");
                    }
                }
                self.ui.capturing = None;
            }
            return;
        }
        let ctrl = self.modifiers.control_key();
        if pressed {
            match code {
                KeyCode::Escape => return self.toggle_pause(),
                KeyCode::F1 => return self.ui.show_controls = true,
                KeyCode::F10 => return self.ui.menu_visible = !self.ui.menu_visible,
                KeyCode::F11 => return self.toggle_fullscreen(),
                KeyCode::F8 => return self.toggle_record(),
                KeyCode::F12 => return self.screenshot(),
                KeyCode::F7 => return self.toggle_console_view(),
                KeyCode::F3 => {
                    self.settings.show_details = !self.settings.show_details;
                    return self.save_settings();
                }
                KeyCode::F5 => return self.save_state(1),
                KeyCode::F9 => return self.load_state(1),
                KeyCode::KeyO if ctrl => return self.act(Action::Pick(Pick::Cartridge), event_loop),
                KeyCode::KeyR if ctrl => return self.reset(),
                _ => {}
            }
        }
        if code == KeyCode::Backquote {
            self.rewinding = pressed && self.session.is_some();
            return;
        }
        if code == KeyCode::Tab {
            self.fast_forward = pressed && self.session.is_some();
            if let Some(s) = &self.session {
                s.emu.resync();
            }
            return;
        }
        // Ctrl with a key is Blub's (not a note).
        if ctrl && pressed {
            return;
        }
        if let Some(input) = self.settings.input_for(code) {
            self.press(Source::Keyboard, input, pressed);
        }
    }

    /// Android: the game waits while the side menu is open, as in other
    /// emulators, and goes on when it closes (unless it was paused before).
    fn pause_for_side_menu(&mut self) {
        let open = self.ui.side_menu_open();
        let (was_open, paused_by_menu) = self.side_menu;
        if open && !was_open {
            let pause = self.session.is_some() && !self.paused;
            if pause {
                self.toggle_pause();
            }
            self.side_menu = (true, pause);
        } else if !open && was_open {
            if paused_by_menu && self.paused {
                self.toggle_pause();
            }
            self.side_menu = (false, false);
        }
    }

    /// Android: the screen stays on while a game plays; paused, or in the
    /// library, the phone sleeps as usual.
    fn keep_screen_on(&mut self) {
        let Some(set) = self.opts.keep_screen_on else {
            return;
        };
        let playing = self.session.is_some() && !self.paused;
        if playing != self.screen_on {
            set(playing);
            self.screen_on = playing;
        }
    }

    /// Android gamepad buttons (winit reports them as raw key codes; the
    /// D-pad usually arrives as arrow keys, its own codes are taken too).
    /// The layout matches the desktop's (`gamepad`).
    fn android_key(&mut self, code: u32, pressed: bool) {
        use blub_piano::Button;
        // Back opens and closes the side menu (egui sees it as BrowserBack).
        let button = match code {
            19 => Button::ScrollUp,          // DPAD_UP
            20 => Button::ScrollDown,        // DPAD_DOWN
            96 | 23 => Button::Enter,        // BUTTON_A, DPAD_CENTER
            97 => Button::Home,              // BUTTON_B
            99 => Button::ChangeInstrument,  // BUTTON_X
            100 => Button::CycleHands,       // BUTTON_Y
            108 => Button::Pause,            // BUTTON_START
            102 => Button::TempoDown,        // BUTTON_L1
            103 => Button::TempoUp,          // BUTTON_R1
            _ => return,
        };
        self.press(Source::Gamepad, Input::Button(button), pressed);
    }

    /// Applies controller input (desktop; Android pads arrive as keys).
    #[cfg(not(target_os = "android"))]
    fn poll_pads(&mut self) {
        use crate::gamepad::PadEvent;
        let Some(pads) = &mut self.pads else {
            return;
        };
        let mut events = Vec::new();
        pads.poll(|e| events.push(e));
        for e in events {
            match e {
                PadEvent::Button(b, pressed) => self.press(Source::Gamepad, b, pressed),
                PadEvent::Menu => {
                    self.ui.menu_visible = true;
                    if self.session.is_some() && !self.paused {
                        self.toggle_pause();
                    }
                }
                PadEvent::Connected(name) => self.ui.toast(format!("Controller connected: {name}")),
                PadEvent::Disconnected(name) => self.ui.toast(format!("Controller disconnected: {name}")),
            }
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let frame_start = Instant::now();
        (self.drawn_at, self.woken) = (frame_start, false);
        self.status.1 += 1;
        #[cfg(not(target_os = "android"))]
        {
            self.poll_pads();
            self.poll_midi();
        }
        self.emulate();
        self.poll_encoding();
        self.record_test();
        if let Some(s) = &self.session {
            s.emu.frame(&mut self.frame);
        }
        let halted = self.halted();
        let running = self.session.is_some();
        let slots = self.slot_times();
        let view_layout = self.view_layout();
        // The console view drawn for a recording (`rec_view`): its shapes
        // and where the picture goes.
        let rec_view = self.recording.as_ref().and(self.rec_view);
        let mut rec_shapes = None;
        let size = (SCREEN_W, SCREEN_H);
        // F3: whether the console's picture changed (the game's own frame
        // rate), and the panel's lines.
        let picture = self.settings.show_details.then(|| picture_hash(&self.frame));
        let details = self.settings.show_details.then(|| self.details_lines());
        let (Some(gpu), Some(state)) = (&mut self.gpu, &mut self.egui_state) else {
            return;
        };
        if running {
            gpu.upload(&self.frame, size);
        }
        let raw = state.take_egui_input(&gpu.window);
        #[cfg(not(target_os = "android"))]
        let (midi_ports, midi_connected) = (self.midi.ports().to_vec(), self.midi.connected().map(str::to_string));
        #[cfg(target_os = "android")]
        let (midi_ports, midi_connected) = (Vec::new(), None);
        let info = ui::Info {
            running,
            game_settings: running.then_some(self.game_own.is_some()),
            paused: halted,
            background: !self.paused && halted,
            title: self.session.as_ref().map(|s| s.title.clone()).unwrap_or_default(),
            speed: self.speed_percent,
            details,
            fullscreen: gpu.window.fullscreen().is_some(),
            save_dir: self.opts.data_dir.join("states").display().to_string(),
            slots,
            rewinding: self.rewinding,
            recording: self.recording.as_ref().map(|r| r.started.elapsed().as_secs()),
            encoding: self.encoding.as_ref().is_some_and(|t| !t.is_finished()),
            record_dir: self.settings.record_folder.clone().unwrap_or_else(|| self.record_default.clone()),
            midi_ports,
            midi_connected,
        };
        let mut actions = Vec::new();
        let mut area = egui::Rect::NOTHING;
        self.library.poll();
        // At start the library opens on the page open last time (once the
        // first scan is in, so the page has its games); after that the open
        // page is remembered.
        if !self.tab_shown {
            if !self.library.scanning && !self.library.games.is_empty() {
                self.tab_shown = true;
                let last = crate::library::Family::ALL.into_iter().find(|f| f.name() == self.settings.library_tab);
                if self.ui.library.tab.is_none() {
                    self.ui.library.tab = last;
                }
            }
        } else {
            let open = self.ui.library.tab.map_or("", |f| f.name()).to_string();
            if open != self.settings.library_tab {
                self.settings.library_tab = open;
                if let Err(e) = Self::usual_of(&mut self.game_own, &mut self.settings).save(&self.settings_path) {
                    tracing::warn!(target: "blub::settings", "saving settings: {e:#}");
                }
            }
        }
        self.discord.set(match &self.session {
            _ if !self.settings.discord_presence => crate::discord::Presence::Off,
            Some(s) => crate::discord::Presence::playing(&s.title, CONSOLE, s.started),
            None => crate::discord::Presence::Browsing,
        });
        let (ui_state, settings, library) = (&mut self.ui, &mut self.settings, &mut self.library);
        let (view, held) = (&mut self.console_view, &self.held);
        // Where the picture is inside the console view (None: the plain view).
        let mut screen = None;
        let mut view_events = Vec::new();
        let out = self.egui_ctx.run_ui(raw, |ui| {
            // The console is painted into this slot, under the menus and
            // messages drawn next.
            let under = ui.painter().add(egui::Shape::Noop);
            area = ui::draw(ui, ui_state, settings, library, &info, &mut actions);
            if let (true, Some(layout)) = (info.running, &view_layout) {
                let pressed = |i: Input| held.pressed(i);
                let keys = |i: Input| settings.keys_for(i).into_iter().map(crate::settings::key_label).collect::<Vec<_>>().join(" / ");
                let cap = |i: Input| settings.keys_for(i).first().map(|k| crate::settings::cap_text(k));
                let lit = crate::console_view::Lit {
                    pressed: &pressed,
                    cap: &cap,
                    show_caps: settings.console_view_keys,
                    keys: &keys,
                    whole_pixels: settings.display == crate::gpu::Display::Pixel,
                };
                if let Some(size) = rec_view {
                    rec_shapes = Some(crate::console_view::capture_shapes(layout, size, &lit, ui.ctx()));
                }
                let (at, events) = view.show(ui, under, layout, area, &lit);
                screen = Some(at);
                view_events = events;
            }
        });
        // Duration::MAX (nothing to animate) overflows: no time asked.
        self.repaint_at = out.viewport_output.get(&egui::ViewportId::ROOT).and_then(|v| frame_start.checked_add(v.repaint_delay));
        state.handle_platform_output(&gpu.window, out.platform_output);
        let ppp = out.pixels_per_point;
        if self.fit_window > 0 {
            self.fit_window -= 1;
            let (unit, scale) = match &view_layout {
                // The whole console: sized so its picture gets the scale.
                Some(l) => {
                    let h = SCREEN_H as f32 / 2.0 / l.screen.height() * 1.04;
                    (((h * l.aspect).round() as u32, h.round() as u32), self.settings.window_scale * 2)
                }
                // Half the picture: a whole scale is two of these.
                None => (((SCREEN_W / 2) as u32, (SCREEN_H / 2) as u32), self.settings.window_scale * 2),
            };
            if Self::fit_window(&gpu.window, scale, (area.width() * ppp, area.height() * ppp), unit) {
                self.fit_window = 0;
            }
        }
        let display = self.settings.display;
        let at = screen.unwrap_or(area);
        let px_area = (at.min.x * ppp, at.min.y * ppp, at.width() * ppp, at.height() * ppp);
        self.lcd_px = Gpu::picture_rect(px_area, size, display);
        let jobs = self.egui_ctx.tessellate(out.shapes, ppp);
        let lcd = running.then_some((self.lcd_px, display, self.settings.ai));
        let mut deltas = out.textures_delta;
        let rendered = gpu.render(lcd, &jobs, &deltas, ppp);
        // Handled by `render` (debug builds of egui panic when an unhandled
        // delta is dropped).
        deltas.clear();
        if let Err(e) = rendered {
            self.fatal = Some(e);
            event_loop.exit();
            return;
        }
        // Screen recording: this frame and the sound since the last.
        if let Some(rec) = &mut self.recording {
            // No sound arrives without an output device, or while paused:
            // silence then (stereo frames), as long as the time that passed,
            // so the picture goes on (frames follow the sound).
            let quiet = self.rec_clock.elapsed();
            if let Some(a) = &self.audio {
                a.drain_record(&mut self.rec_audio);
            }
            let silent = self.audio.is_none() || quiet >= Duration::from_millis(100);
            if self.rec_audio.is_empty() && silent && !halted {
                let frames = (quiet.as_secs_f64() * rec.audio_rate as f64) as usize;
                self.rec_audio.resize(frames * crate::record::CHANNELS, 0);
            }
            if !self.rec_audio.is_empty() || halted {
                self.rec_clock = Instant::now();
            }
            let _ = rec.add_audio(&self.rec_audio);
            self.rec_audio.clear();
            let due = rec.frames_due();
            if due > 0 {
                let captured = match (rec_shapes, rec_view) {
                    (Some((shapes, screen)), Some(rsize)) => {
                        let at = (screen.min.x, screen.min.y, screen.width(), screen.height());
                        let lcd = Gpu::picture_rect(at, size, display);
                        let clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(rsize.0 as f32, rsize.1 as f32));
                        let shapes = shapes.into_iter().map(|shape| egui::epaint::ClippedShape { clip_rect: clip, shape }).collect();
                        let jobs = self.egui_ctx.tessellate(shapes, 1.0);
                        gpu.capture(rsize, lcd, display, &jobs).map(|px| (px, (rsize.0 as usize, rsize.1 as usize)))
                    }
                    _ => None,
                };
                self.rec_frame = captured.unwrap_or_else(|| (self.frame.clone(), crate::record::PIANO_SIZE));
                for _ in 0..due {
                    let _ = rec.add_frame(&self.rec_frame.0, self.rec_frame.1);
                }
            }
        }
        self.apply_view_events(view_events);
        for a in actions {
            self.act(a, event_loop);
        }
        self.pause_for_side_menu();
        self.keep_screen_on();
        let took = frame_start.elapsed();
        if took.as_millis() >= 12 {
            tracing::debug!(target: "blub::frame", ms = took.as_secs_f64() * 1e3, "slow frame (excluding vsync wait)");
        }
        self.frame_diag.frame(frame_start, took, self.session.is_some() && !self.halted());
        match picture {
            Some(hash) => self.details.frame(frame_start, hash, took),
            None => self.details = Details::default(),
        }
    }

    /// F3's lines.
    fn details_lines(&self) -> Vec<(&'static str, String)> {
        let d = &self.details;
        let ms = |t: Duration| t.as_secs_f64() * 1e3;
        let mut lines = vec![("Blub", env!("CARGO_PKG_VERSION").to_string()), ("Console", CONSOLE.to_string())];
        let rate = |v: f64| if d.measured { format!("{v:.0}") } else { "…".into() };
        lines.push(("Game fps", rate(d.shown.0)));
        lines.push(("Window fps", rate(d.shown.1)));
        lines.push(("Frame work", format!("{:.1} ms (longest {:.1})", ms(d.shown.2), ms(d.shown.3))));
        let state = if self.halted() {
            " (paused)"
        } else if self.fast_forward {
            " (fast-forward)"
        } else {
            ""
        };
        lines.push(("Speed", format!("{:.0}%{state}", self.speed_percent)));
        lines.push(("Picture", format!("{SCREEN_W} x {SCREEN_H}")));
        if let Some(gpu) = &self.gpu {
            let (name, (ww, wh)) = gpu.summary();
            lines.push(("Window", format!("{ww} x {wh}")));
            lines.push(("GPU", name));
        }
        lines.push((
            "Sound",
            match &self.audio {
                Some(a) => {
                    let (gaps, skips) = a.glitches();
                    format!("{gaps} gaps, {skips} skips")
                }
                None => "no output".into(),
            },
        ));
        #[cfg(not(target_os = "android"))]
        lines.push(("MIDI", self.midi.connected().unwrap_or("none").to_string()));
        lines
    }
}

/// A cheap fingerprint of a picture, to see when it changes (FNV-1a over
/// the pixels).
fn picture_hash(pixels: &[u32]) -> u64 {
    pixels.iter().fold(0xCBF2_9CE4_8422_2325, |h, &p| (h ^ p as u64).wrapping_mul(0x0000_0100_0000_01B3))
}

/// F3's frame rates over each half second: how often the console's picture
/// changed (the game's own frame rate) and how often the window was drawn,
/// with the window's own work per frame.
#[derive(Default)]
struct Details {
    since: Option<Instant>,
    last_picture: u64,
    redraws: u32,
    pictures: u32,
    work: Duration,
    longest_work: Duration,
    /// A half second has been measured.
    measured: bool,
    /// The last half second: game fps, window fps, average and longest work.
    shown: (f64, f64, Duration, Duration),
}

impl Details {
    fn frame(&mut self, now: Instant, picture: u64, work: Duration) {
        let since = *self.since.get_or_insert(now);
        self.redraws += 1;
        if picture != self.last_picture {
            self.pictures += 1;
            self.last_picture = picture;
        }
        self.work += work;
        self.longest_work = self.longest_work.max(work);
        let span = now - since;
        if span >= Duration::from_millis(500) {
            let s = span.as_secs_f64();
            self.shown = (self.pictures as f64 / s, self.redraws as f64 / s, self.work / self.redraws, self.longest_work);
            self.measured = true;
            (self.since, self.redraws, self.pictures, self.work, self.longest_work) = (Some(now), 0, 0, Duration::ZERO, Duration::ZERO);
        }
    }
}

/// With nothing running or moving, the window is drawn at least this
/// often: in front, and in the background (`next_redraw`).
const IDLE_REDRAW: Duration = Duration::from_millis(250);
const BACKGROUND_REDRAW: Duration = Duration::from_secs(1);
/// How often `log_status` writes its line.
const STATUS_EVERY: Duration = Duration::from_secs(30);

/// The window's frame rate, its longest gap between frames and its own
/// work per frame, logged every 10 s while a game runs (for reports of
/// slowness: is the window, or the emulation, falling behind?).
#[derive(Default)]
struct FrameDiag {
    since: Option<Instant>,
    last: Option<Instant>,
    frames: u32,
    longest_gap: Duration,
    work: Duration,
    longest_work: Duration,
}

impl FrameDiag {
    fn frame(&mut self, start: Instant, work: Duration, running: bool) {
        if !running {
            *self = FrameDiag::default();
            return;
        }
        let since = *self.since.get_or_insert(start);
        if let Some(last) = self.last {
            self.longest_gap = self.longest_gap.max(start - last);
        }
        self.last = Some(start);
        self.frames += 1;
        self.work += work;
        self.longest_work = self.longest_work.max(work);
        let span = start - since;
        if span >= Duration::from_secs(10) {
            let ms = |d: Duration| (d.as_secs_f64() * 1e4).round() / 10.0;
            tracing::info!(target: "blub::perf",
                fps = (self.frames as f64 / span.as_secs_f64()).round(),
                longest_gap_ms = ms(self.longest_gap),
                work_ms = ms(self.work / self.frames.max(1)),
                longest_work_ms = ms(self.longest_work),
                "window (last 10 s)");
            *self = FrameDiag { since: Some(start), last: Some(start), ..Default::default() };
        }
    }
}

/// An egui context with Blub's fonts and look.
pub(crate) fn egui_context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::fonts::install(&ctx);
    // Buttons keep their size when pointed at or pressed (egui grows them a
    // pixel, so their text jumped); selections and links in Blub's blue.
    ctx.all_styles_mut(|s| {
        let w = &mut s.visuals.widgets;
        for state in [&mut w.hovered, &mut w.active, &mut w.open] {
            state.expansion = 0.0;
        }
        s.visuals.selection.bg_fill = egui::Color32::from_rgb(36, 96, 180);
        s.visuals.hyperlink_color = egui::Color32::from_rgb(110, 170, 250);
    });
    ctx
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Back from the background (Android): a new window for the GPU kept
        // from before, with every texture still on it.
        if let Some(gpu) = &mut self.gpu {
            if self.egui_state.is_some() {
                return;
            }
            let reattached = event_loop
                .create_window(Window::default_attributes().with_title(APP_NAME))
                .context("creating window")
                .and_then(|w| gpu.reattach(Arc::new(w)));
            match reattached {
                Ok(()) => {
                    self.egui_state = Some(egui_winit::State::new(
                        self.egui_ctx.clone(),
                        egui::ViewportId::ROOT,
                        &gpu.window,
                        Some(gpu.window.scale_factor() as f32),
                        None,
                        None,
                    ));
                    gpu.window.request_redraw();
                }
                Err(e) => {
                    self.fatal = Some(e);
                    event_loop.exit();
                }
            }
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(APP_NAME)
            // Never maximized or full screen at start: a normal window, the
            // chosen size or the largest that fits the screen.
            .with_inner_size(Self::window_size(Self::fitting_scale(
                self.settings.window_scale,
                event_loop.primary_monitor().map(|m| m.size()),
            )))
            .with_min_inner_size(MIN_WINDOW)
            .with_window_icon(crate::icon::window_icon());
        #[cfg(windows)]
        let attrs = {
            use winit::platform::windows::WindowAttributesExtWindows as _;
            attrs.with_taskbar_icon(crate::icon::taskbar_icon())
        };
        match event_loop.create_window(attrs).context("creating window").and_then(|w| Gpu::new(Arc::new(w), Some(&self.opts.data_dir.join(GPU_GUARD)))) {
            Ok(gpu) => {
                self.fit_window = FIT_TRIES;
                let state = egui_winit::State::new(
                    self.egui_ctx.clone(),
                    egui::ViewportId::ROOT,
                    &gpu.window,
                    Some(gpu.window.scale_factor() as f32),
                    None,
                    None,
                );
                self.egui_state = Some(state);
                self.gpu = Some(gpu);
                self.update_title();
            }
            Err(e) => {
                self.fatal = Some(e);
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android: the surface goes away when the app is backgrounded.
        if let Some(s) = &self.session {
            s.emu.resync();
        }
        self.save_settings();
        // Only the surface goes (the window is being taken away); the GPU
        // and its textures (the UI's fonts, the game art) stay for when the
        // app comes back.
        if let Some(g) = &mut self.gpu {
            g.detach();
        }
        self.egui_state = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let consumed = match (&mut self.egui_state, &self.gpu) {
            (Some(st), Some(g)) => st.on_window_event(&g.window, &event).consumed,
            _ => false,
        };
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.woken = true;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gpu {
                    g.resize(size);
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Focused(f) => self.focus_changed(f),
            WindowEvent::DroppedFile(path) => self.open_cart(path),
            // Synthetic events are winit's stand-ins for keys held while the
            // focus moved: presses for the keys down as the window gains it,
            // releases as it loses it. Both are left out: losing the focus
            // lets go of every key itself (`focus_changed`).
            WindowEvent::KeyboardInput { is_synthetic: true, .. } => {}
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, repeat: false, .. }, .. } => {
                let pressed = state == ElementState::Pressed;
                // A release always reaches the game: egui reports Tab as
                // consumed and focuses a widget with it, so Tab's release was
                // dropped and fast-forward stayed on (letting go of a key not
                // held does nothing).
                if pressed && consumed && self.egui_ctx.egui_wants_keyboard_input() {
                    return;
                }
                match physical_key {
                    PhysicalKey::Code(code) => self.key(code, pressed, event_loop),
                    PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Android(code)) => {
                        self.android_key(code, pressed)
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Android: only asked in the background, as the activity is only
        // destroyed after its window has gone.
        if self.egui_state.is_none() && self.opts.destroyed.is_some_and(|destroyed| destroyed()) {
            event_loop.exit();
            return;
        }
        #[cfg(target_os = "macos")]
        self.poll_sheet();
        self.poll_native_pick();
        let now = Instant::now();
        let next = self.next_redraw(now);
        if let Some(g) = &self.gpu
            && next <= now
        {
            g.window.request_redraw();
        }
        event_loop.set_control_flow(if next <= now {
            winit::event_loop::ControlFlow::Wait
        } else {
            winit::event_loop::ControlFlow::WaitUntil(next)
        });
        self.log_status(now);
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // macOS's Quit (⌘Q, the app menu) ends the process right after this,
        // without returning from run_app: save here, not only after it.
        self.shutdown();
    }
}

/// A worker thread that compresses rewind snapshots.
struct Compressor {
    todo: std::sync::mpsc::Sender<Vec<u8>>,
    done: std::sync::mpsc::Receiver<Vec<u8>>,
}

impl Compressor {
    fn start() -> Self {
        let (todo, jobs) = std::sync::mpsc::channel::<Vec<u8>>();
        let (results, done) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("blub-rewind".into())
            .spawn(move || {
                for raw in jobs {
                    if results.send(miniz_oxide::deflate::compress_to_vec(&raw, 1)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawning the rewind worker");
        Self { todo, done }
    }
}

/// Shows a folder in the system's file manager.
fn open_folder(path: &Path) {
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    if let Err(e) = std::process::Command::new(cmd).arg(path).spawn() {
        tracing::warn!(target: "blub", "opening {}: {e}", path.display());
    }
}
