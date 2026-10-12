//! Notices changes made outside the window (terminal, glab, scripts) without file events:
//! a fingerprint of the modification times git touches when worktrees, branches, the index or
//! HEAD change. The window compares fingerprints every few seconds and reloads what changed.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::model::Repo;

/// Upper bound of entries looked at under `refs/`, so a huge ref tree stays cheap.
const REF_ENTRY_LIMIT: usize = 2000;
const REF_DEPTH: usize = 6;

fn stamp(path: &Path, hasher: &mut DefaultHasher) {
    path.hash(hasher);
    if let Ok(meta) = fs::metadata(path) {
        meta.len().hash(hasher);
        if let Ok(modified) = meta.modified() {
            modified.hash(hasher);
        }
    }
}

/// Directories and files below `dir` (depth- and count-limited), stamped in a stable order.
fn stamp_tree(dir: &Path, depth: usize, budget: &mut usize, hasher: &mut DefaultHasher) {
    stamp(dir, hasher);
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        if path.is_dir() {
            stamp_tree(&path, depth - 1, budget, hasher);
        } else {
            stamp(&path, hasher);
        }
    }
}

/// Changes when a worktree is added, moved or removed, a branch or tag moves, something is
/// staged or committed, or HEAD changes in any worktree of `repo`.
pub fn repo_fingerprint(repo: &Repo) -> u64 {
    let mut hasher = DefaultHasher::new();
    let git = &repo.git_dir;
    // FETCH_HEAD is left out: our own auto-fetch rewrites it, and the refs it moves are seen.
    for name in ["HEAD", "packed-refs", "index"] {
        stamp(&git.join(name), &mut hasher);
    }
    let mut budget = REF_ENTRY_LIMIT;
    stamp_tree(&git.join("refs"), REF_DEPTH, &mut budget, &mut hasher);
    // Per-worktree HEAD and index live in `<git dir>/worktrees/<name>/`; their own budget, so
    // many loose refs never hide them.
    let mut budget = REF_ENTRY_LIMIT;
    stamp_tree(&git.join("worktrees"), 2, &mut budget, &mut hasher);
    for wt in &repo.worktrees {
        wt.path.is_dir().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Layout;
    use std::path::PathBuf;

    fn repo_at(git_dir: PathBuf) -> Repo {
        Repo {
            root: git_dir.clone(),
            name: "r".to_string(),
            layout: Layout::Bare,
            git_dir,
            worktrees: Vec::new(),
            code_sources: Vec::new(),
            trust_key: String::new(),
        }
    }

    #[test]
    fn fingerprint_follows_refs_and_worktree_dirs() {
        let tmp = tempfile::tempdir().expect("tmp");
        let git = tmp.path().join(".bare");
        fs::create_dir_all(git.join("refs/heads")).expect("mkdir");
        fs::write(git.join("HEAD"), "ref: refs/heads/master\n").expect("write");
        let repo = repo_at(git.clone());
        let first = repo_fingerprint(&repo);
        assert_eq!(first, repo_fingerprint(&repo), "stable without changes");
        fs::write(git.join("refs/heads/feature"), "0123\n").expect("write");
        let second = repo_fingerprint(&repo);
        assert_ne!(first, second, "a new branch is noticed");
        fs::create_dir_all(git.join("worktrees/feature")).expect("mkdir");
        fs::write(git.join("worktrees/feature/HEAD"), "x\n").expect("write");
        assert_ne!(second, repo_fingerprint(&repo), "a new worktree is noticed");
    }
}
