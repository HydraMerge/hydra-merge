//! Which repositories may be read: one whose own config or hooks can run a program is left alone
//! until the user trusts exactly that config.
//!
//! A repository that arrives as a whole folder (an archive, a copy) brings its own `config`,
//! `hooks/` and per-worktree config. Git runs what they name: clean and smudge filters on
//! `status` and `diff`, the ssh command on `fetch`, credential helpers, merge drivers, hooks. The
//! user's own settings and the settings the app switches off by itself (`core.fsmonitor`,
//! `log.showSignature`, textconv) are not part of this.
//!
//! Trust is a fingerprint of what can run, kept per repository root. Change a value and it no
//! longer matches, so the question is asked again.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::git::{git_dir_arg, run_git};
use crate::model::{AppError, AppResult, Config, ErrorCode, Repo};

/// Keys of a repository's config that make git start a program, as git prints them (section and
/// variable lowercase). Matched with `git config --get-regexp`.
const CODE_KEYS: &str = concat!(
    r"^(core\.(sshcommand|gitproxy|hookspath|askpass|alternaterefscommand)",
    r"|credential\..*helper",
    r"|filter\..*\.(clean|smudge|process)",
    r"|merge\..*\.driver",
    r"|remote\..*\.(uploadpack|receivepack)",
    r"|protocol\..*allow)$"
);

/// Hook files above this size are fingerprinted by name and size only.
const HOOK_READ_LIMIT: u64 = 1 << 20;

/// What in a repository can run a program, and a fingerprint of exactly that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeSources {
    /// Config keys and `hooks/<name>` files, sorted, for the window.
    pub names: Vec<String>,
    /// Changes when a name or any value (a command line, a hook's content) changes.
    pub fingerprint: String,
}

impl CodeSources {
    /// Shown for a repository whose config could not be read; it stays blocked.
    fn unreadable() -> CodeSources {
        CodeSources {
            names: vec!["unreadable repository config".to_string()],
            fingerprint: "unreadable".to_string(),
        }
    }
}

/// FNV-1a, 64 bit: small, and stable across Rust versions, unlike `DefaultHasher`.
fn fnv1a(parts: &[&[u8]]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in parts.iter().flat_map(|part| part.iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// `git config --get-regexp -z` of the repository's own config, followed through `include`
/// directives as git follows them. Records are `key\nvalue\0`; exit status 1 means none.
fn config_records(config: &Config, git_dir: &Path, cwd: &Path) -> AppResult<Vec<String>> {
    let dir_arg = git_dir_arg(git_dir);
    let args = [
        dir_arg.as_os_str(),
        OsStr::new("config"),
        OsStr::new("--local"),
        OsStr::new("--includes"),
        OsStr::new("-z"),
        OsStr::new("--get-regexp"),
        OsStr::new(CODE_KEYS),
    ];
    records_of(config, &args, cwd)
}

/// The same for one extra config file (per-worktree config), which `--local` does not read.
fn file_records(config: &Config, file: &Path, cwd: &Path) -> AppResult<Vec<String>> {
    let args = [
        OsStr::new("config"),
        OsStr::new("--file"),
        file.as_os_str(),
        OsStr::new("--includes"),
        OsStr::new("-z"),
        OsStr::new("--get-regexp"),
        OsStr::new(CODE_KEYS),
    ];
    records_of(config, &args, cwd)
}

fn records_of(config: &Config, args: &[&OsStr], cwd: &Path) -> AppResult<Vec<String>> {
    let out = run_git(config, args, cwd)?;
    match out.status {
        Some(0) => Ok(out
            .stdout
            .split('\0')
            .filter(|record| !record.is_empty())
            .map(str::to_string)
            .collect()),
        Some(1) => Ok(Vec::new()),
        _ => Err(
            AppError::new(ErrorCode::GitFailed, "cannot read the repository config")
                .with_detail(&out.stderr),
        ),
    }
}

/// `config.worktree` of the repository and of each linked worktree. Git reads them only when
/// `extensions.worktreeConfig` is on, which the repository config itself can switch on.
fn worktree_config_files(git_dir: &Path) -> Vec<PathBuf> {
    let mut files = vec![git_dir.join("config.worktree")];
    if let Ok(entries) = fs::read_dir(git_dir.join("worktrees")) {
        files.extend(entries.flatten().map(|e| e.path().join("config.worktree")));
    }
    files.retain(|file| file.is_file());
    files.sort();
    files
}

/// Hooks that would run: regular files that are executable, other than `*.sample`.
fn hook_files(git_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(git_dir.join("hooks")) else {
        return Vec::new();
    };
    let mut hooks: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let runs = crate::os::file_is_executable(&entry.path());
            (runs && !name.ends_with(".sample")).then(|| (name, entry.path()))
        })
        .collect();
    hooks.sort();
    hooks
}

/// Content of a hook for the fingerprint: the bytes, or name and size when it is large.
fn hook_digest(path: &Path) -> String {
    let size = fs::metadata(path).map_or(0, |meta| meta.len());
    let mut bytes = Vec::new();
    let read = fs::File::open(path)
        .and_then(|file| file.take(HOOK_READ_LIMIT).read_to_end(&mut bytes))
        .is_ok();
    if read && size <= HOOK_READ_LIMIT {
        fnv1a(&[&bytes])
    } else {
        format!("size-{size}")
    }
}

/// What can run in the repository whose git directory is `git_dir`.
pub fn inspect(config: &Config, git_dir: &Path, cwd: &Path) -> AppResult<CodeSources> {
    let mut records = config_records(config, git_dir, cwd)?;
    for file in worktree_config_files(git_dir) {
        let found = file_records(config, &file, cwd)?;
        let label = file.strip_prefix(git_dir).map_or_else(
            |_| file.display().to_string(),
            |rel| rel.display().to_string(),
        );
        records.extend(found.into_iter().map(|record| format!("{label}:{record}")));
    }
    let mut names: Vec<String> = records
        .iter()
        .map(|record| record.split('\n').next().unwrap_or_default().to_string())
        .collect();
    let mut digest: Vec<String> = records;
    for (name, path) in hook_files(git_dir) {
        digest.push(format!("hooks/{name}\n{}", hook_digest(&path)));
        names.push(format!("hooks/{name}"));
    }
    names.sort();
    names.dedup();
    digest.sort();
    let parts: Vec<&[u8]> = digest.iter().map(|d| d.as_bytes()).collect();
    Ok(CodeSources {
        names,
        fingerprint: fnv1a(&parts),
    })
}

/// [`inspect`], or the blocked "unreadable" answer when git cannot read the config.
pub fn inspect_or_block(config: &Config, git_dir: &Path, cwd: &Path) -> CodeSources {
    inspect(config, git_dir, cwd).unwrap_or_else(|_| CodeSources::unreadable())
}

/// Repositories the user trusted, by root, with the fingerprint that was shown. Clones share one
/// store, so a decision made in the window reaches the background threads at once.
#[derive(Clone, Debug, Default)]
pub struct TrustStore(Arc<Mutex<HashMap<PathBuf, String>>>);

impl TrustStore {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn trust(&self, root: &Path, fingerprint: &str) {
        self.map()
            .insert(root.to_path_buf(), fingerprint.to_string());
    }

    pub fn revoke(&self, root: &Path) {
        self.map().remove(root);
    }

    /// `true` only for the fingerprint the user trusted for this very root.
    pub fn is_trusted(&self, root: &Path, fingerprint: &str) -> bool {
        self.map()
            .get(root)
            .is_some_and(|known| known == fingerprint)
    }

    pub fn is_known(&self, root: &Path) -> bool {
        self.map().contains_key(root)
    }

    /// Sorted by path, for the session file.
    pub fn entries(&self) -> Vec<(PathBuf, String)> {
        let mut list: Vec<(PathBuf, String)> = self
            .map()
            .iter()
            .map(|(root, fp)| (root.clone(), fp.clone()))
            .collect();
        list.sort();
        list
    }
}

fn refusal(root: &Path, names: &[String]) -> AppError {
    AppError::new(
        ErrorCode::UntrustedRepo,
        format!(
            "{} is not trusted: its own config or hooks can run programs. \
             Trust it in the window to continue.",
            root.display()
        ),
    )
    .with_detail(&names.join("\n"))
}

fn check(config: &Config, root: &Path, sources: &CodeSources) -> AppResult<()> {
    if sources.names.is_empty() || config.trusted.is_trusted(root, &sources.fingerprint) {
        Ok(())
    } else {
        Err(refusal(root, &sources.names))
    }
}

/// Checks the repository as it was read a moment ago (its own `code_sources`). Use it on a
/// `Repo` that was just built.
pub fn require_trusted(config: &Config, repo: &Repo) -> AppResult<()> {
    let sources = CodeSources {
        names: repo.code_sources.clone(),
        fingerprint: repo.trust_key.clone(),
    };
    check(config, &repo.root, &sources)
}

/// Reads the repository config again and checks that. Use it on a `Repo` kept by the window,
/// which may be older than the config on disk.
pub fn ensure_trusted(config: &Config, repo: &Repo) -> AppResult<()> {
    check(
        config,
        &repo.root,
        &inspect_or_block(config, &repo.git_dir, &repo.root),
    )
}

/// `true` when git may be run in the repository: it holds nothing that runs, or it is trusted.
pub fn allows_git(config: &Config, root: &Path, git_dir: &Path) -> bool {
    check(config, root, &inspect_or_block(config, git_dir, root)).is_ok()
}

/// What the window shows: the repository was read as holding code that is not trusted.
pub fn is_blocked(config: &Config, repo: &Repo) -> bool {
    require_trusted(config, repo).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_depends_on_every_part() {
        assert_eq!(fnv1a(&[b"a", b"b"]), fnv1a(&[b"ab"]));
        assert_eq!(fnv1a(&[b"x"]).len(), 16);
        assert_ne!(fnv1a(&[b"x"]), fnv1a(&[b"y"]));
        assert_eq!(fnv1a(&[b"abc"]), "e71fa2190541574b");
    }

    #[test]
    fn store_matches_root_and_fingerprint_together() {
        let store = TrustStore::default();
        let root = Path::new("/r/a");
        assert!(!store.is_trusted(root, "f1"));
        store.trust(root, "f1");
        assert!(store.is_trusted(root, "f1"));
        assert!(!store.is_trusted(root, "f2"), "another fingerprint");
        assert!(!store.is_trusted(Path::new("/r/b"), "f1"), "another root");
        let shared = store.clone();
        shared.revoke(root);
        assert!(!store.is_trusted(root, "f1"), "clones share one store");
    }

    #[test]
    fn entries_are_sorted() {
        let store = TrustStore::default();
        store.trust(Path::new("/b"), "2");
        store.trust(Path::new("/a"), "1");
        let paths: Vec<PathBuf> = store.entries().into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }
}
