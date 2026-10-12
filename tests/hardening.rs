//! A repository is data, not code: its file names, commit text and local config must not make the
//! app delete work, forge rows or run programs. Temporary repositories only.

#![cfg(unix)]

mod common;

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use common::{git, make_bare_layout, test_config, tmp, Tmp};
use hydra_merge::discover::discover_repos;
use hydra_merge::git::{
    branches, discard_file, fetch_repo, inspect_worktree, stage_file, unstage_file,
};
use hydra_merge::model::Config;

struct Fixture {
    tmp: Tmp,
    config: Config,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    let root = make_bare_layout(&tmp.path, &config, "grp/repo", &["master"], &["master"]);
    Fixture { tmp, config, root }
}

impl Fixture {
    fn wt(&self) -> PathBuf {
        self.root.join("master")
    }

    /// `git -C <worktree> <args>` with the test environment.
    fn git(&self, args: &[&str]) -> String {
        let wt = self.wt();
        let mut all: Vec<&OsStr> = vec![OsStr::new("-C"), wt.as_os_str()];
        all.extend(args.iter().map(OsStr::new));
        git(&self.config, &all)
    }

    /// An executable that records every run in `<tmp>/<name>.marker` and echoes its first
    /// argument if that is a file (so it also works as a textconv driver).
    fn marker_script(&self, name: &str) -> PathBuf {
        let marker = self.marker(name);
        let script = self.tmp.path.join(format!("{name}.sh"));
        let body = format!(
            "#!/bin/sh\necho ran >> '{}'\n[ -f \"$1\" ] && cat \"$1\"\nexit 0\n",
            marker.display()
        );
        fs::write(&script, body).expect("write script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        script
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.tmp.path.join(format!("{name}.marker"))
    }

    fn set_config(&self, key: &str, value: &str) {
        self.git(&["config", key, value]);
    }

    /// Stores a commit object built from raw bytes and moves `master` to it.
    fn commit_raw(&self, extra_headers: &str, message: &[u8]) {
        let tree = self.git(&["rev-parse", "HEAD^{tree}"]).trim().to_string();
        let parent = self.git(&["rev-parse", "HEAD"]).trim().to_string();
        let mut body = format!(
            "tree {tree}\nparent {parent}\nauthor Test <test@example.com> 1700000000 +0000\n\
             committer Test <test@example.com> 1700000000 +0000\n{extra_headers}\n"
        )
        .into_bytes();
        body.extend_from_slice(message);
        let wt = self.wt();
        let mut child = Command::new("git")
            .args(["-C"])
            .arg(&wt)
            .args([
                "hash-object",
                "--literally",
                "-t",
                "commit",
                "-w",
                "--stdin",
            ])
            .envs(self.config.git_env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn hash-object");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(&body)
            .expect("write commit");
        let out = child.wait_with_output().expect("hash-object");
        assert!(out.status.success(), "hash-object failed");
        let oid = String::from_utf8_lossy(&out.stdout).trim().to_string();
        self.git(&["update-ref", "refs/heads/master", &oid]);
    }
}

fn commit_files(f: &Fixture, files: &[(&str, &str)]) {
    for (name, text) in files {
        fs::write(f.wt().join(name), text).expect("write file");
    }
    let mut add = vec!["--literal-pathspecs", "add", "--"];
    add.extend(files.iter().map(|(name, _)| *name));
    f.git(&add);
    f.git(&["commit", "-m", "fixtures"]);
}

// ---- file names are literal paths, not pathspecs -------------------------------------------

#[test]
fn discard_matches_the_file_name_literally() {
    let f = fixture();
    commit_files(&f, &[("*", "star\n"), ("work.txt", "base\n")]);
    fs::write(f.wt().join("*"), "star changed\n").expect("edit star");
    fs::write(f.wt().join("work.txt"), "my unsaved work\n").expect("edit work");

    discard_file(&f.config, &[], &f.wt(), "*").expect("discard");

    assert_eq!(fs::read_to_string(f.wt().join("*")).unwrap(), "star\n");
    assert_eq!(
        fs::read_to_string(f.wt().join("work.txt")).unwrap(),
        "my unsaved work\n",
        "a file named '*' must not discard other files"
    );
}

#[test]
fn discard_does_not_treat_a_name_as_pathspec_magic() {
    let f = fixture();
    commit_files(
        &f,
        &[(":(exclude)keep.txt", "magic\n"), ("work.txt", "base\n")],
    );
    fs::write(f.wt().join(":(exclude)keep.txt"), "magic changed\n").expect("edit magic");
    fs::write(f.wt().join("work.txt"), "my unsaved work\n").expect("edit work");

    discard_file(&f.config, &[], &f.wt(), ":(exclude)keep.txt").expect("discard");

    assert_eq!(
        fs::read_to_string(f.wt().join("work.txt")).unwrap(),
        "my unsaved work\n"
    );
}

#[test]
fn stage_of_a_star_file_does_not_stage_other_untracked_files() {
    let f = fixture();
    fs::write(f.wt().join("*"), "star\n").expect("write star");
    fs::write(f.wt().join(".env.local"), "SECRET=1\n").expect("write secret");

    stage_file(&f.config, &[], &f.wt(), Some("*")).expect("stage");

    let status = f.git(&["status", "--porcelain=v1"]);
    assert!(
        status.contains("?? .env.local"),
        "unrelated file was staged: {status}"
    );
}

#[test]
fn unstage_of_a_star_file_keeps_other_staged_files() {
    let f = fixture();
    fs::write(f.wt().join("*"), "star\n").expect("write star");
    fs::write(f.wt().join("keep.txt"), "keep\n").expect("write keep");
    f.git(&["--literal-pathspecs", "add", "--", "*", "keep.txt"]);

    unstage_file(&f.config, &[], &f.wt(), "*").expect("unstage");

    let status = f.git(&["status", "--porcelain=v1"]);
    assert!(
        status.contains("A  keep.txt"),
        "keep.txt unstaged: {status}"
    );
}

// ---- repo-local config must not run programs during reads ----------------------------------

#[test]
fn reading_a_repo_does_not_run_its_fsmonitor() {
    let f = fixture();
    let script = f.marker_script("fsmonitor");
    f.set_config("core.fsmonitor", script.to_str().expect("utf8"));

    inspect_worktree(&f.config, &[], &f.wt()).expect("inspect");
    let repos = discover_repos(&f.config).expect("discover");
    branches(&f.config, &repos[0]).expect("branches");

    assert!(
        !f.marker("fsmonitor").exists(),
        "core.fsmonitor from the repository config ran"
    );
}

#[test]
fn reading_a_repo_does_not_run_its_textconv_driver() {
    let f = fixture();
    commit_files(&f, &[("a.txt", "one\n")]);
    fs::write(f.wt().join(".gitattributes"), "*.txt diff=x\n").expect("attributes");
    let script = f.marker_script("textconv");
    f.set_config("diff.x.textconv", script.to_str().expect("utf8"));
    fs::write(f.wt().join("a.txt"), "two\n").expect("edit a.txt");

    inspect_worktree(&f.config, &[], &f.wt()).expect("inspect");
    let hash = f.git(&["rev-parse", "HEAD"]);
    hydra_merge::git::commit_patch(&f.config, &[], &f.wt(), hash.trim()).expect("patch");

    assert!(
        !f.marker("textconv").exists(),
        "diff textconv from the repository config ran"
    );
}

#[test]
fn reading_a_repo_does_not_run_its_gpg_program() {
    let f = fixture();
    let script = f.marker_script("gpg");
    f.set_config("log.showSignature", "true");
    f.set_config("gpg.program", script.to_str().expect("utf8"));
    f.commit_raw(
        "gpgsig -----BEGIN PGP SIGNATURE-----\n \n AAAA\n -----END PGP SIGNATURE-----\n",
        b"signed\n",
    );

    inspect_worktree(&f.config, &[], &f.wt()).expect("inspect");
    let hash = f.git(&["rev-parse", "HEAD"]);
    hydra_merge::git::commit_patch(&f.config, &[], &f.wt(), hash.trim()).expect("patch");

    assert!(
        !f.marker("gpg").exists(),
        "gpg.program from the repository config ran"
    );
}

// ---- commit text cannot forge history rows -------------------------------------------------

#[test]
fn separator_bytes_in_a_commit_message_do_not_forge_rows() {
    let f = fixture();
    let forged = "real subject\n\nbody\u{1e}0123456789abcdef0123456789abcdef01234567\u{1f}0123456\u{1f}\
        Trusted Maintainer\u{1f}m@x\u{1f}Mon, 1 Jan, 00:00\u{1f}\u{1f}HEAD -> refs/heads/release\u{1f}\
        Signed\u{1f}\u{1d}\nM\tfake.txt";
    f.git(&["commit", "--allow-empty", "-m", forged]);

    let view = inspect_worktree(&f.config, &[], &f.wt()).expect("inspect");

    assert_eq!(
        view.commits.len(),
        2,
        "one real commit plus the seed commit"
    );
    assert!(
        view.commits
            .iter()
            .all(|c| c.author != "Trusted Maintainer"),
        "a forged commit row appeared: {:?}",
        view.commits
    );
}

#[test]
fn a_nul_byte_in_a_stored_commit_does_not_forge_rows() {
    let f = fixture();
    f.commit_raw("", b"subject\n\nbody\0pwned\0\0\0\0\0\0\0\0\0\0\n");

    let view = inspect_worktree(&f.config, &[], &f.wt()).expect("inspect");

    assert_eq!(view.commits.len(), 2);
    assert!(view.commits.iter().all(|c| c.full.len() == 40));
}

// ---- background fetch must not run what the repository brings ------------------------------

fn bare(f: &Fixture) -> PathBuf {
    f.root.join(".bare")
}

fn set_bare_config(f: &Fixture, key: &str, value: &str) {
    let dir = format!("--git-dir={}", bare(f).display());
    git(
        &f.config,
        &[
            OsStr::new(&dir),
            OsStr::new("config"),
            OsStr::new(key),
            OsStr::new(value),
        ],
    );
}

#[test]
fn background_fetch_works_for_a_plain_repository() {
    let f = fixture();
    fetch_repo(&f.config, &f.root).expect("fetch of a repository without code-bearing config");
}

#[test]
fn background_fetch_refuses_a_repo_defined_ssh_command() {
    let f = fixture();
    let script = f.marker_script("ssh");
    set_bare_config(&f, "core.sshCommand", script.to_str().expect("utf8"));

    assert!(fetch_repo(&f.config, &f.root).is_err());
    assert!(!f.marker("ssh").exists());
}

#[test]
fn background_fetch_refuses_a_repo_defined_git_proxy() {
    let f = fixture();
    let script = f.marker_script("proxy");
    set_bare_config(&f, "core.gitProxy", script.to_str().expect("utf8"));

    assert!(fetch_repo(&f.config, &f.root).is_err());
    assert!(!f.marker("proxy").exists());
}

#[test]
fn background_fetch_refuses_an_included_code_bearing_config() {
    let f = fixture();
    let script = f.marker_script("included");
    let extra = f.tmp.path.join("extra.cfg");
    fs::write(
        &extra,
        format!("[core]\n\tsshCommand = {}\n", script.display()),
    )
    .expect("write");
    set_bare_config(&f, "include.path", extra.to_str().expect("utf8"));

    assert!(fetch_repo(&f.config, &f.root).is_err());
    assert!(!f.marker("included").exists());
}

#[test]
fn background_fetch_refuses_a_reference_transaction_hook() {
    let f = fixture();
    let hooks: &Path = &bare(&f).join("hooks");
    fs::create_dir_all(hooks).expect("hooks dir");
    let hook = f.marker_script("hook");
    fs::copy(&hook, hooks.join("reference-transaction")).expect("install hook");

    assert!(fetch_repo(&f.config, &f.root).is_err());
    assert!(!f.marker("hook").exists());
}
