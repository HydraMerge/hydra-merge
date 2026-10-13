#![cfg(unix)]

mod common;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use common::{
    fake_smerge, git, make_bare_layout, make_plain_clone, smerge_calls, test_config, tmp, Tmp,
};
use hydra_merge::app::openable;
use hydra_merge::discover::{discover_repos, layout_path_for, open_path};
use hydra_merge::git::{
    branches, create_worktree, inspect_worktree, open_in_merge, remove_worktree,
    validate_branch_name,
};
use hydra_merge::model::{Config, ErrorCode, Layout, Repo, Worktree};

struct Fixture {
    tmp: Tmp,
    config: Config,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    fake_smerge(&tmp.path);
    let root = make_bare_layout(
        &tmp.path,
        &config,
        "grp/repo",
        &["master", "test", "feature/x"],
        &["master", "test"],
    );
    Fixture { tmp, config, root }
}

#[test]
fn discover_finds_a_normal_clone() {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    let root = make_plain_clone(&tmp.path, &config, "grp/app");
    let repos = discover_repos(&config).expect("discover");
    let repo = repos
        .iter()
        .find(|repo| repo.root == root)
        .expect("the clone is listed");
    assert_eq!(repo.layout, Layout::Plain);
    assert!(repo.worktrees.iter().any(|wt| wt.path == root && !wt.bare));
}

fn only_repo(config: &Config) -> Repo {
    let mut repos = discover_repos(config).expect("discover");
    assert_eq!(repos.len(), 1, "expected one repository");
    repos.remove(0)
}

fn worktree_paths(config: &Config) -> Vec<PathBuf> {
    only_repo(config)
        .worktrees
        .into_iter()
        .map(|w| w.path)
        .collect()
}

/// `git` with plain string arguments.
fn git_str(config: &Config, args: &[&str]) -> String {
    let os: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git(config, &os)
}

fn err_code<T: std::fmt::Debug>(result: Result<T, hydra_merge::model::AppError>) -> ErrorCode {
    result.expect_err("expected an error").code
}

#[test]
fn discover_bare_layout() {
    let f = fixture();
    let repo = only_repo(&f.config);
    assert_eq!(repo.layout, Layout::Bare);
    assert_eq!(repo.name, "grp/repo");
    assert_eq!(repo.root, f.root);
    for branch in ["master", "test"] {
        let wt = repo
            .worktrees
            .iter()
            .find(|w| w.branch.as_deref() == Some(branch))
            .expect("worktree listed");
        assert!(wt.layout_correct, "{branch} layout");
        assert_eq!(
            wt.layout_path.as_deref(),
            Some(f.root.join(branch).as_path())
        );
        assert_eq!(wt.dirty, Some(false));
        let head = wt.head.as_deref().expect("head");
        assert!(head.len() == 7 && head.chars().all(|c| c.is_ascii_hexdigit()));
    }
    let bare: Vec<&Worktree> = repo.worktrees.iter().filter(|w| w.bare).collect();
    assert_eq!(bare.len(), 1);
    assert_eq!(bare[0].path, f.root.join(".bare"));
    assert_eq!(bare[0].dirty, None);
    assert!(!bare[0].layout_correct);
}

#[test]
fn discover_does_not_descend_into_layout() {
    let f = fixture();
    fs::create_dir_all(f.root.join("master/nested/.bare")).expect("mkdir");
    let repos = discover_repos(&f.config).expect("discover");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "grp/repo");
}

#[test]
fn discover_skips_hidden_and_node_modules() {
    let f = fixture();
    let seed = f.tmp.path.join(".seed");
    for hidden in [".hidden/r", "node_modules/r"] {
        let bare = f.tmp.path.join(hidden).join(".bare");
        git(
            &f.config,
            &[
                OsStr::new("clone"),
                OsStr::new("--bare"),
                seed.as_os_str(),
                bare.as_os_str(),
            ],
        );
    }
    make_plain_clone(&f.tmp.path, &f.config, "plain");
    let names: Vec<String> = discover_repos(&f.config)
        .expect("discover")
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, vec!["grp/repo".to_string(), "plain".to_string()]);
}

#[test]
fn inspect_shows_status_and_history() {
    let f = fixture();
    let wt = f.root.join("master");
    fs::write(wt.join("note.txt"), "local\n").expect("write note");
    let view = inspect_worktree(&f.config, &[], &wt).expect("inspect");
    assert!(
        view.files.iter().any(|file| file.path == "note.txt"),
        "untracked file missing: {:?}",
        view.files
    );
    let init = view
        .commits
        .iter()
        .find(|commit| commit.subject == "init")
        .expect("history missing");
    assert!(
        init.files.iter().any(|file| file.path == "README"),
        "commit files missing: {:?}",
        init.files
    );
    let patch = hydra_merge::git::commit_patch(&f.config, &[], &wt, &init.hash).expect("patch");
    assert!(patch.contains("README"), "patch missing README: {patch}");
}

#[test]
fn commit_patch_parses_into_highlighted_rows() {
    use hydra_merge::diff::{parse_diff, RowKind};
    use hydra_merge::highlight::TokenKind;

    let f = fixture();
    let wt = f.root.join("master");
    fs::create_dir_all(wt.join("пакет")).expect("mkdir");
    fs::write(wt.join("пакет/main.rs"), "fn main() {\n    // привет\n}\n").expect("write");
    hydra_merge::git::stage_file(&f.config, &[], &wt, None).expect("stage all");
    hydra_merge::git::commit_worktree(&f.config, &[], &wt, "add main").expect("commit");
    let view = inspect_worktree(&f.config, &[], &wt).expect("inspect");
    let commit = view
        .commits
        .iter()
        .find(|commit| commit.subject == "add main")
        .expect("commit listed");
    let patch = hydra_merge::git::commit_patch(&f.config, &[], &wt, &commit.hash).expect("patch");
    let diff = parse_diff(&patch);
    assert_eq!(diff.rows[0].kind, RowKind::File);
    assert_eq!(diff.rows[0].text, "пакет/main.rs  (new file)");
    let added: Vec<_> = diff
        .rows
        .iter()
        .filter(|row| row.kind == RowKind::Added)
        .collect();
    assert_eq!(added.len(), 3);
    assert_eq!(added[1].new_no, Some(2));
    assert!(added[0].spans.iter().any(|s| s.kind == TokenKind::Keyword));
    assert!(added[1].spans.iter().any(|s| s.kind == TokenKind::Comment));
}

#[test]
fn stage_commit_refuses_empty_message_and_parent_path() {
    let f = fixture();
    let wt = f.root.join("master");
    fs::write(wt.join("note.txt"), "local\n").expect("write note");
    let empty = hydra_merge::git::commit_worktree(&f.config, &[], &wt, "  ");
    assert_eq!(err_code(empty), ErrorCode::GitFailed);
    let escaped = hydra_merge::git::stage_file(&f.config, &[], &wt, Some("../note.txt"));
    assert_eq!(err_code(escaped), ErrorCode::OutsideRoots);
    let untracked = hydra_merge::git::discard_file(&f.config, &[], &wt, "note.txt");
    assert_eq!(err_code(untracked), ErrorCode::GitFailed);
    assert!(wt.join("note.txt").is_file(), "untracked file must stay");
    hydra_merge::git::stage_file(&f.config, &[], &wt, Some("note.txt")).expect("stage");
    hydra_merge::git::unstage_file(&f.config, &[], &wt, "note.txt").expect("unstage");
    let mid = inspect_worktree(&f.config, &[], &wt).expect("inspect");
    let note = mid
        .files
        .iter()
        .find(|file| file.path == "note.txt")
        .expect("note still present");
    assert_eq!(note.code, "??");
    hydra_merge::git::stage_file(&f.config, &[], &wt, None).expect("stage all");
    hydra_merge::git::commit_worktree(&f.config, &[], &wt, "add note").expect("commit");
    let view = inspect_worktree(&f.config, &[], &wt).expect("inspect after commit");
    assert!(
        view.files.iter().all(|file| file.path != "note.txt"),
        "note still dirty: {:?}",
        view.files
    );
    assert!(view
        .commits
        .iter()
        .any(|commit| commit.subject == "add note"));
}

#[test]
fn discard_restores_tracked_file() {
    let f = fixture();
    let wt = f.root.join("master");
    fs::write(wt.join("README"), "changed\n").expect("rewrite readme");
    hydra_merge::git::discard_file(&f.config, &[], &wt, "README").expect("discard");
    let text = fs::read_to_string(wt.join("README")).expect("read");
    assert_eq!(text, "seed\n");
    let view = inspect_worktree(&f.config, &[], &wt).expect("inspect");
    assert!(view.files.iter().all(|file| file.path != "README"));
}

#[test]
fn fetch_uses_local_origin() {
    let f = fixture();
    let wt = f.root.join("master");
    hydra_merge::git::fetch_worktree(&f.config, &[], &wt).expect("fetch");
}

#[test]
fn bare_entry_not_openable() {
    let f = fixture();
    let err = open_in_merge(&f.config, &f.root.join(".bare"));
    assert_eq!(err_code(err), ErrorCode::BareNotOpenable);
    assert!(smerge_calls(&f.tmp.path).is_none(), "smerge must not run");
    let repo = only_repo(&f.config);
    for wt in &repo.worktrees {
        assert_eq!(openable(wt), !wt.bare);
    }
    assert!(repo.worktrees.iter().any(|w| w.bare && !openable(w)));
}

#[test]
fn parent_not_openable() {
    let f = fixture();
    assert_eq!(
        err_code(open_in_merge(&f.config, &f.root)),
        ErrorCode::ParentNotOpenable
    );
    assert!(smerge_calls(&f.tmp.path).is_none());
}

#[test]
fn open_passes_only_worktree_path() {
    let f = fixture();
    let opened = open_in_merge(&f.config, &f.root.join("test")).expect("open");
    assert_eq!(opened, f.root.join("test"));
    let calls = smerge_calls(&f.tmp.path).expect("smerge called");
    assert_eq!(calls, vec![f.root.join("test").display().to_string()]);
}

#[test]
fn open_reports_missing_merge_cli() {
    let f = fixture();
    let config = Config {
        smerge: f.tmp.path.join("absent"),
        ..f.config.clone()
    };
    assert_eq!(
        err_code(open_in_merge(&config, &f.root.join("test"))),
        ErrorCode::MergeNotFound
    );
}

#[test]
fn open_unknown_worktree() {
    let f = fixture();
    let stray = f.tmp.path.join("grp/stray");
    fs::create_dir_all(&stray).expect("mkdir");
    assert_eq!(
        err_code(open_in_merge(&f.config, &stray)),
        ErrorCode::UnknownWorktree
    );
    assert!(smerge_calls(&f.tmp.path).is_none());
}

#[test]
fn create_refuses_branch_in_use() {
    let f = fixture();
    let before = worktree_paths(&f.config);
    let err = create_worktree(&f.config, &f.root, "test").expect_err("in use");
    assert_eq!(err.code, ErrorCode::BranchInUse);
    assert!(err
        .message
        .contains(&f.root.join("test").display().to_string()));
    assert_eq!(worktree_paths(&f.config), before);
}

#[test]
fn create_worktree_for_slash_branch() {
    let f = fixture();
    let wt = create_worktree(&f.config, &f.root, "feature/x").expect("create");
    assert!(f.root.join("feature/x").is_dir());
    assert_eq!(wt.branch.as_deref(), Some("feature/x"));
    assert_eq!(wt.path, f.root.join("feature/x"));
    assert!(wt.layout_correct);
}

#[test]
fn create_refuses_existing_dest() {
    let f = fixture();
    fs::create_dir_all(f.root.join("feature/x")).expect("mkdir");
    let before = worktree_paths(&f.config);
    assert_eq!(
        err_code(create_worktree(&f.config, &f.root, "feature/x")),
        ErrorCode::DestExists
    );
    assert_eq!(worktree_paths(&f.config), before);
}

#[test]
fn create_refuses_nested_dest() {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    let root = make_bare_layout(
        &tmp.path,
        &config,
        "grp/repo",
        &["master", "other", "feature/y"],
        &["master"],
    );
    // git forbids branches `feature` and `feature/y` together, so the worktree at
    // <root>/feature carries another branch.
    let bare = format!("--git-dir={}", root.join(".bare").display());
    let dest = root.join("feature");
    git(
        &config,
        &[
            OsStr::new(&bare),
            OsStr::new("worktree"),
            OsStr::new("add"),
            dest.as_os_str(),
            OsStr::new("other"),
        ],
    );
    assert_eq!(
        err_code(create_worktree(&config, &root, "feature/y")),
        ErrorCode::DestNested
    );
}

#[test]
fn create_unknown_branch() {
    let f = fixture();
    assert_eq!(
        err_code(create_worktree(&f.config, &f.root, "nope")),
        ErrorCode::BranchNotFound
    );
}

#[test]
fn invalid_branch_names() {
    let f = fixture();
    for name in [
        "..", "../x", "a/../b", "/abs", ".bare", "x/.bare", "-f", "a//b", "a/", ".hidden", "",
    ] {
        assert_eq!(
            err_code(validate_branch_name(&f.config, name)),
            ErrorCode::InvalidBranch,
            "{name:?}"
        );
        assert_eq!(
            err_code(layout_path_for(&f.root, name)),
            ErrorCode::InvalidBranch,
            "{name:?}"
        );
        assert_eq!(
            err_code(create_worktree(&f.config, &f.root, name)),
            ErrorCode::InvalidBranch
        );
    }
    assert!(validate_branch_name(&f.config, "feature/ok").is_ok());
    assert_eq!(
        err_code(validate_branch_name(&f.config, "a b")),
        ErrorCode::InvalidBranch
    );
}

#[test]
fn remove_refuses_dirty() {
    let f = fixture();
    let file = f.root.join("test/new.txt");
    fs::write(&file, "x").expect("write");
    assert_eq!(
        err_code(remove_worktree(&f.config, &f.root.join("test"))),
        ErrorCode::DirtyWorktree
    );
    assert!(f.root.join("test").is_dir() && file.is_file());
}

#[test]
fn remove_clean_keeps_bare() {
    let f = fixture();
    let removed = remove_worktree(&f.config, &f.root.join("test")).expect("remove");
    assert_eq!(removed, f.root.join("test"));
    assert!(!f.root.join("test").exists());
    assert!(f.root.join(".bare").is_dir());
    let bare = OsString::from(format!("--git-dir={}", f.root.join(".bare").display()));
    git(
        &f.config,
        &[
            bare.as_os_str(),
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new("refs/heads/test"),
        ],
    );
}

#[test]
fn remove_bare_refused() {
    let f = fixture();
    assert_eq!(
        err_code(remove_worktree(&f.config, &f.root.join(".bare"))),
        ErrorCode::BareNotOpenable
    );
    assert!(f.root.join(".bare").is_dir());
}

#[test]
fn remove_parent_refused() {
    let f = fixture();
    assert_eq!(
        err_code(remove_worktree(&f.config, &f.root)),
        ErrorCode::ParentNotOpenable
    );
}

#[test]
fn path_outside_roots_refused() {
    let f = fixture();
    assert_eq!(
        err_code(open_in_merge(&f.config, Path::new("/etc"))),
        ErrorCode::OutsideRoots
    );
    let other = tmp();
    assert_eq!(
        err_code(open_in_merge(&f.config, &other.path)),
        ErrorCode::OutsideRoots
    );
    assert_eq!(
        err_code(remove_worktree(&f.config, &other.path)),
        ErrorCode::OutsideRoots
    );
    assert_eq!(
        err_code(create_worktree(&f.config, &other.path, "x")),
        ErrorCode::OutsideRoots
    );
}

#[test]
fn plain_clone_marked_plain() {
    let f = fixture();
    let plain = make_plain_clone(&f.tmp.path, &f.config, "plain");
    let repo = open_path(&f.config, &plain).expect("open path");
    assert_eq!(repo.layout, Layout::Plain);
    assert_eq!(repo.root, plain);
    assert_eq!(repo.worktrees.len(), 1);
    assert!(repo.worktrees[0].layout_path.is_none() && !repo.worktrees[0].layout_correct);
    assert_eq!(
        err_code(create_worktree(&f.config, &repo.root, "master")),
        ErrorCode::PlainLayout
    );
    assert_eq!(
        err_code(remove_worktree(&f.config, &repo.root)),
        ErrorCode::PlainLayout
    );
}

#[test]
fn plain_clone_opens_and_removal_is_refused() {
    let f = fixture();
    let plain = make_plain_clone(&f.tmp.path, &f.config, "plain");
    let extra = vec![plain.clone()];
    let opened =
        hydra_merge::git::open_in_merge_with(&f.config, &extra, &plain).expect("open plain");
    assert_eq!(opened, plain);
    let err = hydra_merge::git::remove_worktree_with(&f.config, &extra, &plain);
    assert_eq!(err_code(err), ErrorCode::PlainLayout);
    assert!(plain.is_dir());
}

#[test]
fn open_path_rejects_non_repo() {
    let f = fixture();
    let dir = f.tmp.path.join("empty");
    fs::create_dir_all(&dir).expect("mkdir");
    assert_eq!(err_code(open_path(&f.config, &dir)), ErrorCode::NotARepo);
    assert_eq!(
        err_code(open_path(&f.config, &f.tmp.path.join(".seed/README"))),
        ErrorCode::NotARepo
    );
}

#[test]
fn open_path_accepts_layout_root() {
    let f = fixture();
    let repo = open_path(&f.config, &f.root).expect("open layout root");
    assert_eq!(repo.layout, Layout::Bare);
    assert_eq!(repo.name, "grp/repo");
}

#[test]
fn branches_split_available_and_used() {
    let f = fixture();
    let list = branches(&f.config, &only_repo(&f.config)).expect("branches");
    assert_eq!(list.available, vec!["feature/x".to_string()]);
    assert_eq!(
        list.used,
        vec![
            ("master".to_string(), f.root.join("master")),
            ("test".to_string(), f.root.join("test")),
        ]
    );
    assert!(list.local.iter().any(|name| name == "master"));
    assert!(list.tags.is_empty());
    assert!(list.stashes.is_empty());
}

#[test]
fn locations_include_remote_tag_and_stash() {
    let f = fixture();
    let bare = f.root.join(".bare");
    let wt = f.root.join("master");
    let bare_arg = format!("--git-dir={}", bare.display());
    git(
        &f.config,
        &[
            OsStr::new(&bare_arg),
            OsStr::new("tag"),
            OsStr::new("v1"),
            OsStr::new("master"),
        ],
    );
    git(
        &f.config,
        &[
            OsStr::new(&bare_arg),
            OsStr::new("update-ref"),
            OsStr::new("refs/remotes/origin/topic"),
            OsStr::new("master"),
        ],
    );
    fs::write(wt.join("README"), "stashed\n").expect("edit");
    git(
        &f.config,
        &[
            OsStr::new("-C"),
            wt.as_os_str(),
            OsStr::new("stash"),
            OsStr::new("push"),
            OsStr::new("-m"),
            OsStr::new("wip note"),
        ],
    );
    let list = branches(&f.config, &only_repo(&f.config)).expect("branches");
    assert!(list
        .tags
        .iter()
        .any(|tag| tag.name == "v1" && !tag.commit.is_empty()));
    assert!(list
        .remotes
        .iter()
        .any(|remote| remote.name == "origin" && remote.branches.iter().any(|b| b == "topic")));
    assert!(
        list.stashes
            .iter()
            .any(|stash| stash.subject.contains("wip note")),
        "stashes: {:?}",
        list.stashes
    );
    let id = list.stashes[0].id.clone();
    hydra_merge::git::apply_stash(&f.config, &[], &wt, &id).expect("apply");
    let text = fs::read_to_string(wt.join("README")).expect("read");
    assert_eq!(text, "stashed\n");
    hydra_merge::git::drop_stash(&f.config, &[], &wt, &id).expect("drop");
    let after = branches(&f.config, &only_repo(&f.config)).expect("branches");
    assert!(after.stashes.iter().all(|stash| stash.id != id));
    assert_eq!(
        err_code(hydra_merge::git::drop_stash(
            &f.config,
            &[],
            &wt,
            "stash@{0};rm"
        )),
        ErrorCode::GitFailed
    );
    let created =
        hydra_merge::git::create_tracking_worktree(&f.config, &f.root, "origin", "topic")
            .expect("tracking worktree");
    assert_eq!(created.path, f.root.join("topic"));
}

#[test]
fn staged_rename_is_listed_by_new_path_and_unstaged_whole() {
    let f = fixture();
    let wt = f.root.join("master");
    let wt_arg = wt.to_str().expect("utf8");
    git_str(&f.config, &["-C", wt_arg, "mv", "README", "README.md"]);
    let view = inspect_worktree(&f.config, &[], &wt).expect("inspect");
    let renamed = view
        .files
        .iter()
        .find(|file| file.code.starts_with('R'))
        .expect("rename listed");
    assert_eq!(renamed.path, "README.md");
    assert_eq!(renamed.orig_path.as_deref(), Some("README"));
    hydra_merge::git::unstage_paths(&f.config, &[], &wt, &["README.md", "README"])
        .expect("unstage rename");
    let after = inspect_worktree(&f.config, &[], &wt).expect("inspect");
    assert!(
        after
            .files
            .iter()
            .all(|file| !file.code.starts_with(['R', 'D', 'A'])),
        "nothing staged: {:?}",
        after.files
    );
}

#[test]
fn plain_clone_under_scan_roots_opens() {
    let f = fixture();
    let plain = make_plain_clone(&f.tmp.path, &f.config, "plain");
    let plain = fs::canonicalize(plain).expect("canonical");
    let opened = hydra_merge::git::open_in_merge_with(&f.config, &[], &plain);
    assert_eq!(opened.expect("scan finds the clone"), plain);
}

#[test]
fn delete_remote_branch_removes_it_on_the_remote() {
    let f = fixture();
    let seed = f.tmp.path.join(".seed");
    let seed_arg = seed.to_str().expect("utf8");
    let bare_arg = format!("--git-dir={}", f.root.join(".bare").display());
    git_str(&f.config, &["-C", seed_arg, "branch", "topic"]);
    let refspec = "+refs/heads/*:refs/remotes/origin/*";
    git_str(&f.config, &[&bare_arg, "fetch", "origin", refspec]);
    let bad_remote =
        hydra_merge::git::delete_remote_branch(&f.config, &f.root, "nowhere", "topic");
    assert_eq!(err_code(bad_remote), ErrorCode::BranchNotFound);
    let bad_name = hydra_merge::git::delete_remote_branch(&f.config, &f.root, "origin", "-x");
    assert_eq!(err_code(bad_name), ErrorCode::InvalidBranch);
    hydra_merge::git::delete_remote_branch(&f.config, &f.root, "origin", "topic")
        .expect("delete remote branch");
    let on_remote = git_str(&f.config, &["-C", seed_arg, "branch", "--list", "topic"]);
    assert!(on_remote.trim().is_empty(), "topic still on remote");
    let list = branches(&f.config, &only_repo(&f.config)).expect("branches");
    assert!(list
        .remotes
        .iter()
        .all(|remote| remote.branches.iter().all(|b| b != "topic")));
}

#[test]
fn create_local_branch_from_remote() {
    let f = fixture();
    let bare_arg = format!("--git-dir={}", f.root.join(".bare").display());
    git_str(
        &f.config,
        &[
            &bare_arg,
            "update-ref",
            "refs/remotes/origin/mr/162",
            "master",
        ],
    );
    let create = |remote: &str, branch: &str| {
        hydra_merge::git::create_tracking_branch(&f.config, &f.root, remote, branch)
    };
    assert_eq!(
        err_code(create("origin", "nope")),
        ErrorCode::BranchNotFound
    );
    assert_eq!(err_code(create("origin", "../x")), ErrorCode::InvalidBranch);
    create("origin", "mr/162").expect("create local branch");
    let list = branches(&f.config, &only_repo(&f.config)).expect("branches");
    assert!(list.local.iter().any(|name| name == "mr/162"));
    assert!(
        list.available.iter().any(|name| name == "mr/162"),
        "no worktree is made"
    );
    assert_eq!(err_code(create("origin", "mr/162")), ErrorCode::BranchInUse);
}

#[test]
fn fast_scan_leaves_status_for_later() {
    use hydra_merge::discover::{discover_repos_with, fill_status};
    use hydra_merge::git::Status;

    let f = fixture();
    fs::write(f.root.join("test/new.txt"), "x\n").expect("write");
    let fast = discover_repos_with(&f.config, Status::Skip).expect("fast scan");
    assert_eq!(fast.len(), 1);
    let checked_out = |repos: &[Repo]| -> Vec<Option<bool>> {
        repos[0]
            .worktrees
            .iter()
            .filter(|wt| !wt.bare)
            .map(|wt| wt.dirty)
            .collect()
    };
    assert_eq!(checked_out(&fast), vec![None, None]);
    let full = fill_status(&f.config, fast);
    assert_eq!(checked_out(&full), vec![Some(false), Some(true)]);
    assert_eq!(full, discover_repos(&f.config).expect("full scan"));
}

#[test]
fn compare_branches_and_search_history() {
    let f = fixture();
    let wt = f.root.join("test");
    let wt_s = wt.to_str().expect("utf8");
    fs::write(wt.join("app.yml"), "name: demo\n").expect("write");
    git_str(&f.config, &["-C", wt_s, "add", "app.yml"]);
    git_str(
        &f.config,
        &["-C", wt_s, "commit", "-m", "INT3-355 add app config"],
    );
    let diff =
        hydra_merge::git::compare_refs(&f.config, &f.root, "master", "test").expect("compare");
    assert!(diff.contains("+++ b/app.yml"), "{diff}");
    let none = hydra_merge::git::compare_refs(&f.config, &f.root, "test", "master")
        .expect("compare back");
    assert!(none.trim().is_empty(), "master has nothing new: {none}");
    let bad = hydra_merge::git::compare_refs(&f.config, &f.root, "--output=/tmp/x", "test");
    assert_eq!(err_code(bad), ErrorCode::BranchNotFound);
    let missing = hydra_merge::git::compare_refs(&f.config, &f.root, "nope", "test");
    assert_eq!(err_code(missing), ErrorCode::BranchNotFound);
    let found = hydra_merge::git::search_log(&f.config, &[], &wt, "int3-355").expect("search");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].subject, "INT3-355 add app config");
    let odd =
        hydra_merge::git::search_log(&f.config, &[], &wt, "-p --all").expect("text, not options");
    assert!(odd.is_empty());
}

#[test]
fn stage_and_discard_several_files() {
    let f = fixture();
    let wt = f.root.join("master");
    let wt_s = wt.to_str().expect("utf8");
    fs::create_dir_all(wt.join("dir")).expect("mkdir");
    fs::write(wt.join("dir/a.txt"), "a\n").expect("write");
    fs::write(wt.join("dir/b.txt"), "b\n").expect("write");
    fs::write(wt.join("README"), "changed\n").expect("write");
    hydra_merge::git::stage_paths(&f.config, &[], &wt, &["dir/"]).expect("stage folder");
    let status = git_str(&f.config, &["-C", wt_s, "status", "--porcelain"]);
    assert!(
        status.contains("A  dir/a.txt") && status.contains("A  dir/b.txt"),
        "{status}"
    );
    hydra_merge::git::discard_paths(&f.config, &[], &wt, &["README"]).expect("discard");
    assert_eq!(
        fs::read_to_string(wt.join("README")).expect("read"),
        "seed\n"
    );
    let escaped = hydra_merge::git::stage_paths(&f.config, &[], &wt, &["dir/a.txt", "../x"]);
    assert_eq!(err_code(escaped), ErrorCode::OutsideRoots);
}

#[test]
fn config_from_values_defaults() {
    let tmp = tmp();
    let home = tmp.path.join("home");
    fs::create_dir_all(home.join("repos")).expect("mkdir");
    let config = Config::from_values(&home, None, None);
    let real = fs::canonicalize(&home).expect("canonicalize");
    assert_eq!(config.roots, vec![real.join("repos")]);
    assert_eq!(config.smerge, hydra_merge::os::default_smerge());
    assert!(config.git_env.is_empty());
}

#[test]
fn config_from_values_env_roots() {
    let tmp = tmp();
    let home = tmp.path.join("home");
    fs::create_dir_all(home.join("a")).expect("mkdir");
    fs::create_dir_all(tmp.path.join("b")).expect("mkdir");
    let var = format!(
        "~/a::{}:/does/not/exist:{}/../b:~/a",
        tmp.path.join("b").display(),
        home.display()
    );
    let smerge = OsString::from("/x/smerge");
    let config = Config::from_values(&home, Some(OsStr::new(&var)), Some(smerge.as_os_str()));
    let real = fs::canonicalize(&tmp.path).expect("canonicalize");
    assert_eq!(config.roots, vec![real.join("home/a"), real.join("b")]);
    assert_eq!(config.smerge, PathBuf::from("/x/smerge"));
    let empty = Config::from_values(&home, None, Some(OsStr::new("")));
    assert_eq!(empty.smerge, hydra_merge::os::default_smerge());
}

#[test]
fn roots_are_temp_only() {
    let tmp = tmp();
    let config = test_config(&tmp.path);
    let real_repos = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("repos"));
    assert!(!config.roots.is_empty());
    for root in &config.roots {
        assert!(root.starts_with(&tmp.path));
        if let Some(repos) = &real_repos {
            assert!(!root.starts_with(repos));
        }
    }
}

/// String literals of a Rust source, comments and char literals skipped.
fn string_literals(src: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\'' if chars.get(i + 2) == Some(&'\'') => i += 3,
            '\'' if chars.get(i + 1) == Some(&'\\') && chars.get(i + 3) == Some(&'\'') => i += 4,
            '"' => {
                let mut lit = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    lit.extend(chars.get(i));
                    i += 1;
                }
                out.push(lit);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

#[test]
fn source_has_no_forbidden_git() {
    let guarded = [
        ("src/git/mod.rs", include_str!("../src/git/mod.rs")),
        ("src/git/process.rs", include_str!("../src/git/process.rs")),
        (
            "src/git/worktrees.rs",
            include_str!("../src/git/worktrees.rs"),
        ),
        ("src/git/refs.rs", include_str!("../src/git/refs.rs")),
        ("src/git/changes.rs", include_str!("../src/git/changes.rs")),
        ("src/git/editor.rs", include_str!("../src/git/editor.rs")),
        ("src/discover.rs", include_str!("../src/discover.rs")),
        ("src/model.rs", include_str!("../src/model.rs")),
    ];
    let forbidden_literals = [
        "checkout", "switch", "reset", "clean", "merge", "rebase", "prune", "--force", "-f", "-D",
        "sh", "-c",
    ];
    for (name, src) in guarded {
        for lit in string_literals(src) {
            assert!(
                !forbidden_literals.contains(&lit.as_str()),
                "{name} has literal {lit:?}"
            );
        }
    }
    let all = rust_sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    assert!(all.len() >= 10, "src walk found {} files", all.len());
    for (path, src) in all {
        for banned in ["remove_dir_all", "remove_file", "git2"] {
            assert!(
                !src.contains(banned),
                "{} mentions {banned}",
                path.display()
            );
        }
    }
}

/// Every `.rs` file under `dir`, so new modules are checked without editing this test.
fn rust_sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("read src") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path).expect("read source");
            out.push((path, text));
        }
    }
    out
}

#[test]
fn string_literal_scanner_sees_literals_only() {
    let lits = string_literals("let a = \"x\"; // \"no\"\nlet c = '\"'; let b = \"y\\\"z\";");
    assert_eq!(lits, vec!["x".to_string(), "y\"z".to_string()]);
}
