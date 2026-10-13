//! A single log file in the settings directory. The level is chosen in the window.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const FILENAME: &str = "hydra-merge.log";

/// How much is written. A level also includes the more serious ones above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }

    pub fn parse(text: &str) -> Option<Level> {
        match text {
            "error" => Some(Level::Error),
            "warn" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            _ => None,
        }
    }

    pub const ALL: [Level; 4] = [Level::Error, Level::Warn, Level::Info, Level::Debug];
}

struct State {
    level: Level,
    file: Option<File>,
    path: Option<PathBuf>,
}

static STATE: Mutex<State> = Mutex::new(State {
    level: Level::Info,
    file: None,
    path: None,
});

/// Opens `<dir>/hydra-merge.log` for append. Later records also go there.
pub fn open(dir: &Path) -> PathBuf {
    let path = dir.join(FILENAME);
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    if let Ok(mut state) = STATE.lock() {
        state.path = Some(path.clone());
        state.file = file;
    }
    path
}

pub fn set_level(level: Level) {
    if let Ok(mut state) = STATE.lock() {
        state.level = level;
    }
}

pub fn level() -> Level {
    STATE.lock().map(|state| state.level).unwrap_or(Level::Info)
}

pub fn path() -> Option<PathBuf> {
    STATE.lock().ok().and_then(|state| state.path.clone())
}

pub fn record(level: Level, args: std::fmt::Arguments<'_>) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if (level as u8) > (state.level as u8) {
        return;
    }
    let line = format!("{} {} {args}", stamp(), level.label());
    eprintln!("{line}");
    if let Some(file) = state.file.as_mut() {
        let _ = writeln!(file, "{line}");
    }
}

fn stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let time = secs % 86_400;
    let (year, month, day) = civil_date(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time / 60) % 60,
        time % 60
    )
}

/// Days since 1970-01-01 to a UTC calendar date.
fn civil_date(days: u64) -> (i32, u32, u32) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log::record($crate::log::Level::Error, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log::record($crate::log::Level::Warn, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::record($crate::log::Level::Info, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::log::record($crate::log::Level::Debug, format_args!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level and the file are process-wide, so the tests take turns.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn a_level_keeps_the_more_serious_lines() {
        let _guard = lock();
        set_level(Level::Warn);
        assert!(Level::Error as u8 <= level() as u8);
        assert!(Level::Info as u8 > level() as u8);
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("verbose"), None);
    }

    #[test]
    fn the_file_receives_a_line_at_the_chosen_level() {
        let _guard = lock();
        let dir = tempfile::tempdir().expect("dir");
        let path = open(dir.path());
        set_level(Level::Info);
        record(Level::Debug, format_args!("hidden"));
        record(Level::Info, format_args!("shown"));
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(!text.contains("hidden"));
        assert!(text.contains("INFO shown"));
        assert_eq!(super::path().as_deref(), Some(path.as_path()));
    }
}
