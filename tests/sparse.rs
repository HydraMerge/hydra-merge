#![cfg(unix)]
//! Sparse worktrees: a worktree with only some top-level folders of its branch.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use common::{git, make_bare_layout, make_plain_clone, test_config, tmp, Tmp};
use hydra_merge::git::{
    create_sparse_worktree, inspect_worktree, show_all_files, sparse_folders, top_folders,
};
use hydra_merge::model::{Config, ErrorCode};

struct Fixture {
    _tmp: Tmp,
    config: Config,
    root: PathBuf,
}

fn g(config: &Config, args: &[&str]) -> String {
    let os: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git(config, &os)
}

/// Branch `mono` holds `app/`, `docs/`, `lib/` and the root `README`; `master` has a worktree.
fn fixture() -> Fixture {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    let root = make_bare_layout(&tmp.path, &config, "grp/repo", &["master"], &["master"]);
    let master = root.join("master");
    for (dir, file) in [("app", "a.txt"), ("docs", "c.txt"), ("lib", "b.txt")] {
        fs::create_dir_all(master.join(dir)).expect("mkdir");
        fs::write(master.join(dir).join(file), "x\n").expect("write");
    }
    let wt = master.to_str().expect("utf8");
    g(&config, &["-C", wt, "add", "--all"]);
    g(&config, &["-C", wt, "commit", "-m", "folders"]);
    g(&config, &["-C", wt, "branch", "mono"]);
    Fixture {
        _tmp: tmp,
        config,
        root,
    }
}

fn worktree_count(f: &Fixture) -> usize {
    let bare = format!("--git-dir={}", f.root.join(".bare").display());
    g(&f.config, &[&bare, "worktree", "list", "--porcelain"])
        .lines()
        .filter(|l| l.starts_with("worktree "))
        .count()
}

#[test]
fn top_folders_lists_directories_of_the_branch() {
    let f = fixture();
    let folders = top_folders(&f.config, &f.root, "mono").expect("folders");
    assert_eq!(folders, vec!["app", "docs", "lib"]);
}

#[test]
fn sparse_worktree_holds_only_the_named_folders() {
    let f = fixture();
    let wt = create_sparse_worktree(&f.config, &f.root, "mono", &["app".to_string()])
        .expect("create sparse");
    assert_eq!(wt.path, f.root.join("mono"));
    assert!(wt.path.join("app/a.txt").is_file());
    assert!(
        wt.path.join("README").is_file(),
        "cone mode keeps root files"
    );
    assert!(!wt.path.join("lib").exists());
    assert!(!wt.path.join("docs").exists());
    let view = inspect_worktree(&f.config, &[], &wt.path).expect("inspect");
    assert!(
        view.files.is_empty(),
        "fresh sparse worktree is clean: {:?}",
        view.files
    );
    assert_eq!(
        sparse_folders(&f.config, &[], &wt.path).expect("sparse"),
        Some(vec!["app".to_string()])
    );
}

#[test]
fn full_worktree_is_not_sparse() {
    let f = fixture();
    let master = f.root.join("master");
    assert_eq!(
        sparse_folders(&f.config, &[], &master).expect("sparse"),
        None
    );
}

#[test]
fn show_all_files_restores_the_full_tree() {
    let f = fixture();
    let folders = ["lib".to_string()];
    let wt = create_sparse_worktree(&f.config, &f.root, "mono", &folders).expect("create");
    show_all_files(&f.config, &[], &wt.path).expect("disable");
    assert!(wt.path.join("app/a.txt").is_file());
    assert!(wt.path.join("docs/c.txt").is_file());
    assert_eq!(
        sparse_folders(&f.config, &[], &wt.path).expect("sparse"),
        None
    );
}

#[test]
fn sparse_refuses_bad_folder_lists_before_touching_the_repo() {
    let f = fixture();
    let before = worktree_count(&f);
    for folders in [
        vec![],
        vec!["nope".to_string()],
        vec!["../escape".to_string()],
        vec!["-x".to_string()],
        vec!["app/inner".to_string()],
    ] {
        let err = create_sparse_worktree(&f.config, &f.root, "mono", &folders)
            .expect_err("bad folder list");
        assert_eq!(err.code, ErrorCode::UnknownFolder, "{folders:?}");
    }
    assert_eq!(worktree_count(&f), before);
    assert!(!f.root.join("mono").exists());
}

#[test]
fn sparse_refuses_branch_in_use_and_plain_clones() {
    let f = fixture();
    let err = create_sparse_worktree(&f.config, &f.root, "master", &["app".to_string()])
        .expect_err("in use");
    assert_eq!(err.code, ErrorCode::BranchInUse);
    let tmp = tmp();
    let config = test_config(&tmp.path);
    let clone = make_plain_clone(&tmp.path, &config, "plain");
    let err =
        create_sparse_worktree(&config, &clone, "master", &["app".to_string()]).expect_err("plain");
    assert_eq!(err.code, ErrorCode::PlainLayout);
}
