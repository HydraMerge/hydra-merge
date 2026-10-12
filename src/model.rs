//! Data types, configuration and error codes shared by all modules.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::trust::TrustStore;

pub const BARE_DIRNAME: &str = ".bare";
pub const DEFAULT_ROOTS: [&str; 1] = ["~/repos"];
/// Discovery roots, separated by `:` on Unix and `;` on Windows.
pub const ENV_ROOTS: &str = "HYDRA_MERGE_ROOTS";
pub const ENV_SMERGE: &str = "HYDRA_MERGE_SMERGE";
pub use crate::os::DEFAULT_SMERGE;
pub const MAX_DEPTH: usize = 4;
pub const GIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Push, pull and fetch may wait on the network.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
pub const SMERGE_TIMEOUT: Duration = Duration::from_secs(10);
pub const SKIP_DIRS: [&str; 3] = [".bare", ".git", "node_modules"];
/// Upper bound for the stderr text kept in `AppError::detail`.
pub const DETAIL_LIMIT: usize = 2000;

#[derive(Clone, Debug)]
pub struct Config {
    /// Canonicalized, existing, without duplicates, order preserved.
    pub roots: Vec<PathBuf>,
    pub smerge: PathBuf,
    /// Extra variables for every git call (tests: GIT_CONFIG_GLOBAL and similar).
    pub git_env: Vec<(OsString, OsString)>,
    /// Repositories the user trusted although their config can run programs. Clones of the
    /// config share it.
    pub trusted: TrustStore,
}

impl Config {
    /// Reads the home directory, HYDRA_MERGE_ROOTS and HYDRA_MERGE_SMERGE.
    pub fn from_env() -> Config {
        let home = crate::os::home_dir().unwrap_or_default();
        let roots = std::env::var_os(ENV_ROOTS);
        let smerge = std::env::var_os(ENV_SMERGE);
        Config::from_values(&home, roots.as_deref(), smerge.as_deref())
    }

    /// Pure: no process environment is read.
    pub fn from_values(
        home: &Path,
        roots_var: Option<&OsStr>,
        smerge_var: Option<&OsStr>,
    ) -> Config {
        let raw: Vec<OsString> = match roots_var {
            Some(var) => split_roots(var),
            None => DEFAULT_ROOTS.iter().map(OsString::from).collect(),
        };
        let mut roots: Vec<PathBuf> = Vec::new();
        for item in &raw {
            let Some(expanded) = expand_tilde(home, item) else {
                continue;
            };
            let Ok(canonical) = std::fs::canonicalize(expanded) else {
                continue;
            };
            if !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }
        let smerge = match smerge_var {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            _ => crate::os::default_smerge(),
        };
        Config {
            roots,
            smerge,
            git_env: Vec::new(),
            trusted: TrustStore::default(),
        }
    }
}

fn split_roots(var: &OsStr) -> Vec<OsString> {
    std::env::split_paths(var)
        .filter(|part| !part.as_os_str().is_empty())
        .map(|part| part.into_os_string())
        .collect()
}

/// `~` and `~/...` use `home`. An empty `home` makes tilde paths unusable.
fn expand_tilde(home: &Path, item: &OsStr) -> Option<PathBuf> {
    let path = Path::new(item);
    match path.strip_prefix("~") {
        Ok(_) if home.as_os_str().is_empty() => None,
        Ok(rest) => Some(home.join(rest)),
        Err(_) => Some(path.to_path_buf()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Bare,
    Plain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    /// Canonicalized when the directory exists, otherwise the porcelain path.
    pub path: PathBuf,
    /// "feature/foo"; None for detached and bare entries.
    pub branch: Option<String>,
    /// 7 hex characters; None for bare.
    pub head: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    /// None for bare and prunable entries.
    pub dirty: Option<bool>,
    pub layout_path: Option<PathBuf>,
    pub layout_correct: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    /// Canonicalized repository root (toplevel for plain clones).
    pub root: PathBuf,
    /// Path of `root` relative to its discovery root, e.g. "group/repo".
    pub name: String,
    pub layout: Layout,
    pub git_dir: PathBuf,
    pub worktrees: Vec<Worktree>,
    /// What in the repository's own config and hooks can run a program (empty for most).
    pub code_sources: Vec<String>,
    /// Fingerprint of `code_sources` and the values behind them; what "trust" is bound to.
    pub trust_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidBranch,
    NotARepo,
    OutsideRoots,
    UnknownRepo,
    UnknownWorktree,
    BareNotOpenable,
    ParentNotOpenable,
    BranchNotFound,
    BranchInUse,
    DestExists,
    DestNested,
    DirtyWorktree,
    LockedWorktree,
    MainWorktree,
    PlainLayout,
    GitFailed,
    GitTimeout,
    MergeNotFound,
    /// git needed a password or key and could not ask for it.
    AuthRequired,
    /// The repository's own config or hooks can run programs and the user has not trusted them.
    UntrustedRepo,
    Io,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> AppError {
        AppError {
            code,
            message: message.into(),
            detail: None,
        }
    }

    /// Attaches stderr text, truncated to `DETAIL_LIMIT` characters. Blank text is dropped.
    pub fn with_detail(self, detail: &str) -> AppError {
        let trimmed = detail.trim();
        if trimmed.is_empty() {
            return self;
        }
        let cut: String = trimmed.chars().take(DETAIL_LIMIT).collect();
        AppError {
            detail: Some(cut),
            ..self
        }
    }

    pub fn io(operation: &str, err: &io::Error) -> AppError {
        AppError::new(ErrorCode::Io, format!("{operation}: {err}"))
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;
