//! Branches, remotes, tags, stashes and submodules of one repository.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteBranches {
    pub name: String,
    pub branches: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagEntry {
    pub name: String,
    pub commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashEntry {
    pub id: String,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmoduleEntry {
    pub path: String,
    pub commit: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchList {
    /// Local branches that are not checked out in a worktree.
    pub available: Vec<String>,
    pub used: Vec<(String, PathBuf)>,
    pub local: Vec<String>,
    pub remotes: Vec<RemoteBranches>,
    pub tags: Vec<TagEntry>,
    pub stashes: Vec<StashEntry>,
    pub submodules: Vec<SubmoduleEntry>,
}

pub(crate) const REF_FORMAT: &str = "%(refname)\t%(objectname)\t%(*objectname)";

pub fn branches(config: &Config, repo: &Repo) -> AppResult<BranchList> {
    crate::trust::ensure_trusted(config, repo)?;
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("for-each-ref"),
        os("--format"),
        os(REF_FORMAT),
        os("refs/heads"),
        os("refs/remotes"),
        os("refs/tags"),
    ];
    let out = run_git_ok(config, &args, &repo.root)?;
    let mut list = parse_ref_lines(&out.stdout);
    for name in &list.local {
        match worktree_of_branch(&repo.worktrees, name) {
            Some(wt) => list.used.push((name.clone(), wt.path.clone())),
            None => list.available.push(name.clone()),
        }
    }
    list.available.sort();
    list.used.sort();
    let known = remote_names(config, repo).unwrap_or_default();
    for name in known {
        if !list.remotes.iter().any(|remote| remote.name == name) {
            list.remotes.push(RemoteBranches {
                name,
                branches: Vec::new(),
            });
        }
    }
    list.remotes.sort_by(|a, b| a.name.cmp(&b.name));
    list.stashes = stash_list(config, repo).unwrap_or_default();
    list.submodules = submodule_list(config, repo).unwrap_or_default();
    Ok(list)
}

/// `(remote, branch)` pairs grouped per remote, in first-seen remote order, branches sorted.
fn group_remotes(pairs: &[(&str, &str)]) -> Vec<RemoteBranches> {
    let mut remotes: Vec<RemoteBranches> = Vec::new();
    for (remote, branch) in pairs {
        match remotes.iter_mut().find(|item| item.name == *remote) {
            Some(item) => item.branches.push(branch.to_string()),
            None => remotes.push(RemoteBranches {
                name: remote.to_string(),
                branches: vec![branch.to_string()],
            }),
        }
    }
    remotes.iter_mut().for_each(|item| item.branches.sort());
    remotes
}

pub fn parse_ref_lines(stdout: &str) -> BranchList {
    let mut local = Vec::new();
    let mut tags = Vec::new();
    let mut remote_refs: Vec<(&str, &str)> = Vec::new();
    for line in stdout.lines().filter(|line| !line.is_empty()) {
        let mut fields = line.split('\t');
        let Some(name) = fields.next() else { continue };
        let object = fields.next().unwrap_or("");
        let peeled = fields.next().unwrap_or("");
        let commit = if peeled.is_empty() { object } else { peeled };
        if let Some(branch) = name.strip_prefix(REFS_HEADS) {
            if !branch.is_empty() {
                local.push(branch.to_string());
            }
            continue;
        }
        if let Some(rest) = name.strip_prefix("refs/tags/") {
            if !rest.is_empty() {
                tags.push(TagEntry {
                    name: rest.to_string(),
                    commit: commit.to_string(),
                });
            }
            continue;
        }
        if let Some(rest) = name.strip_prefix("refs/remotes/") {
            let Some((remote, branch)) = rest.split_once('/') else {
                continue;
            };
            if !remote.is_empty() && !branch.is_empty() && branch != "HEAD" {
                remote_refs.push((remote, branch));
            }
        }
    }
    local.sort();
    tags.sort_by(|a, b| a.name.cmp(&b.name));
    BranchList {
        local,
        remotes: group_remotes(&remote_refs),
        tags,
        ..BranchList::default()
    }
}

pub fn parse_stashes(stdout: &str) -> Vec<StashEntry> {
    stdout
        .lines()
        .filter_map(|line| {
            let (id, subject) = line.split_once('\u{1f}')?;
            if !stash_id_ok(id) {
                return None;
            }
            Some(StashEntry {
                id: id.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect()
}

pub fn parse_submodules(stdout: &str) -> Vec<SubmoduleEntry> {
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.get(1..)?.trim_start();
            let (commit, path) = rest.split_once(' ')?;
            let path = path.split(" (").next().unwrap_or(path).trim();
            if commit.is_empty() || path.is_empty() {
                return None;
            }
            Some(SubmoduleEntry {
                path: path.to_string(),
                commit: commit.to_string(),
            })
        })
        .collect()
}

/// `stash@{n}` only. Anything else is refused before git runs.
pub fn stash_id_ok(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("stash@{") else {
        return false;
    };
    let Some(number) = rest.strip_suffix('}') else {
        return false;
    };
    !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
}

pub(crate) fn remote_names(config: &Config, repo: &Repo) -> AppResult<Vec<String>> {
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [dir_arg.as_os_str(), os("remote")];
    let out = run_git_ok(config, &args, &repo.root)?;
    Ok(out
        .stdout
        .lines()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

pub(crate) fn stash_list(config: &Config, repo: &Repo) -> AppResult<Vec<StashEntry>> {
    let Some(cwd) = repo
        .worktrees
        .iter()
        .find(|wt| !wt.bare && wt.path.is_dir())
        .map(|wt| &wt.path)
    else {
        return Ok(Vec::new());
    };
    let args = [os("stash"), os("list"), os("--format=%gd\u{1f}%s")];
    let out = run_git_ok(config, &args, cwd)?;
    Ok(parse_stashes(&out.stdout))
}

pub(crate) fn submodule_list(config: &Config, repo: &Repo) -> AppResult<Vec<SubmoduleEntry>> {
    let Some(cwd) = repo
        .worktrees
        .iter()
        .find(|wt| !wt.bare && wt.path.is_dir())
        .map(|wt| &wt.path)
    else {
        return Ok(Vec::new());
    };
    let args = [os("submodule"), os("status")];
    let out = run_git(config, &args, cwd)?;
    if !out.success() {
        return Ok(Vec::new());
    }
    Ok(parse_submodules(&out.stdout))
}

/// `git push <remote> --delete refs/heads/<branch>`, then drops the stale remote-tracking ref.
/// The remote must be configured and the branch must be listed under it. No force is used.
pub fn delete_remote_branch(
    config: &Config,
    repo_root: &Path,
    remote: &str,
    branch: &str,
) -> AppResult<PathBuf> {
    if branch_syntax_error(remote).is_some() || branch_syntax_error(branch).is_some() {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid remote branch: {remote}/{branch}"),
        ));
    }
    let repo = resolve_repo(config, repo_root)?;
    let tracking = format!("refs/remotes/{remote}/{branch}");
    let known = remote_names(config, &repo)?
        .iter()
        .any(|name| name == remote);
    if !known || ref_exists(config, &repo, &tracking).is_err() {
        return Err(AppError::new(
            ErrorCode::BranchNotFound,
            format!("Remote branch not found: {remote}/{branch}"),
        ));
    }
    let dir_arg = git_dir_arg(&repo.git_dir);
    let target = format!("{REFS_HEADS}{branch}");
    let push = [
        dir_arg.as_os_str(),
        os("push"),
        os("--delete"),
        os(remote),
        os(&target),
    ];
    let out = run_git_timeout(config, &push, &repo.root, NETWORK_TIMEOUT)?;
    if !out.success() {
        return Err(failure(&push, &out));
    }
    drop_tracking_ref(config, &repo, &tracking)?;
    Ok(repo.root)
}

/// A bare clone has no fetch refspec, so `push --delete` leaves `refs/remotes/...` behind.
fn drop_tracking_ref(config: &Config, repo: &Repo, tracking: &str) -> AppResult<()> {
    if ref_exists(config, repo, tracking).is_err() {
        return Ok(());
    }
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("update-ref"),
        os("-d"),
        os(tracking),
    ];
    run_git_ok(config, &args, &repo.root).map(|_| ())
}

pub fn apply_stash(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    id: &str,
) -> AppResult<PathBuf> {
    if !stash_id_ok(id) {
        return Err(AppError::new(
            ErrorCode::GitFailed,
            format!("Not a stash id: {id}"),
        ));
    }
    let args = [os("stash"), os("apply"), os(id)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

pub fn drop_stash(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    id: &str,
) -> AppResult<PathBuf> {
    if !stash_id_ok(id) {
        return Err(AppError::new(
            ErrorCode::GitFailed,
            format!("Not a stash id: {id}"),
        ));
    }
    let args = [os("stash"), os("drop"), os(id)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

fn commit_hash_ok(hash: &str) -> bool {
    (7..=40).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit())
}

fn ref_name_ok(name: &str) -> bool {
    branch_syntax_error(name).is_none() && !name.starts_with('-')
}

/// `git cherry-pick <hash>` in the open worktree.
pub fn cherry_pick(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    hash: &str,
) -> AppResult<PathBuf> {
    if !commit_hash_ok(hash) {
        return Err(AppError::new(
            ErrorCode::GitFailed,
            format!("Not a commit hash: {hash}"),
        ));
    }
    let args = [os("cherry-pick"), os(hash)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

/// `git revert --no-edit <hash>` in the open worktree.
pub fn revert_commit(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    hash: &str,
) -> AppResult<PathBuf> {
    if !commit_hash_ok(hash) {
        return Err(AppError::new(
            ErrorCode::GitFailed,
            format!("Not a commit hash: {hash}"),
        ));
    }
    let args = [os("revert"), os("--no-edit"), os(hash)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

/// `git merge --no-edit <branch>` into the open worktree.
pub fn merge_ref(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    branch: &str,
) -> AppResult<PathBuf> {
    if !ref_name_ok(branch) {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid branch name '{branch}'"),
        ));
    }
    let args = [os("merge"), os("--no-edit"), os(branch)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

/// `git rebase <branch>` of the open worktree onto that branch.
pub fn rebase_onto(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    branch: &str,
) -> AppResult<PathBuf> {
    if !ref_name_ok(branch) {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid branch name '{branch}'"),
        ));
    }
    let args = [os("rebase"), os(branch)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

/// `git branch -d`. Refuses a branch that a worktree still has checked out.
pub fn delete_local_branch(config: &Config, repo_root: &Path, branch: &str) -> AppResult<PathBuf> {
    if !ref_name_ok(branch) {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid branch name '{branch}'"),
        ));
    }
    let repo = resolve_repo(config, repo_root)?;
    if repo
        .worktrees
        .iter()
        .any(|wt| wt.branch.as_deref() == Some(branch))
    {
        return Err(AppError::new(
            ErrorCode::BranchInUse,
            format!("'{branch}' is checked out in a worktree"),
        ));
    }
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("branch"),
        os("-d"),
        os(branch),
    ];
    let out = run_git(config, &args, &repo.root)?;
    if out.success() {
        Ok(repo.root)
    } else {
        Err(failure(&args, &out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_locations_from_refs() {
        let parsed = parse_ref_lines(
            "refs/heads/master\tabc\t\n\
             refs/remotes/origin/HEAD\t\t\n\
             refs/remotes/origin/master\tdef\t\n\
             refs/remotes/origin/feature/x\tghi\t\n\
             refs/tags/v1\ttagobj\tcommitsha\n",
        );
        assert_eq!(parsed.local, vec!["master".to_string()]);
        assert_eq!(parsed.remotes.len(), 1);
        assert_eq!(parsed.remotes[0].name, "origin");
        assert_eq!(
            parsed.remotes[0].branches,
            vec!["feature/x".to_string(), "master".to_string()]
        );
        assert_eq!(parsed.tags[0].name, "v1");
        assert_eq!(parsed.tags[0].commit, "commitsha");
        let stashes = parse_stashes("stash@{0}\u{1f}wip\nstash@{no}\u{1f}bad\n");
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].id, "stash@{0}");
        let mods = parse_submodules(" 0123456789abcdef path/one (v1)\n-abcdef path/two\n");
        assert_eq!(mods[0].path, "path/one");
        assert_eq!(mods[1].path, "path/two");
        assert!(stash_id_ok("stash@{12}"));
        assert!(!stash_id_ok("stash@{0};rm"));
    }
}
