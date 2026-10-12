//! A repository whose own config can run programs is not read or changed until the user trusts
//! exactly that config. Temporary repositories only.

#![cfg(unix)]

mod common;

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use common::{fake_smerge, git, make_bare_layout, smerge_calls, test_config, tmp, Tmp};
use hydra_merge::discover::{discover_repos, open_path, read_statuses};
use hydra_merge::git::{
    apply_stash, branches, commit_patch, commit_worktree, compare_refs, create_tracking_branch,
    create_tracking_worktree, create_worktree, delete_remote_branch, discard_file, discard_paths,
    drop_stash, fetch_repo, fetch_worktree, inspect_worktree, open_in_merge, pull_worktree,
    push_worktree, remove_worktree, search_log, stage_file, stage_paths, unstage_file,
    unstage_paths,
};
use hydra_merge::model::{AppError, Config, ErrorCode, Repo};

struct Fixture {
    tmp: Tmp,
    config: Config,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    fake_smerge(&tmp.path);
    let root = make_bare_layout(&tmp.path, &config, "grp/repo", &["master"], &["master"]);
    Fixture { tmp, config, root }
}

impl Fixture {
    fn wt(&self) -> PathBuf {
        self.root.join("master")
    }

    fn set_config(&self, key: &str, value: &str) {
        let dir = format!("--git-dir={}", self.root.join(".bare").display());
        git(
            &self.config,
            &[
                OsStr::new(&dir),
                OsStr::new("config"),
                OsStr::new(key),
                OsStr::new(value),
            ],
        );
    }

    fn marker(&self) -> PathBuf {
        self.tmp.path.join("clean.marker")
    }

    /// A clean filter that records every run, selected for `*.dat` by `.gitattributes`.
    fn add_clean_filter(&self) {
        let script = self.tmp.path.join("clean.sh");
        let body = format!(
            "#!/bin/sh\necho ran >> '{}'\ncat\n",
            self.marker().display()
        );
        fs::write(&script, body).expect("write script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        fs::write(self.wt().join(".gitattributes"), "*.dat filter=f\n").expect("attributes");
        fs::write(self.wt().join("a.dat"), "one\n").expect("write a.dat");
        self.set_config("filter.f.clean", script.to_str().expect("utf8"));
    }

    fn repo(&self) -> Repo {
        discover_repos(&self.config)
            .expect("discover")
            .into_iter()
            .next()
            .expect("one repo")
    }

    fn trust(&self, repo: &Repo) {
        self.config.trusted.trust(&repo.root, &repo.trust_key);
    }
}

fn code(result: Result<impl std::fmt::Debug, hydra_merge::model::AppError>) -> ErrorCode {
    result.expect_err("expected an error").code
}

#[test]
fn a_plain_repository_needs_no_trust() {
    let f = fixture();
    let repo = f.repo();
    assert!(repo.code_sources.is_empty(), "{:?}", repo.code_sources);
    inspect_worktree(&f.config, &[], &f.wt()).expect("inspect");
}

#[test]
fn config_that_can_run_programs_is_listed_by_name() {
    let f = fixture();
    f.add_clean_filter();
    f.set_config("core.sshCommand", "ssh -i key");
    f.set_config("credential.helper", "store");

    let repo = f.repo();

    for expected in ["filter.f.clean", "core.sshcommand", "credential.helper"] {
        assert!(
            repo.code_sources.iter().any(|name| name == expected),
            "{expected} missing from {:?}",
            repo.code_sources
        );
    }
}

#[test]
fn an_untrusted_repository_is_not_read_or_changed() {
    let f = fixture();
    f.add_clean_filter();
    let repo = f.repo();
    fs::write(f.wt().join("a.dat"), "two\n").expect("edit");

    assert_eq!(
        code(inspect_worktree(&f.config, &[], &f.wt())),
        ErrorCode::UntrustedRepo
    );
    assert_eq!(code(branches(&f.config, &repo)), ErrorCode::UntrustedRepo);
    assert_eq!(
        code(commit_patch(&f.config, &[], &f.wt(), "0123456")),
        ErrorCode::UntrustedRepo
    );
    assert_eq!(
        code(stage_file(&f.config, &[], &f.wt(), Some("a.dat"))),
        ErrorCode::UntrustedRepo
    );
    assert_eq!(
        code(fetch_repo(&f.config, &f.root)),
        ErrorCode::UntrustedRepo
    );
    assert!(
        !f.marker().exists(),
        "the clean filter ran in an untrusted repository"
    );
}

fn refused<T: std::fmt::Debug>(result: Result<T, AppError>) -> bool {
    matches!(result, Err(e) if e.code == ErrorCode::UntrustedRepo)
}

/// Every operation that starts git in a repository answers the trust question first.
#[test]
fn every_git_operation_refuses_an_untrusted_repository() {
    let f = fixture();
    f.add_clean_filter();
    let (c, root, wt) = (&f.config, &f.root, f.wt());
    let rels = ["a.dat"];
    let results = [
        (
            "compare_refs",
            refused(compare_refs(c, root, "master", "master")),
        ),
        ("search_log", refused(search_log(c, &[], &wt, "x"))),
        (
            "create_worktree",
            refused(create_worktree(c, root, "topic")),
        ),
        (
            "create_tracking_worktree",
            refused(create_tracking_worktree(c, root, "origin", "master")),
        ),
        (
            "create_tracking_branch",
            refused(create_tracking_branch(c, root, "origin", "master")),
        ),
        ("remove_worktree", refused(remove_worktree(c, &wt))),
        ("push_worktree", refused(push_worktree(c, &[], &wt))),
        ("pull_worktree", refused(pull_worktree(c, &[], &wt))),
        ("fetch_worktree", refused(fetch_worktree(c, &[], &wt))),
        (
            "delete_remote_branch",
            refused(delete_remote_branch(c, root, "origin", "master")),
        ),
        (
            "apply_stash",
            refused(apply_stash(c, &[], &wt, "stash@{0}")),
        ),
        ("drop_stash", refused(drop_stash(c, &[], &wt, "stash@{0}"))),
        (
            "commit_worktree",
            refused(commit_worktree(c, &[], &wt, "message")),
        ),
        ("stage_paths", refused(stage_paths(c, &[], &wt, &rels))),
        ("unstage_paths", refused(unstage_paths(c, &[], &wt, &rels))),
        ("unstage_file", refused(unstage_file(c, &[], &wt, "a.dat"))),
        ("discard_paths", refused(discard_paths(c, &[], &wt, &rels))),
        ("discard_file", refused(discard_file(c, &[], &wt, "a.dat"))),
    ];
    for (name, refused) in results {
        assert!(refused, "{name} did not refuse an untrusted repository");
    }
    assert!(!f.marker().exists(), "the clean filter ran");
}

#[test]
fn the_error_names_what_can_run() {
    let f = fixture();
    f.add_clean_filter();
    let err = inspect_worktree(&f.config, &[], &f.wt()).expect_err("blocked");
    assert!(err.detail.unwrap_or_default().contains("filter.f.clean"));
}

#[test]
fn status_is_not_read_in_an_untrusted_repository() {
    let f = fixture();
    f.add_clean_filter();
    fs::write(f.wt().join("a.dat"), "two\n").expect("edit");

    let repo = f.repo();
    let statuses = read_statuses(&f.config, std::slice::from_ref(&repo));

    assert!(
        repo.worktrees.iter().all(|wt| wt.dirty.is_none()),
        "discovery read a status: {:?}",
        repo.worktrees
    );
    assert!(statuses.is_empty(), "{statuses:?}");
    assert!(
        !f.marker().exists(),
        "the clean filter ran during discovery"
    );
}

#[test]
fn opening_a_path_does_not_read_status_of_an_untrusted_repository() {
    let f = fixture();
    f.add_clean_filter();
    fs::write(f.wt().join("a.dat"), "two\n").expect("edit");

    let repo = open_path(&f.config, &f.root).expect("open the path");

    assert!(repo.worktrees.iter().all(|wt| wt.dirty.is_none()));
    assert!(
        !f.marker().exists(),
        "the clean filter ran while opening the path"
    );
    f.trust(&repo);
    let again = open_path(&f.config, &f.root).expect("open after trust");
    assert!(again.worktrees.iter().any(|wt| wt.dirty.is_some()));
}

#[test]
fn trusting_a_repository_unblocks_it() {
    let f = fixture();
    f.add_clean_filter();
    f.trust(&f.repo());
    fs::write(f.wt().join("a.dat"), "two\n").expect("edit");

    inspect_worktree(&f.config, &[], &f.wt()).expect("inspect after trust");
    let repo = f.repo();
    assert!(repo.worktrees.iter().any(|wt| wt.dirty.is_some()));
}

#[test]
fn changing_the_config_after_trust_asks_again() {
    let f = fixture();
    f.add_clean_filter();
    f.trust(&f.repo());
    inspect_worktree(&f.config, &[], &f.wt()).expect("trusted");

    f.set_config("filter.f.clean", "/bin/sh -c 'echo changed'");

    assert_eq!(
        code(inspect_worktree(&f.config, &[], &f.wt())),
        ErrorCode::UntrustedRepo
    );
}

#[test]
fn trust_belongs_to_one_repository() {
    let f = fixture();
    f.add_clean_filter();
    let repo = f.repo();
    f.config
        .trusted
        .trust(&f.tmp.path.join("somewhere/else"), &repo.trust_key);

    assert_eq!(
        code(inspect_worktree(&f.config, &[], &f.wt())),
        ErrorCode::UntrustedRepo
    );
}

#[test]
fn opening_an_editor_does_not_need_trust() {
    let f = fixture();
    f.add_clean_filter();

    open_in_merge(&f.config, &f.wt()).expect("an editor launch runs no git config");

    assert_eq!(
        smerge_calls(&f.tmp.path).expect("called"),
        vec![f.wt().display().to_string()]
    );
}
