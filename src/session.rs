//! The window as it was left: size and place, tabs, selection, panel widths and options.
//! Kept in `<config dir>/session.txt`, one `key<TAB>value` per line; unknown keys are ignored.

use std::fs;
use std::path::{Path, PathBuf};

use crate::datefmt::DateFormat;
use crate::model::{AppError, AppResult};

const SESSION_FILENAME: &str = "session.txt";
/// Larger sizes or positions in the file are treated as damaged.
const MAX_WINDOW: f32 = 10_000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    /// Outer window rectangle: x, y, width, height.
    pub window: Option<[f32; 4]>,
    pub active_root: Option<PathBuf>,
    pub selected: Option<PathBuf>,
    pub closed_tabs: Vec<PathBuf>,
    /// Plain clones added with "Open path…".
    pub extra_roots: Vec<PathBuf>,
    /// Folders the user chose to scan. Empty means the default discovery roots.
    pub scan_roots: Vec<PathBuf>,
    /// Log detail kept between launches. `None` means the default, info.
    pub log_level: Option<crate::log::Level>,
    pub history_width: Option<f32>,
    pub locations_open: bool,
    pub auto_fetch: bool,
    /// How commit dates are written.
    pub date_format: DateFormat,
    /// Commands run from the palette, newest first.
    pub recent_commands: Vec<String>,
    /// The app Open uses (`.app` bundle path); `None` means Sublime Text.
    pub editor_app: Option<PathBuf>,
    /// Repositories the user trusted: root and the fingerprint of what could run there.
    pub trusted: Vec<(PathBuf, String)>,
}

impl Default for Session {
    fn default() -> Session {
        Session {
            window: None,
            active_root: None,
            selected: None,
            closed_tabs: Vec::new(),
            extra_roots: Vec::new(),
            scan_roots: Vec::new(),
            log_level: None,
            history_width: None,
            locations_open: true,
            auto_fetch: false,
            date_format: DateFormat::default(),
            recent_commands: Vec::new(),
            editor_app: None,
            trusted: Vec::new(),
        }
    }
}

pub fn session_file(dir: &Path) -> PathBuf {
    dir.join(SESSION_FILENAME)
}

/// A value that fits on one line of the file.
fn line_safe(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\n', '\r', '\t'])
}

impl Session {
    pub fn to_text(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let mut put = |key: &str, value: String| {
            if line_safe(&value) {
                lines.push(format!("{key}\t{value}"));
            }
        };
        if let Some([x, y, w, h]) = self.window {
            put("window", format!("{x} {y} {w} {h}"));
        }
        let path = |p: &Path| p.to_string_lossy().into_owned();
        self.active_root.iter().for_each(|p| put("active", path(p)));
        self.selected.iter().for_each(|p| put("selected", path(p)));
        self.closed_tabs.iter().for_each(|p| put("closed", path(p)));
        self.extra_roots.iter().for_each(|p| put("extra", path(p)));
        self.scan_roots.iter().for_each(|p| put("scan", path(p)));
        if let Some(level) = self.log_level {
            put("log_level", level.as_str().to_string());
        }
        self.history_width
            .iter()
            .for_each(|w| put("history_width", w.to_string()));
        put(
            "locations",
            if self.locations_open { "on" } else { "off" }.to_string(),
        );
        put(
            "auto_fetch",
            if self.auto_fetch { "on" } else { "off" }.to_string(),
        );
        put("date_format", self.date_format.key().to_string());
        self.recent_commands
            .iter()
            .for_each(|c| put("recent", c.clone()));
        self.editor_app
            .iter()
            .for_each(|p| put("editor_app", path(p)));
        for (root, fingerprint) in &self.trusted {
            put("trusted", format!("{fingerprint} {}", path(root)));
        }
        lines.join("\n") + "\n"
    }

    pub fn parse(text: &str) -> Session {
        text.lines()
            .filter_map(|line| line.split_once('\t'))
            .fold(Session::default(), |session, (key, value)| {
                session.with(key, value)
            })
    }

    /// `self` with one stored line applied; bad values are skipped.
    fn with(mut self, key: &str, value: &str) -> Session {
        let path = || PathBuf::from(value);
        match key {
            "window" => self.window = parse_window(value).or(self.window),
            "active" => self.active_root = Some(path()),
            "selected" => self.selected = Some(path()),
            "closed" => self.closed_tabs.push(path()),
            "extra" => self.extra_roots.push(path()),
            "scan" => self.scan_roots.push(path()),
            "log_level" => self.log_level = crate::log::Level::parse(value),
            "history_width" => {
                self.history_width = value
                    .parse()
                    .ok()
                    .filter(|w: &f32| w.is_finite() && *w > 0.0)
            }
            "locations" => self.locations_open = value != "off",
            "auto_fetch" => self.auto_fetch = value == "on",
            "date_format" => {
                self.date_format = DateFormat::from_key(value).unwrap_or(self.date_format);
            }
            "recent" => self.recent_commands.push(value.to_string()),
            "editor_app" => self.editor_app = Some(path()),
            "trusted" => {
                if let Some((fingerprint, root)) = value.split_once(' ') {
                    if !fingerprint.is_empty() && !root.is_empty() {
                        self.trusted
                            .push((PathBuf::from(root), fingerprint.to_string()));
                    }
                }
            }
            _ => {}
        }
        self
    }
}

fn parse_window(value: &str) -> Option<[f32; 4]> {
    let numbers: Vec<f32> = value.split(' ').filter_map(|n| n.parse().ok()).collect();
    let [x, y, w, h] = numbers.as_slice() else {
        return None;
    };
    let size_ok = (400.0..=MAX_WINDOW).contains(w) && (300.0..=MAX_WINDOW).contains(h);
    let place_ok = [x, y].iter().all(|n| n.abs() <= MAX_WINDOW);
    let sane = [x, y, w, h].iter().all(|n| n.is_finite()) && size_ok && place_ok;
    sane.then_some([*x, *y, *w, *h])
}

/// The stored session, or the defaults when there is none or it cannot be read.
pub fn load_session(dir: &Path) -> Session {
    fs::read_to_string(session_file(dir))
        .map(|text| Session::parse(&text))
        .unwrap_or_default()
}

/// Writes through a temporary file and a rename, so a crash never leaves half a session.
pub fn save_session(dir: &Path, session: &Session) -> AppResult<()> {
    fs::create_dir_all(dir)
        .map_err(|e| AppError::io(&format!("cannot create {}", dir.display()), &e))?;
    let file = session_file(dir);
    let partial = dir.join(format!("{SESSION_FILENAME}.partial"));
    fs::write(&partial, session.to_text())
        .map_err(|e| AppError::io(&format!("cannot write {}", partial.display()), &e))?;
    fs::rename(&partial, &file)
        .map_err(|e| AppError::io(&format!("cannot write {}", file.display()), &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        Session {
            window: Some([10.0, 20.0, 1400.0, 820.0]),
            active_root: Some(PathBuf::from("/r/group/repo")),
            selected: Some(PathBuf::from("/r/group/repo/feature")),
            closed_tabs: vec![PathBuf::from("/r/a"), PathBuf::from("/r/b")],
            extra_roots: vec![PathBuf::from("/r/plain")],
            scan_roots: vec![PathBuf::from("/r/scan")],
            log_level: Some(crate::log::Level::Debug),
            history_width: Some(420.5),
            locations_open: false,
            auto_fetch: true,
            date_format: DateFormat::UsNumeric,
            recent_commands: vec!["Push".to_string(), "Worktree: master".to_string()],
            editor_app: Some(PathBuf::from("/Applications/Visual Studio Code.app")),
            trusted: vec![(
                PathBuf::from("/r/with space/repo"),
                "0123456789abcdef".to_string(),
            )],
        }
    }

    #[test]
    fn round_trip_keeps_everything() {
        assert_eq!(Session::parse(&sample().to_text()), sample());
        assert_eq!(
            Session::parse(&Session::default().to_text()),
            Session::default()
        );
    }

    #[test]
    fn junk_and_bad_values_are_ignored() {
        assert_eq!(
            Session::parse("window\t0 0 1e9 800\n").window,
            None,
            "huge window"
        );
        let text = "window\t1 2 3\nhistory_width\tNaN\nmystery\tx\nno tab here\nactive\t/r/x\n";
        let session = Session::parse(text);
        assert_eq!(session.window, None);
        assert_eq!(session.history_width, None);
        assert_eq!(session.active_root, Some(PathBuf::from("/r/x")));
    }

    #[test]
    fn an_unknown_date_format_keeps_the_default() {
        assert_eq!(Session::parse("date_format\tklingon\n").date_format, DateFormat::Europe);
        assert_eq!(Session::parse("date_format\tiso\n").date_format, DateFormat::Iso);
    }

    #[test]
    fn trusted_lines_need_a_fingerprint_and_a_path() {
        let session = Session::parse("trusted\tonlyonetoken\ntrusted\t \ntrusted\tabc /r/x\n");
        assert_eq!(
            session.trusted,
            vec![(PathBuf::from("/r/x"), "abc".to_string())]
        );
    }

    #[test]
    fn file_round_trip_and_missing_file() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("cfg");
        assert_eq!(load_session(&dir), Session::default());
        save_session(&dir, &sample()).expect("save");
        assert_eq!(load_session(&dir), sample());
        assert!(!dir.join("session.txt.partial").exists());
    }
}
