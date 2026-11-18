//! The log: to stderr, and for the windowed app to a file as well (players
//! send it with bug reports). RUST_LOG overrides the filter.

use tracing_subscriber::EnvFilter;

/// Logs to stderr at `default_filter` ("info").
pub fn init(default_filter: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(true).with_writer(std::io::stderr).try_init();
}

/// Logs to stderr and to `log` (made anew); stderr alone if it can't be.
pub fn init_to_file(default_filter: &str, log: &std::path::Path) {
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    let Some(file) = log.parent().and_then(|d| std::fs::create_dir_all(d).ok()).and_then(|()| std::fs::File::create(log).ok())
    else {
        return init(default_filter);
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let writer = std::io::stderr.and(std::sync::Mutex::new(file));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(true).with_ansi(false).with_writer(writer).try_init();
}
