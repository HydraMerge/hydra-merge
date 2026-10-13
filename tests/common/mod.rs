//! Fixtures: temporary repositories only. Nothing outside the TempDir is read or changed.
#![allow(dead_code)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use hydra_merge::model::Config;

pub const SMERGE_CALLS: &str = "smerge-calls.txt";

/// A TempDir plus its canonical path (on macOS `/var` is a link to `/private/var`).
pub struct Tmp {
    pub dir: TempDir,
    pub path: PathBuf,
}

pub fn tmp() -> Tmp {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = fs::canonicalize(dir.path()).expect("canonicalize tempdir");
    fs::create_dir_all(path.join("home")).expect("create fake home");
    Tmp { dir, path }
}

fn os_pair(key: &str, value: impl AsRef<OsStr>) -> (OsString, OsString) {
    (OsString::from(key), value.as_ref().to_os_string())
}

pub fn test_config(tmp: &Path) -> Config {
    let root = fs::canonicalize(tmp).expect("canonicalize tmp");
    let git_env = vec![
        os_pair("GIT_CONFIG_GLOBAL", "/dev/null"),
        os_pair("GIT_CONFIG_NOSYSTEM", "1"),
        os_pair("HOME", root.join("home")),
        os_pair("GIT_AUTHOR_NAME", "Test"),
        os_pair("GIT_AUTHOR_EMAIL", "test@example.com"),
        os_pair("GIT_COMMITTER_NAME", "Test"),
        os_pair("GIT_COMMITTER_EMAIL", "test@example.com"),
    ];
    Config {
        smerge: root.join("fake-smerge"),
        roots: vec![root],
        git_env,
        trusted: Default::default(),
    }
}

/// Runs git with the test environment; panics on failure.
pub fn git(config: &Config, args: &[&OsStr]) -> String {
    let out = Command::new("git")
        .args(args)
        .envs(config.git_env.iter().map(|(k, v)| (k, v)))
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn g(config: &Config, args: &[&str]) -> String {
    let os: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git(config, &os)
}

/// `<tmp>/.seed`: one commit on `master`. The name starts with a dot so discovery skips it.
pub fn make_seed(tmp: &Path, config: &Config) -> PathBuf {
    let seed = tmp.join(".seed");
    if seed.exists() {
        return seed;
    }
    let seed_str = seed.to_str().expect("utf8 path");
    g(config, &["init", "-b", "master", seed_str]);
    fs::write(seed.join("README"), "seed\n").expect("write README");
    g(config, &["-C", seed_str, "add", "README"]);
    g(config, &["-C", seed_str, "commit", "-m", "init"]);
    seed
}

/// `<tmp>/<rel>/.bare` + one folder per entry of `worktrees`. Returns the canonical repo root.
pub fn make_bare_layout(
    tmp: &Path,
    config: &Config,
    rel: &str,
    branches: &[&str],
    worktrees: &[&str],
) -> PathBuf {
    let seed = make_seed(tmp, config);
    let root = tmp.join(rel);
    let bare = root.join(".bare");
    let bare_arg = format!("--git-dir={}", bare.display());
    g(
        config,
        &[
            "clone",
            "--bare",
            seed.to_str().expect("utf8"),
            bare.to_str().expect("utf8"),
        ],
    );
    for branch in branches.iter().filter(|b| **b != "master") {
        g(config, &[&bare_arg, "branch", branch, "master"]);
    }
    for branch in worktrees {
        let dest = root.join(branch);
        g(
            config,
            &[
                &bare_arg,
                "worktree",
                "add",
                dest.to_str().expect("utf8"),
                branch,
            ],
        );
    }
    fs::canonicalize(root).expect("canonicalize repo root")
}

pub fn make_plain_clone(tmp: &Path, config: &Config, name: &str) -> PathBuf {
    let seed = make_seed(tmp, config);
    let dest = tmp.join(name);
    g(
        config,
        &[
            "clone",
            seed.to_str().expect("utf8"),
            dest.to_str().expect("utf8"),
        ],
    );
    fs::canonicalize(dest).expect("canonicalize clone")
}

/// Executable that appends its arguments, one per line, to `<tmp>/smerge-calls.txt`.
pub fn fake_smerge(tmp: &Path) -> PathBuf {
    let script = tmp.join("fake-smerge");
    let calls = tmp.join(SMERGE_CALLS);
    let body = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\n",
        calls.display()
    );
    fs::write(&script, body).expect("write fake smerge");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod fake smerge");
    script
}

pub fn smerge_calls(tmp: &Path) -> Option<Vec<String>> {
    let text = fs::read_to_string(tmp.join(SMERGE_CALLS)).ok()?;
    Some(text.lines().map(str::to_string).collect())
}
