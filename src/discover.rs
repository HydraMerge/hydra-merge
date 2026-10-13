//! Repository discovery, path checks and the mapping branch name -> worktree path.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::git::{git_dir_arg, is_dirty, list_worktrees_with, run_git, Status};
use crate::model::{
    AppError, AppResult, Config, ErrorCode, Layout, Repo, Worktree, BARE_DIRNAME, MAX_DEPTH,
    SKIP_DIRS,
};
use crate::pool::parallel_map;
use crate::trust::{ensure_trusted, inspect_or_block, require_trusted};

const MAX_BRANCH_BYTES: usize = 255;

/// Reloads one repository already shown in the window. Does not walk the discovery roots.
pub fn reload_repo(config: &Config, repo: &Repo) -> AppResult<Repo> {
    let mut fresh = match repo.layout {
        Layout::Bare => {
            let base = config
                .roots
                .iter()
                .find(|root| repo.root.starts_with(root))
                .map_or(repo.root.as_path(), |root| root.as_path());
            build_bare_repo_status(config, base, &repo.root, Status::Skip)?
        }
        Layout::Plain => build_plain_repo_with(config, &repo.root, Status::Skip)?,
    };
    fresh.name = repo.name.clone();
    // The worktrees' `git status` calls run in parallel.
    Ok(fill_status(config, vec![fresh]).remove(0))
}

/// Every repository under the discovery roots, with `git status` of each worktree.
pub fn discover_repos(config: &Config) -> AppResult<Vec<Repo>> {
    discover_repos_with(config, Status::Read)
}

/// A repository found by the walk: a `.bare` layout or a normal checkout with a `.git` directory.
enum Found {
    Bare { base: PathBuf, dir: PathBuf },
    Plain { dir: PathBuf },
}

/// Discovery on a few threads. With `Status::Skip` the worktrees come back with `dirty = None`
/// and `fill_status` adds it later: the list shows at once and the slow part runs behind it.
pub fn discover_repos_with(config: &Config, status: Status) -> AppResult<Vec<Repo>> {
    let hits = find_layouts(config);
    let built = parallel_map(&hits, |hit| match hit {
        Found::Bare { base, dir } => build_bare_repo_status(config, base, dir, Status::Skip)
            .map_err(|e| {
                crate::log_warn!("cannot list worktrees of {}: {}", dir.display(), e.message)
            })
            .ok(),
        Found::Plain { dir } => build_plain_repo_with(config, dir, Status::Skip)
            .map_err(|e| {
                crate::log_warn!("cannot list worktrees of {}: {}", dir.display(), e.message)
            })
            .ok(),
    });
    let repos: Vec<Repo> = built.into_iter().flatten().collect();
    let repos = match status {
        Status::Read => fill_status(config, repos),
        Status::Skip => repos,
    };
    Ok(sorted(repos))
}

/// Repositories under the discovery roots. The walk only reads directories; `.bare` is checked
/// with git in parallel. A directory whose `.git` is a directory is a normal repository.
fn find_layouts(config: &Config) -> Vec<Found> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut hits = Vec::new();
    for root in &config.roots {
        let mut queue = vec![(root.clone(), 0usize)];
        while !queue.is_empty() {
            let candidates = walk(&mut visited, queue);
            let checks = parallel_map(&candidates, |(dir, _)| {
                is_bare_repo(config, &dir.join(BARE_DIRNAME))
            });
            queue = Vec::new();
            for ((dir, depth), bare) in candidates.into_iter().zip(checks) {
                if bare {
                    hits.push(Found::Bare {
                        base: root.clone(),
                        dir,
                    });
                } else if dir.join(".git").is_dir() {
                    hits.push(Found::Plain { dir });
                } else if !dir.join(".git").exists() && depth < MAX_DEPTH {
                    // `.bare` that is not a repository: walk on below it, as before.
                    queue.extend(child_dirs(&dir, depth + 1));
                }
            }
        }
    }
    hits
}

/// Breadth-first walk. Directories holding `.bare` or a `.git` directory are returned and not
/// entered. A `.git` file is a linked worktree and is not a separate repository. Anything else
/// is entered up to `MAX_DEPTH`.
fn walk(visited: &mut HashSet<PathBuf>, start: Vec<(PathBuf, usize)>) -> Vec<(PathBuf, usize)> {
    let mut queue = VecDeque::from(start);
    let mut candidates = Vec::new();
    while let Some((dir, depth)) = queue.pop_front() {
        if !visited.insert(dir.clone()) {
            continue;
        }
        if dir.join(BARE_DIRNAME).exists() || dir.join(".git").is_dir() {
            candidates.push((dir, depth));
        } else if !dir.join(".git").exists() && depth < MAX_DEPTH {
            queue.extend(child_dirs(&dir, depth + 1));
        }
    }
    candidates
}

fn sorted(mut repos: Vec<Repo>) -> Vec<Repo> {
    repos.sort_by_key(|r| r.name.to_lowercase());
    repos
}

/// Worktrees whose `git status` is worth asking: not `.bare`, not prunable, still on disk.
fn needs_status(wt: &Worktree) -> bool {
    !wt.bare && !wt.prunable && wt.path.is_dir()
}

/// `git status` of every listed worktree, on a few threads. `None` where it failed.
pub fn read_statuses(config: &Config, repos: &[Repo]) -> HashMap<PathBuf, Option<bool>> {
    // `git status` can run filters of the repository's config: not in a repository that is not
    // trusted. Its config is read again here, the snapshot may be older.
    let allowed = parallel_map(repos, |repo| ensure_trusted(config, repo).is_ok());
    let paths: Vec<PathBuf> = repos
        .iter()
        .zip(allowed)
        .filter(|(_, allowed)| *allowed)
        .flat_map(|(repo, _)| &repo.worktrees)
        .filter(|wt| needs_status(wt))
        .map(|wt| wt.path.clone())
        .collect();
    let states = parallel_map(&paths, |path| match is_dirty(config, path) {
        Ok(dirty) => Some(dirty),
        Err(e) => {
            crate::log_warn!(
                "cannot read status of {}: {}",
                path.display(),
                e.message
            );
            None
        }
    });
    paths.into_iter().zip(states).collect()
}

/// Copies of `repos` with `dirty` taken from `statuses`; worktrees not in it keep theirs.
pub fn apply_statuses(repos: &[Repo], statuses: &HashMap<PathBuf, Option<bool>>) -> Vec<Repo> {
    repos
        .iter()
        .map(|repo| Repo {
            worktrees: repo
                .worktrees
                .iter()
                .map(|wt| Worktree {
                    dirty: statuses.get(&wt.path).copied().unwrap_or(wt.dirty),
                    ..wt.clone()
                })
                .collect(),
            ..repo.clone()
        })
        .collect()
}

/// `repos` with the status of every worktree read in parallel.
pub fn fill_status(config: &Config, repos: Vec<Repo>) -> Vec<Repo> {
    let statuses = read_statuses(config, &repos);
    apply_statuses(&repos, &statuses)
}

fn child_dirs(dir: &Path, depth: usize) -> Vec<(PathBuf, usize)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            crate::log_warn!("cannot scan {}: {}", dir.display(), e);
            return Vec::new();
        }
    };
    let mut children: Vec<PathBuf> = entries
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(e) => {
                crate::log_warn!("cannot scan {}: {}", dir.display(), e);
                None
            }
        })
        .filter(is_scannable)
        .map(|entry| entry.path())
        .collect();
    children.sort();
    children.into_iter().map(|path| (path, depth)).collect()
}

fn is_scannable(entry: &std::fs::DirEntry) -> bool {
    let name = entry.file_name();
    let name = name.to_string_lossy();
    if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
        return false;
    }
    match entry.file_type() {
        Ok(kind) => kind.is_dir() && !kind.is_symlink(),
        Err(e) => {
            crate::log_warn!("cannot scan {}: {}", entry.path().display(), e);
            false
        }
    }
}

/// `<root>/.bare` is a bare repository.
pub fn is_bare_repo(config: &Config, bare_path: &Path) -> bool {
    let cwd = bare_path.parent().unwrap_or(bare_path);
    let dir_arg = git_dir_arg(bare_path);
    let args = [
        dir_arg.as_os_str(),
        OsStr::new("rev-parse"),
        OsStr::new("--is-bare-repository"),
    ];
    match run_git(config, &args, cwd) {
        Ok(out) => out.status == Some(0) && out.stdout.trim() == "true",
        Err(_) => false,
    }
}

/// Name of `root` relative to the discovery root that contains it.
fn repo_name(config: &Config, root: &Path) -> String {
    let relative = config
        .roots
        .iter()
        .find_map(|base| root.strip_prefix(base).ok())
        .filter(|rel| !rel.as_os_str().is_empty());
    match relative {
        Some(rel) => rel.to_string_lossy().into_owned(),
        None => root.file_name().map_or_else(
            || root.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        ),
    }
}

fn build_bare_repo(config: &Config, discovery_root: &Path, root: &Path) -> AppResult<Repo> {
    build_bare_repo_status(config, discovery_root, root, Status::Read)
}

fn build_bare_repo_status(
    config: &Config,
    discovery_root: &Path,
    root: &Path,
    status: Status,
) -> AppResult<Repo> {
    let git_dir = root.join(BARE_DIRNAME);
    let worktrees = list_worktrees_with(config, root, &git_dir, Layout::Bare, status)?;
    let name = repo_name_under(config, discovery_root, root);
    let sources = inspect_or_block(config, &git_dir, root);
    Ok(Repo {
        root: root.to_path_buf(),
        name,
        layout: Layout::Bare,
        git_dir,
        worktrees,
        code_sources: sources.names,
        trust_key: sources.fingerprint,
    })
}

/// One bare-layout repository, named relative to the first discovery root that holds it.
fn build_bare_repo_with(config: &Config, root: &Path, status: Status) -> AppResult<Repo> {
    let git_dir = root.join(BARE_DIRNAME);
    let worktrees = list_worktrees_with(config, root, &git_dir, Layout::Bare, status)?;
    let sources = inspect_or_block(config, &git_dir, root);
    Ok(Repo {
        root: root.to_path_buf(),
        name: repo_name(config, root),
        layout: Layout::Bare,
        git_dir,
        worktrees,
        code_sources: sources.names,
        trust_key: sources.fingerprint,
    })
}

fn repo_name_under(config: &Config, discovery_root: &Path, root: &Path) -> String {
    match root.strip_prefix(discovery_root) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => repo_name(config, root),
    }
}

// ---------------------------------------------------------------------------
// Branch name -> path
// ---------------------------------------------------------------------------

/// Checks 1-4 of the branch name rules (no git). `None` means the syntax is acceptable.
pub fn branch_syntax_error(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("empty name");
    }
    if name.len() > MAX_BRANCH_BYTES {
        return Some("longer than 255 bytes");
    }
    if name.contains(['\0', '\\', '\n', '\r']) {
        return Some("contains a forbidden character");
    }
    if name.starts_with('/') || name.starts_with('-') || name.ends_with('/') {
        return Some("must not start with '/' or '-' or end with '/'");
    }
    if name == BARE_DIRNAME {
        return Some("reserved name");
    }
    if name
        .split('/')
        .any(|seg| seg.is_empty() || seg.starts_with('.'))
    {
        return Some("empty or hidden path segment");
    }
    None
}

pub fn layout_path_for(repo_root: &Path, branch: &str) -> AppResult<PathBuf> {
    if let Some(reason) = branch_syntax_error(branch) {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid branch name '{branch}': {reason}"),
        ));
    }
    let path = branch
        .split('/')
        .fold(repo_root.to_path_buf(), |acc, seg| acc.join(seg));
    if path == repo_root || !path.starts_with(repo_root) {
        return Err(AppError::new(
            ErrorCode::InvalidBranch,
            format!("Invalid branch name '{branch}': escapes the repository root"),
        ));
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Path checks
// ---------------------------------------------------------------------------

fn outside_roots(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::OutsideRoots,
        format!("Path is outside discovery roots: {}", path.display()),
    )
}

/// Canonicalizes `path` and requires it to be under one of `config.roots`.
pub fn ensure_under_roots(config: &Config, path: &Path) -> AppResult<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(canonical) if config.roots.iter().any(|r| canonical.starts_with(r)) => Ok(canonical),
        Ok(_) => Err(outside_roots(path)),
        Err(e) if config.roots.iter().any(|r| path.starts_with(r)) => Err(AppError::io(
            &format!("cannot access {}", path.display()),
            &e,
        )),
        Err(_) => Err(outside_roots(path)),
    }
}

/// Discovery plus the plain clones added by hand, as one snapshot.
pub fn scan_all(config: &Config, extra_roots: &[PathBuf]) -> AppResult<Vec<Repo>> {
    scan_all_with(config, extra_roots, Status::Read)
}

/// `scan_all` where `Status::Skip` leaves the status of discovered worktrees for later.
pub fn scan_all_with(
    config: &Config,
    extra_roots: &[PathBuf],
    status: Status,
) -> AppResult<Vec<Repo>> {
    let mut repos = discover_repos_with(config, status)?;
    for path in extra_roots {
        match open_path(config, path) {
            Ok(repo) if repos.iter().all(|r| r.root != repo.root) => repos.push(repo),
            Ok(_) => {}
            Err(e) => crate::log_warn!("cannot rescan {}: {}", path.display(), e),
        }
    }
    Ok(sorted(repos))
}

/// `root` must equal a repository root. Only that repository is read: no discovery walk and no
/// `git status`. A plain clone under the roots is still recognised, so create/remove can answer
/// `PlainLayout` for it.
pub fn resolve_repo(config: &Config, root: &Path) -> AppResult<Repo> {
    let canonical = ensure_under_roots(config, root)?;
    let unknown = || {
        AppError::new(
            ErrorCode::UnknownRepo,
            format!("Unknown repository: {}", root.display()),
        )
    };
    let bare = canonical.join(BARE_DIRNAME);
    let repo = if bare.exists() && is_bare_repo(config, &bare) {
        build_bare_repo_with(config, &canonical, Status::Skip)?
    } else {
        match git_toplevel(config, &canonical) {
            Ok(Some(top)) if top == canonical => build_plain_repo_with(config, &top, Status::Skip)?,
            _ => return Err(unknown()),
        }
    };
    require_trusted(config, &repo)?;
    Ok(repo)
}

pub fn resolve_worktree(config: &Config, path: &Path) -> AppResult<(Repo, Worktree)> {
    resolve_worktree_with(config, &[], path)
}

/// Checks of the path before any git command (stage, commit, remove, load). Reads only the
/// repository that owns `path`: plain clones count only when they were added by hand
/// (`extra_roots`). A repository whose config can run programs must be trusted first.
pub fn resolve_worktree_with(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<(Repo, Worktree)> {
    let (repo, wt) = resolve_worktree_for_launch(config, extra_roots, path)?;
    require_trusted(config, &repo)?;
    Ok((repo, wt))
}

/// The same checks of the path without the trust check, for starting an editor, Terminal or
/// Finder on the worktree: no git config is run on the way.
pub fn resolve_worktree_for_launch(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<(Repo, Worktree)> {
    let canonical = ensure_under_roots(config, path)?;
    if canonical
        .components()
        .any(|c| c.as_os_str() == BARE_DIRNAME)
    {
        return Err(bare_not_openable());
    }
    let bare = canonical.join(BARE_DIRNAME);
    if bare.exists() && is_bare_repo(config, &bare) {
        return Err(AppError::new(
            ErrorCode::ParentNotOpenable,
            format!(
                "The repository root is not a worktree: {}",
                canonical.display()
            ),
        ));
    }
    let unknown = || {
        AppError::new(
            ErrorCode::UnknownWorktree,
            format!("Unknown worktree: {}", path.display()),
        )
    };
    let repo = owning_repo(config, extra_roots, &canonical)?.ok_or_else(unknown)?;
    let found = repo
        .worktrees
        .iter()
        .find(|wt| wt.path == canonical)
        .cloned();
    match found {
        Some(wt) if wt.bare => Err(bare_not_openable()),
        Some(wt) => Ok((repo, wt)),
        None => Err(unknown()),
    }
}

/// The repository whose common git dir serves `dir`: `<root>/.bare` for the bare layout,
/// `<toplevel>/.git` for a plain clone added by hand. `None` for anything else.
fn owning_repo(config: &Config, extra_roots: &[PathBuf], dir: &Path) -> AppResult<Option<Repo>> {
    let args = [OsStr::new("rev-parse"), OsStr::new("--git-common-dir")];
    let out = run_git(config, &args, dir)?;
    let line = out.stdout.trim();
    if out.status != Some(0) || line.is_empty() {
        return Ok(None);
    }
    let Ok(common) = std::fs::canonicalize(dir.join(line)) else {
        return Ok(None);
    };
    let Some(root) = common.parent().map(Path::to_path_buf) else {
        return Ok(None);
    };
    if common.file_name() == Some(OsStr::new(BARE_DIRNAME)) && is_bare_repo(config, &common) {
        let root = ensure_under_roots(config, &root)?;
        return build_bare_repo_with(config, &root, Status::Skip).map(Some);
    }
    let under_scan = config.roots.iter().any(|base| root.starts_with(base));
    if common.file_name() == Some(OsStr::new(".git"))
        && (under_scan || extra_roots.contains(&root))
    {
        let root = ensure_under_roots(config, &root)?;
        return build_plain_repo_with(config, &root, Status::Skip).map(Some);
    }
    Ok(None)
}

fn bare_not_openable() -> AppError {
    AppError::new(
        ErrorCode::BareNotOpenable,
        "The .bare directory cannot be opened or removed",
    )
}

/// Manual add of a repository ("Open path...").
pub fn open_path(config: &Config, path: &Path) -> AppResult<Repo> {
    let canonical = ensure_under_roots(config, path)?;
    let not_a_repo = || {
        AppError::new(
            ErrorCode::NotARepo,
            format!("Not a git repository: {}", path.display()),
        )
    };
    if !canonical.is_dir() {
        return Err(not_a_repo());
    }
    let bare = canonical.join(BARE_DIRNAME);
    if bare.exists() && is_bare_repo(config, &bare) {
        return build_bare_repo(config, &canonical, &canonical).map(|repo| Repo {
            name: repo_name(config, &canonical),
            ..repo
        });
    }
    let toplevel = git_toplevel(config, &canonical)?.ok_or_else(not_a_repo)?;
    build_plain_repo(config, &toplevel)
}

fn git_toplevel(config: &Config, dir: &Path) -> AppResult<Option<PathBuf>> {
    let args = [OsStr::new("rev-parse"), OsStr::new("--show-toplevel")];
    let out = run_git(config, &args, dir)?;
    let line = out.stdout.trim();
    if out.status != Some(0) || line.is_empty() {
        return Ok(None);
    }
    let top = PathBuf::from(line);
    Ok(Some(std::fs::canonicalize(&top).unwrap_or(top)))
}

fn build_plain_repo(config: &Config, toplevel: &Path) -> AppResult<Repo> {
    build_plain_repo_with(config, toplevel, Status::Read)
}

fn build_plain_repo_with(config: &Config, toplevel: &Path, status: Status) -> AppResult<Repo> {
    let args = [OsStr::new("rev-parse"), OsStr::new("--absolute-git-dir")];
    let out = run_git(config, &args, toplevel)?;
    let git_dir = PathBuf::from(out.stdout.trim());
    let worktrees = list_worktrees_with(config, toplevel, &git_dir, Layout::Plain, status)?;
    let sources = inspect_or_block(config, &git_dir, toplevel);
    Ok(Repo {
        root: toplevel.to_path_buf(),
        name: repo_name(config, toplevel),
        layout: Layout::Plain,
        git_dir,
        worktrees,
        code_sources: sources.names,
        trust_key: sources.fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_rules() {
        for bad in [
            "", "..", "../x", "a/../b", "/abs", ".bare", "x/.bare", "-x", "a//b", "a/", ".hidden",
            "a\\b", "a\nb",
        ] {
            assert!(
                branch_syntax_error(bad).is_some(),
                "{bad:?} must be rejected"
            );
        }
        for good in ["master", "feature/x", "release-1.2", "a/b/c"] {
            assert!(
                branch_syntax_error(good).is_none(),
                "{good:?} must be accepted"
            );
        }
        assert!(branch_syntax_error(&"a".repeat(256)).is_some());
    }

    #[test]
    fn layout_path_is_joined_per_segment() {
        let root = Path::new("/r");
        assert_eq!(
            layout_path_for(root, "feature/x").ok(),
            Some(PathBuf::from("/r/feature/x"))
        );
        assert!(layout_path_for(root, "..").is_err());
    }
}
