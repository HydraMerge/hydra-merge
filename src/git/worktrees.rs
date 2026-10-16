//! Worktree list, create and remove.

use super::*;

pub(crate) const SHORT_HEAD_LEN: usize = 7;

pub(crate) fn invalid_branch(name: &str, reason: &str) -> AppError {
    AppError::new(
        ErrorCode::InvalidBranch,
        format!("Invalid branch name '{name}': {reason}"),
    )
}

pub fn validate_branch_name(config: &Config, name: &str) -> AppResult<()> {
    let cwd = config
        .roots
        .first()
        .map_or_else(|| PathBuf::from("."), PathBuf::clone);
    validate_branch_name_in(config, &cwd, name)
}

pub fn validate_branch_name_in(config: &Config, cwd: &Path, name: &str) -> AppResult<()> {
    if let Some(reason) = branch_syntax_error(name) {
        return Err(invalid_branch(name, reason));
    }
    let args = [os("check-ref-format"), os("--branch"), os(name)];
    let out = run_git(config, &args, cwd)?;
    if out.success() {
        Ok(())
    } else {
        Err(invalid_branch(name, "rejected by git check-ref-format").with_detail(&out.stderr))
    }
}

// ---------------------------------------------------------------------------
// worktree list
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawWorktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

impl RawWorktree {
    fn at(path: &str) -> RawWorktree {
        RawWorktree {
            path: PathBuf::from(path),
            head: None,
            branch: None,
            bare: false,
            detached: false,
            locked: false,
            prunable: false,
        }
    }

    fn with_line(self, line: &str) -> RawWorktree {
        if let Some(sha) = line.strip_prefix("HEAD ") {
            let head = sha.get(..SHORT_HEAD_LEN).unwrap_or(sha).to_string();
            return RawWorktree {
                head: Some(head),
                ..self
            };
        }
        if let Some(name) = line.strip_prefix("branch ") {
            let branch = name.strip_prefix(REFS_HEADS).unwrap_or(name).to_string();
            return RawWorktree {
                branch: Some(branch),
                ..self
            };
        }
        let flag = |word: &str| line == word || line.starts_with(&format!("{word} "));
        RawWorktree {
            bare: self.bare || line == "bare",
            detached: self.detached || line == "detached",
            locked: self.locked || flag("locked"),
            prunable: self.prunable || flag("prunable"),
            ..self
        }
    }
}

/// Pure parser of `git worktree list --porcelain`. Unknown lines are ignored.
pub fn parse_worktree_porcelain(stdout: &str) -> Vec<RawWorktree> {
    let mut done = Vec::new();
    let mut current: Option<RawWorktree> = None;
    for line in stdout.lines() {
        if line.is_empty() {
            done.extend(current.take());
        } else if let Some(path) = line.strip_prefix("worktree ") {
            done.extend(current.take());
            current = Some(RawWorktree::at(path));
        } else {
            current = current.map(|w| w.with_line(line));
        }
    }
    done.extend(current);
    done
}

pub(crate) fn list_args<'a>(
    layout: Layout,
    git_dir_arg: &'a OsStr,
    root: &'a Path,
) -> Vec<&'a OsStr> {
    match layout {
        Layout::Bare => vec![git_dir_arg, os("worktree"), os("list"), os("--porcelain")],
        Layout::Plain => {
            vec![
                os("-C"),
                root.as_os_str(),
                os("worktree"),
                os("list"),
                os("--porcelain"),
            ]
        }
    }
}

pub fn list_worktrees(
    config: &Config,
    repo_root: &Path,
    git_dir: &Path,
    layout: Layout,
) -> AppResult<Vec<Worktree>> {
    list_worktrees_with(config, repo_root, git_dir, layout, Status::Read)
}

/// Whether listing also runs `git status` in every worktree to fill `Worktree::dirty`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Read,
    /// Path checks before an action need only the list; `dirty` stays `None`.
    Skip,
}

pub fn list_worktrees_with(
    config: &Config,
    repo_root: &Path,
    git_dir: &Path,
    layout: Layout,
    status: Status,
) -> AppResult<Vec<Worktree>> {
    let dir_arg = git_dir_arg(git_dir);
    let args = list_args(layout, &dir_arg, repo_root);
    let out = run_git_ok(config, &args, repo_root)?;
    // Listing runs no config. `git status` does (filters), so it waits for trust.
    let status = match status {
        Status::Read if !crate::trust::allows_git(config, repo_root, git_dir) => Status::Skip,
        other => other,
    };
    Ok(parse_worktree_porcelain(&out.stdout)
        .into_iter()
        .map(|raw| build_worktree(config, repo_root, layout, raw, status))
        .collect())
}

pub(crate) fn build_worktree(
    config: &Config,
    repo_root: &Path,
    layout: Layout,
    raw: RawWorktree,
    status: Status,
) -> Worktree {
    let path = std::fs::canonicalize(&raw.path).unwrap_or_else(|_| raw.path.clone());
    let skip = status == Status::Skip || raw.bare || raw.prunable || !path.is_dir();
    let dirty = if skip {
        None
    } else {
        match is_dirty(config, &path) {
            Ok(dirty) => Some(dirty),
            Err(e) => {
                crate::log_warn!(
                    "cannot read status of {}: {}",
                    path.display(),
                    e.message
                );
                None
            }
        }
    };
    let layout_path = expected_layout_path(layout, repo_root, &raw);
    let layout_correct = layout_path.as_deref() == Some(path.as_path());
    Worktree {
        path,
        branch: raw.branch,
        head: raw.head,
        bare: raw.bare,
        detached: raw.detached,
        locked: raw.locked,
        prunable: raw.prunable,
        dirty,
        layout_path,
        layout_correct,
    }
}

pub(crate) fn expected_layout_path(
    layout: Layout,
    repo_root: &Path,
    raw: &RawWorktree,
) -> Option<PathBuf> {
    if layout == Layout::Plain || raw.bare {
        return None;
    }
    let branch = raw.branch.as_deref()?;
    layout_path_for(repo_root, branch).ok()
}

/// Untracked files count as dirty, same as `git worktree remove` sees them.
pub(crate) fn is_dirty(config: &Config, wt_path: &Path) -> AppResult<bool> {
    let args = [
        os("-C"),
        wt_path.as_os_str(),
        os("status"),
        os("--porcelain=v1"),
        os("--untracked-files=normal"),
    ];
    let out = run_git_ok(config, &args, wt_path)?;
    Ok(!out.stdout.trim().is_empty())
}

pub fn create_tracking_worktree(
    config: &Config,
    repo_root: &Path,
    remote: &str,
    branch: &str,
) -> AppResult<Worktree> {
    let (repo, start) = remote_branch_preflight(config, repo_root, remote, branch)?;
    let listed = list_worktrees_with(config, &repo.root, &repo.git_dir, repo.layout, Status::Skip)?;
    let dest = layout_path_for(&repo.root, branch)?;
    check_destination(&dest, &listed)?;
    let dir_arg = git_dir_arg(&repo.git_dir);
    let head = [dir_arg.as_os_str(), os("worktree"), os("add")];
    let tail = [os("-b"), os(branch), dest.as_os_str(), os(&start)];
    run_with_track(config, &repo, &head, &tail, branch)?;
    find_created(config, &repo, dest)
}

/// `git branch --track <branch> <remote>/<branch>`: a local branch without a worktree.
/// Returns the repository root so the window can reload it.
pub fn create_tracking_branch(
    config: &Config,
    repo_root: &Path,
    remote: &str,
    branch: &str,
) -> AppResult<PathBuf> {
    let (repo, start) = remote_branch_preflight(config, repo_root, remote, branch)?;
    let dir_arg = git_dir_arg(&repo.git_dir);
    let head = [dir_arg.as_os_str(), os("branch")];
    let tail = [os(branch), os(&start)];
    run_with_track(config, &repo, &head, &tail, branch)?;
    Ok(repo.root)
}

/// Checks shared by both ways of starting from a remote branch: valid names, a `.bare` layout,
/// no local branch of that name yet, and an existing `refs/remotes/<remote>/<branch>`.
fn remote_branch_preflight(
    config: &Config,
    repo_root: &Path,
    remote: &str,
    branch: &str,
) -> AppResult<(Repo, String)> {
    if branch_syntax_error(remote).is_some() || branch_syntax_error(branch).is_some() {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid remote branch: {remote}/{branch}"),
        ));
    }
    let root = ensure_under_roots(config, repo_root)?;
    let repo = resolve_repo(config, &root)?;
    if repo.layout == Layout::Plain {
        return Err(plain_layout_error());
    }
    validate_branch_name_in(config, &repo.root, branch)?;
    if branch_exists(config, &repo, branch).is_ok() {
        return Err(AppError::new(
            ErrorCode::BranchInUse,
            format!("Local branch already exists: {branch}"),
        ));
    }
    let start = format!("{remote}/{branch}");
    ref_exists(config, &repo, &format!("refs/remotes/{start}")).map_err(|_| {
        AppError::new(
            ErrorCode::BranchNotFound,
            format!("Remote branch not found: {start}"),
        )
    })?;
    Ok((repo, start))
}

/// Runs `head --track tail`. A bare clone without `remote.<name>.fetch` cannot track; git then
/// creates nothing, so the command is repeated without `--track` and the reason goes to stderr.
fn run_with_track(
    config: &Config,
    repo: &Repo,
    head: &[&OsStr],
    tail: &[&OsStr],
    branch: &str,
) -> AppResult<()> {
    let track = os("--track");
    let tracked: Vec<&OsStr> = head.iter().chain([&track]).chain(tail).copied().collect();
    let out = run_git(config, &tracked, &repo.root)?;
    if out.success() {
        return Ok(());
    }
    if !out.stderr.contains("cannot set up tracking information") {
        return Err(failure(&tracked, &out));
    }
    crate::log_warn!("{branch} has no upstream: {}", out.stderr.trim());
    let plain: Vec<&OsStr> = head.iter().chain(tail).copied().collect();
    let out = run_git(config, &plain, &repo.root)?;
    if out.success() {
        Ok(())
    } else {
        Err(failure(&plain, &out))
    }
}

/// The new worktree from a fresh list; only its own status is read.
pub(crate) fn find_created(config: &Config, repo: &Repo, dest: PathBuf) -> AppResult<Worktree> {
    let fresh = list_worktrees_with(config, &repo.root, &repo.git_dir, repo.layout, Status::Skip)?;
    let created = std::fs::canonicalize(&dest).unwrap_or(dest);
    let wt = fresh
        .into_iter()
        .find(|wt| wt.path == created)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::GitFailed,
                "git worktree add succeeded but the worktree is not listed",
            )
        })?;
    let dirty = is_dirty(config, &wt.path).ok();
    Ok(Worktree { dirty, ..wt })
}

pub(crate) fn worktree_of_branch<'a>(
    worktrees: &'a [Worktree],
    branch: &str,
) -> Option<&'a Worktree> {
    worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
}

pub(crate) fn branch_exists(config: &Config, repo: &Repo, branch: &str) -> AppResult<()> {
    ref_exists(config, repo, &format!("{REFS_HEADS}{branch}")).map_err(|_| {
        AppError::new(
            ErrorCode::BranchNotFound,
            format!("Local branch not found: {branch}"),
        )
    })
}

/// `rev-parse --verify --quiet <full ref>`; `Err` when the ref is missing.
pub(crate) fn ref_exists(config: &Config, repo: &Repo, full_ref: &str) -> AppResult<()> {
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("rev-parse"),
        os("--verify"),
        os("--quiet"),
        os(full_ref),
    ];
    let out = run_git(config, &args, &repo.root)?;
    if out.success() {
        Ok(())
    } else {
        Err(failure(&args, &out))
    }
}

// ---------------------------------------------------------------------------
// Create worktree
// ---------------------------------------------------------------------------

pub fn create_worktree(config: &Config, repo_root: &Path, branch: &str) -> AppResult<Worktree> {
    let (repo, dest) = create_preflight(config, repo_root, branch)?;
    run_worktree_add(config, &repo, branch, &dest, false)?;
    find_created(config, &repo, dest)
}

/// Checks shared by every way of creating a worktree for a local branch: a `.bare` layout, a
/// valid existing branch that no worktree holds, and a free `<repo_root>/<branch>`. Returns the
/// repository and that destination; nothing is written yet.
pub(crate) fn create_preflight(
    config: &Config,
    repo_root: &Path,
    branch: &str,
) -> AppResult<(Repo, PathBuf)> {
    let root = ensure_under_roots(config, repo_root)?;
    let repo = resolve_repo(config, &root)?;
    if repo.layout == Layout::Plain {
        return Err(plain_layout_error());
    }
    validate_branch_name_in(config, &repo.root, branch)?;
    branch_exists(config, &repo, branch)?;
    let listed = list_worktrees_with(config, &repo.root, &repo.git_dir, repo.layout, Status::Skip)?;
    if let Some(wt) = worktree_of_branch(&listed, branch) {
        return Err(branch_in_use(branch, &wt.path));
    }
    let dest = layout_path_for(&repo.root, branch)?;
    check_destination(&dest, &listed)?;
    Ok((repo, dest))
}

pub(crate) fn plain_layout_error() -> AppError {
    AppError::new(
        ErrorCode::PlainLayout,
        "Create/remove is only available for .bare layout",
    )
}

pub(crate) fn branch_in_use(branch: &str, path: &Path) -> AppError {
    AppError::new(
        ErrorCode::BranchInUse,
        format!(
            "Branch '{branch}' is already used by worktree at {}",
            path.display()
        ),
    )
}

pub(crate) fn check_destination(dest: &Path, listed: &[Worktree]) -> AppResult<()> {
    if dest.symlink_metadata().is_ok() {
        return Err(AppError::new(
            ErrorCode::DestExists,
            format!("Destination already exists: {}", dest.display()),
        ));
    }
    let overlap = listed
        .iter()
        .filter(|wt| !wt.bare)
        .find(|wt| dest.starts_with(&wt.path) || wt.path.starts_with(dest));
    match overlap {
        Some(wt) => Err(AppError::new(
            ErrorCode::DestNested,
            format!("Destination overlaps worktree at {}", wt.path.display()),
        )),
        None => Ok(()),
    }
}

pub(crate) fn run_worktree_add(
    config: &Config,
    repo: &Repo,
    branch: &str,
    dest: &Path,
    no_checkout: bool,
) -> AppResult<()> {
    let dir_arg = git_dir_arg(&repo.git_dir);
    let mut args = vec![dir_arg.as_os_str(), os("worktree"), os("add")];
    if no_checkout {
        args.push(os("--no-checkout"));
    }
    args.extend([dest.as_os_str(), os(branch)]);
    let out = run_git(config, &args, &repo.root)?;
    if out.success() {
        return Ok(());
    }
    if out.stderr.contains("is already used by worktree")
        || out.stderr.contains("is already checked out at")
    {
        let listed =
            list_worktrees_with(config, &repo.root, &repo.git_dir, repo.layout, Status::Skip)?;
        let at = worktree_of_branch(&listed, branch).map_or(dest, |wt| wt.path.as_path());
        return Err(branch_in_use(branch, at).with_detail(&out.stderr));
    }
    if out.stderr.contains("already exists") {
        return Err(AppError::new(
            ErrorCode::DestExists,
            format!("Destination already exists: {}", dest.display()),
        )
        .with_detail(&out.stderr));
    }
    Err(failure(&args, &out))
}

// ---------------------------------------------------------------------------
// Remove worktree
// ---------------------------------------------------------------------------

pub fn remove_worktree(config: &Config, path: &Path) -> AppResult<PathBuf> {
    remove_worktree_with(config, &[], path)
}

/// `extra_roots` are plain clones added by hand in the window.
pub fn remove_worktree_with(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<PathBuf> {
    let (repo, wt) = resolve_worktree_with(config, extra_roots, path)?;
    if repo.layout == Layout::Plain {
        return Err(plain_layout_error());
    }
    if wt.path == repo.root {
        return Err(AppError::new(
            ErrorCode::MainWorktree,
            format!("Main worktree cannot be removed: {}", wt.path.display()),
        ));
    }
    if wt.locked {
        return Err(AppError::new(
            ErrorCode::LockedWorktree,
            format!("Worktree is locked: {}", wt.path.display()),
        ));
    }
    if is_dirty(config, &wt.path)? {
        return Err(dirty_error(&wt.path, ""));
    }
    run_worktree_remove(config, &repo, &wt.path)?;
    Ok(wt.path)
}

pub(crate) fn dirty_error(path: &Path, stderr: &str) -> AppError {
    AppError::new(
        ErrorCode::DirtyWorktree,
        format!("Worktree has changes, not removed: {}", path.display()),
    )
    .with_detail(stderr)
}

pub(crate) fn run_worktree_remove(config: &Config, repo: &Repo, wt_path: &Path) -> AppResult<()> {
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("worktree"),
        os("remove"),
        wt_path.as_os_str(),
    ];
    let out = run_git(config, &args, &repo.root)?;
    if out.success() {
        return Ok(());
    }
    if out.stderr.contains("modified or untracked") || out.stderr.contains("is dirty") {
        return Err(dirty_error(wt_path, &out.stderr));
    }
    Err(failure(&args, &out))
}

/// Name of the bare directory, re-exported for callers that build `.bare` paths.
pub fn bare_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(BARE_DIRNAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORCELAIN: &str = "worktree /r/.bare\nbare\n\n\
        worktree /r/master\nHEAD 0123456789abcdef0123456789abcdef01234567\nbranch refs/heads/master\n\n\
        worktree /r/feature/x\nHEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nbranch refs/heads/feature/x\n\n\
        worktree /r/det\nHEAD bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\ndetached\n\n\
        worktree /r/lk\nHEAD cccccccccccccccccccccccccccccccccccccccc\nbranch refs/heads/lk\nlocked reason here\n\n\
        worktree /r/gone\nHEAD dddddddddddddddddddddddddddddddddddddddd\ndetached\nprunable gitdir file points to non-existent location\nmystery line\n";

    #[test]
    fn parse_porcelain_bare_and_branches() {
        let list = parse_worktree_porcelain(PORCELAIN);
        assert_eq!(list.len(), 6);
        assert!(list[0].bare && list[0].head.is_none() && list[0].branch.is_none());
        assert_eq!(list[1].branch.as_deref(), Some("master"));
        assert_eq!(list[1].head.as_deref(), Some("0123456"));
        assert_eq!(list[2].branch.as_deref(), Some("feature/x"));
        assert!(list[3].detached && list[3].branch.is_none());
        assert!(list[4].locked && !list[4].prunable);
        assert!(list[5].prunable && list[5].detached && !list[5].locked);
    }

    #[test]
    fn parse_porcelain_without_trailing_blank_line() {
        let list = parse_worktree_porcelain("worktree /a\nHEAD 1234567890\nbranch refs/heads/a");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, PathBuf::from("/a"));
    }
}
