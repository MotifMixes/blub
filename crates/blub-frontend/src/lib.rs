//! Blub's user interface, shared by the desktop app and the Android app:
//! the window, the emulated screen, menus and dialogs (egui), input, sound
//! output, settings and save states. Nothing here changes emulation.

pub mod app;
mod audio_out;
mod backup;
mod record;
mod console_icons;
mod console_view;
mod discord;
mod emu;
mod files;
#[cfg(not(target_os = "android"))]
mod gamepad;
mod game_settings;
mod fonts;
mod gpu;
mod held;
mod icon;
mod install;
mod library;
pub mod logging;
mod manual;
#[cfg(not(target_os = "android"))]
mod midi;
mod perf;
pub mod piano;
pub mod settings;
mod ui;
mod updates;
pub mod upscale;
pub mod zip;

pub use app::{StartOptions, run, run_with};
pub use settings::default_data_dir;
