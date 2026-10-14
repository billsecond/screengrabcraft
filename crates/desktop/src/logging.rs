//! The log: `log::` records to standard error and to `<settings>/logs/<program>.log`.
//!
//! A program started from the tray or at sign-in has no terminal, so the file is what a bug
//! report can attach. Each start moves the previous log to `<program>.1.log` (and that one to
//! `.2`), so the log of a run that crashed survives the next start. The file stops growing at
//! 16 MB. `RUST_LOG` sets the level (`debug`, `warn`, …); by default ScreenGrabCraft's own
//! crates log at info and everything else at warn. Runs without settings log to standard error
//! only.

use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock, PoisonError};

use log::{LevelFilter, Metadata, Record};

const MAX_BYTES: u64 = 16 * 1024 * 1024;

struct Logger {
    file: Mutex<Option<(File, u64)>>,
    ours: LevelFilter,
    others: LevelFilter,
}

impl log::Log for Logger {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        let limit = if m.target().starts_with("screengrabcraft") { self.ours } else { self.others };
        m.level() <= limit
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let now = crate::time::rfc3339_now();
        let line = format!("{now} {:<5} {}: {}\n", record.level(), record.target(), record.args());
        let _ = std::io::stderr().write_all(line.as_bytes());
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((f, written)) = guard.as_mut()
            && *written < MAX_BYTES
            && f.write_all(line.as_bytes()).is_ok()
        {
            *written += line.len() as u64;
        }
    }

    fn flush(&self) {
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((f, _)) = guard.as_mut() {
            let _ = f.flush();
        }
    }
}

fn level(text: &str) -> Option<LevelFilter> {
    match text.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Starts logging for the program `name` (the log file's name).
pub fn init(name: &str) {
    let env = std::env::var("RUST_LOG").ok().and_then(|v| level(&v));
    let (ours, others) = match env {
        Some(l) => (l, l),
        None => (LevelFilter::Info, LevelFilter::Warn),
    };
    let file = if crate::paths::no_prefs() {
        None
    } else {
        crate::paths::log_dir().and_then(|dir| {
            std::fs::create_dir_all(&dir).ok()?;
            let current = dir.join(format!("{name}.log"));
            let older = |n: u32| dir.join(format!("{name}.{n}.log"));
            let _ = std::fs::rename(older(1), older(2));
            let _ = std::fs::rename(&current, older(1));
            File::create(&current).ok().map(|f| (f, 0u64))
        })
    };
    let logger = LOGGER.get_or_init(|| Logger { file: Mutex::new(file), ours, others });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(ours.max(others));
    }
    install_panic_hook();
}

/// Reports every panic through the log (a panic in a worker thread would otherwise vanish).
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        log::error!("panic at {at}: {what}");
        log::logger().flush();
        previous(info);
    }));
}
