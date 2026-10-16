//! Sparse worktrees: a worktree that holds only some top-level folders of its branch, through
//! `git sparse-checkout` in cone mode. The idea is OpenScion's one-directory clone: in a large
//! repository a worktree should cost the slice you work on, not the whole tree. Files at the
//! root of the branch stay, as cone mode always keeps them.

use super::*;

/// Top-level folders of `branch`, sorted as git lists them. Reads objects only, so it runs no
/// program from the repository's config.
pub fn top_folders(config: &Config, repo_root: &Path, branch: &str) -> AppResult<Vec<String>> {
    let root = ensure_under_roots(config, repo_root)?;
    let repo = resolve_repo(config, &root)?;
    validate_branch_name_in(config, &repo.root, branch)?;
    branch_exists(config, &repo, branch)?;
    folders_of(config, &repo, branch)
}

fn folders_of(config: &Config, repo: &Repo, branch: &str) -> AppResult<Vec<String>> {
    let dir_arg = git_dir_arg(&repo.git_dir);
    let rev = format!("{REFS_HEADS}{branch}");
    let args = [
        dir_arg.as_os_str(),
        os("ls-tree"),
        os("-d"),
        os("--name-only"),
        os("-z"),
        os(&rev),
    ];
    let out = run_git_ok(config, &args, &repo.root)?;
    Ok(out
        .stdout
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

/// `git worktree add --no-checkout`, then `sparse-checkout set --cone <folders>` and
/// `read-tree -mu HEAD` inside it, so only the named folders are ever written to disk.
///
/// Every check runs before the first write: the same ones as [`create_worktree`], trust (the
/// files are written through the repository's filters), and each folder must be a top-level
/// folder of the branch. A name that is not one, or that git could read as an option, is
/// refused, so no pattern of ours can reach outside the folders the user picked.
pub fn create_sparse_worktree(
    config: &Config,
    repo_root: &Path,
    branch: &str,
    folders: &[String],
) -> AppResult<Worktree> {
    let (repo, dest) = create_preflight(config, repo_root, branch)?;
    check_folders(&folders_of(config, &repo, branch)?, folders)?;
    crate::trust::ensure_trusted(config, &repo)?;
    run_worktree_add(config, &repo, branch, &dest, true)?;
    fill_sparse(config, &dest, folders).map_err(|e| {
        AppError::new(
            e.code,
            format!(
                "The worktree at {} was created without files: {}",
                dest.display(),
                e.message
            ),
        )
        .with_detail(e.detail.as_deref().unwrap_or_default())
    })?;
    find_created(config, &repo, dest)
}

fn check_folders(available: &[String], folders: &[String]) -> AppResult<()> {
    if folders.is_empty() {
        return Err(unknown_folder("Pick at least one folder"));
    }
    for folder in folders {
        let plain = rel_path_ok(folder) && !folder.contains('/');
        if !plain || !available.contains(folder) {
            return Err(unknown_folder(&format!(
                "'{folder}' is not a top-level folder of the branch"
            )));
        }
    }
    Ok(())
}

fn unknown_folder(message: &str) -> AppError {
    AppError::new(ErrorCode::UnknownFolder, message)
}

fn fill_sparse(config: &Config, dest: &Path, folders: &[String]) -> AppResult<()> {
    let mut set = vec![os("sparse-checkout"), os("set"), os("--cone")];
    set.extend(folders.iter().map(|f| OsStr::new(f.as_str())));
    run_git_ok(config, &set, dest)?;
    run_git_ok(config, &[os("read-tree"), os("-mu"), os("HEAD")], dest)?;
    Ok(())
}

/// The folders of a sparse worktree, or `None` for a full one.
pub fn sparse_folders(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<Option<Vec<String>>> {
    sparse_list(config, &worktree_dir(config, extra_roots, path)?)
}

/// `git sparse-checkout list` in `cwd`, which is already a resolved worktree.
pub(crate) fn sparse_list(config: &Config, cwd: &Path) -> AppResult<Option<Vec<String>>> {
    let args = [os("sparse-checkout"), os("list")];
    let out = run_git(config, &args, cwd)?;
    if out.success() {
        return Ok(Some(out.stdout.lines().map(str::to_string).collect()));
    }
    if out.stderr.contains("not sparse") {
        return Ok(None);
    }
    Err(failure(&args, &out))
}

/// `git sparse-checkout disable`: writes the rest of the branch, so the worktree is full again.
/// Nothing is deleted; files that were outside the folders appear.
pub fn show_all_files(config: &Config, extra_roots: &[PathBuf], path: &Path) -> AppResult<PathBuf> {
    let args = [os("sparse-checkout"), os("disable")];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn folders_must_be_plain_top_level_names() {
        let available = names(&["app", "lib", "-x"]);
        assert!(check_folders(&available, &names(&["app", "lib"])).is_ok());
        for bad in [
            &[][..],
            &["nope"],
            &["app/inner"],
            &["../app"],
            &["-x"],
            &["/app"],
        ] {
            let err = check_folders(&available, &names(bad)).expect_err("refused");
            assert_eq!(err.code, ErrorCode::UnknownFolder, "{bad:?}");
        }
    }
}
