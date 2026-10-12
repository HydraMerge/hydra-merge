//! Fetch, pull and push: network timeouts, sign-in errors explained, and links from the server
//! (GitLab prints a merge-request URL after a push) kept for the window.

use super::*;

/// Shown when git could not sign in; the window offers "Open in Terminal" next to it.
const AUTH_HINT: &str =
    "git could not sign in without a prompt. Open the worktree in Terminal and \
run the same command once (or set up a credential helper / SSH key); later runs here reuse it.";

/// What git prints when it would have to ask for a password or key.
const AUTH_MARKERS: [&str; 8] = [
    "could not read Username",
    "could not read Password",
    "terminal prompts disabled",
    "Authentication failed",
    "Permission denied (publickey",
    "Host key verification failed",
    "HTTP Basic: Access denied",
    "The requested URL returned error: 403",
];

/// A finished push: the worktree and the first link the server printed, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushDone {
    pub path: PathBuf,
    pub link: Option<String>,
}

/// `true` when `stderr` shows that git needed to sign in.
pub fn needs_sign_in(stderr: &str) -> bool {
    AUTH_MARKERS.iter().any(|marker| stderr.contains(marker))
}

/// `http(s)://` links in server output, trailing punctuation removed, in order, without repeats.
pub fn find_urls(text: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        // Only https: a plain http link could not be opened anyway.
        let Some(start) = word.find("https://") else {
            continue;
        };
        let url = word[start..].trim_end_matches(['.', ',', ')', '>', '\'', '"', ';']);
        if url.len() > "https://".len() && !urls.iter().any(|known| known == url) {
            urls.push(url.to_string());
        }
    }
    urls
}

/// The link worth a button: a merge / pull request page first, otherwise the first link.
pub fn review_link(text: &str) -> Option<String> {
    let urls = find_urls(text);
    let review = urls
        .iter()
        .find(|url| url.contains("merge_request") || url.contains("/pull"))
        .cloned();
    review.or_else(|| urls.into_iter().next())
}

/// A failed network command: a sign-in problem gets its own code and a hint.
fn network_failure(args: &[&OsStr], out: &GitOutput) -> AppError {
    if !needs_sign_in(&out.stderr) {
        return failure(args, out);
    }
    let detail = format!("{AUTH_HINT}\n\n{}", out.stderr.trim());
    AppError::new(
        ErrorCode::AuthRequired,
        format!("git {}: sign-in required", subcommand(args)),
    )
    .with_detail(&detail)
}

/// Runs a network command in a worktree; returns its directory and stdout + stderr.
fn run_network(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    args: &[&OsStr],
) -> AppResult<(PathBuf, String)> {
    let cwd = worktree_dir(config, extra_roots, path)?;
    let out = run_git_timeout(config, args, &cwd, NETWORK_TIMEOUT)?;
    if out.success() {
        Ok((cwd, format!("{}\n{}", out.stdout, out.stderr)))
    } else {
        Err(network_failure(args, &out))
    }
}

pub fn push_worktree(config: &Config, extra_roots: &[PathBuf], path: &Path) -> AppResult<PushDone> {
    let (path, output) = run_network(config, extra_roots, path, &[os("push")])?;
    Ok(PushDone {
        path,
        link: review_link(&output),
    })
}

pub fn pull_worktree(config: &Config, extra_roots: &[PathBuf], path: &Path) -> AppResult<PathBuf> {
    let args = [os("pull"), os("--no-edit")];
    run_network(config, extra_roots, path, &args).map(|(path, _)| path)
}

pub fn fetch_worktree(config: &Config, extra_roots: &[PathBuf], path: &Path) -> AppResult<PathBuf> {
    run_network(config, extra_roots, path, &[os("fetch")]).map(|(path, _)| path)
}

/// `git fetch --all` of a whole repository, for the periodic background fetch. A repository whose
/// own config or hooks can run programs is refused until it is trusted (`resolve_repo`).
pub fn fetch_repo(config: &Config, repo_root: &Path) -> AppResult<PathBuf> {
    let repo = resolve_repo(config, repo_root)?;
    let dir_arg = git_dir_arg(&repo.git_dir);
    let args = [dir_arg.as_os_str(), os("fetch"), os("--all")];
    let out = run_git_timeout(config, &args, &repo.root, NETWORK_TIMEOUT)?;
    if out.success() {
        Ok(repo.root)
    } else {
        Err(network_failure(&args, &out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITLAB: &str = "remote:\nremote: To create a merge request for feat/x, visit:\nremote:   \
https://gitlab.example.com/grp/repo/-/merge_requests/new?merge_request%5Bsource_branch%5D=feat%2Fx\n\
remote:\nTo gitlab.example.com:grp/repo.git\n * [new branch]      feat/x -> feat/x\n";

    #[test]
    fn finds_the_merge_request_link_in_push_output() {
        let link = review_link(GITLAB).expect("link");
        assert!(link.starts_with("https://gitlab.example.com/grp/repo/-/merge_requests/new?"));
        assert_eq!(review_link("Everything up-to-date\n"), None);
        let github = "remote: Create a pull request for 'x' on GitHub by visiting:\nremote:      https://github.com/o/r/pull/new/x\n";
        assert_eq!(
            review_link(github).as_deref(),
            Some("https://github.com/o/r/pull/new/x")
        );
    }

    #[test]
    fn urls_lose_trailing_punctuation_and_repeats() {
        let urls =
            find_urls("see (https://a.example/x). and https://a.example/x, https://b.example");
        assert_eq!(urls, vec!["https://a.example/x", "https://b.example"]);
        assert!(find_urls("https:// nothing").is_empty());
    }

    #[test]
    fn sign_in_errors_are_recognised() {
        assert!(needs_sign_in("fatal: could not read Username for 'https://gitlab.example.com': terminal prompts disabled"));
        assert!(needs_sign_in("git@host: Permission denied (publickey).\nfatal: Could not read from remote repository."));
        assert!(!needs_sign_in(
            "fatal: refusing to merge unrelated histories"
        ));
    }
}
