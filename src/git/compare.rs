//! Read-only views across history: the diff between two refs and a search of commit messages.

use super::*;

/// A ref typed or picked by the user: a branch, `remote/branch`, tag or hash. Options, ranges,
/// spaces and control characters are refused before git sees it.
pub fn ref_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('-')
        && !name.contains("..")
        && !name.contains([':', '\\', '?', '*', '['])
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn bad_ref(name: &str) -> AppError {
    AppError::new(
        ErrorCode::BranchNotFound,
        format!("Not a branch or commit: {name}"),
    )
}

/// The ref must name a commit in this repository.
fn verify_commit(config: &Config, repo: &Repo, name: &str) -> AppResult<()> {
    if !ref_name_ok(name) {
        return Err(bad_ref(name));
    }
    let spec = format!("{name}^{{commit}}");
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("rev-parse"),
        os("--verify"),
        os("--quiet"),
        os(&spec),
    ];
    let out = run_git(config, &args, &repo.root)?;
    if out.success() {
        Ok(())
    } else {
        Err(bad_ref(name))
    }
}

/// `git diff base...target`: what `target` changed since it left `base`, as a merge request shows.
pub fn compare_refs(
    config: &Config,
    repo_root: &Path,
    base: &str,
    target: &str,
) -> AppResult<String> {
    let repo = resolve_repo(config, repo_root)?;
    verify_commit(config, &repo, base)?;
    verify_commit(config, &repo, target)?;
    let range = format!("{base}...{target}");
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [
        dir_arg.as_os_str(),
        os("diff"),
        os("--no-color"),
        os("--no-ext-diff"),
        os("--no-textconv"),
        os(&range),
    ];
    let out = run_git_ok(config, &args, &repo.root)?;
    Ok(cap_patch(out.stdout, PATCH_CAP))
}

/// Commits of all branches whose message contains `text` (any case), newest first.
pub fn search_log(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    text: &str,
) -> AppResult<Vec<CommitLine>> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(char::is_control) {
        return Ok(Vec::new());
    }
    let (_, wt) = resolve_worktree_with(config, extra_roots, path)?;
    // One argv item, so the text can never become an option.
    let grep = format!("--grep={text}");
    let filter = [os("--regexp-ignore-case"), os("--fixed-strings"), os(&grep)];
    read_log_with(config, &wt.path, &filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_names_from_the_user_are_checked() {
        for good in ["master", "origin/feature/x", "v1.0", "abc1234", "mr/162"] {
            assert!(ref_name_ok(good), "{good}");
        }
        for bad in ["", "-p", "a..b", "a b", "a:b", "x\ny", "*"] {
            assert!(!ref_name_ok(bad), "{bad:?}");
        }
    }
}
