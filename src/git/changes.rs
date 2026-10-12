//! Status, history, patches and the allowed writes in one worktree.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    /// Two porcelain letters, for example ` M`, `??`, `R `.
    pub code: String,
    pub path: String,
    /// Source path of a rename or copy.
    pub orig_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFile {
    pub status: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitLine {
    pub full: String,
    pub hash: String,
    pub author: String,
    pub email: String,
    /// `Fri, 2 Oct, 10:52`.
    pub date: String,
    /// Full hashes of the parents; two or more for a merge.
    pub parents: Vec<String>,
    /// Branches and tags pointing at this commit.
    pub refs: Vec<RefLabel>,
    pub subject: String,
    pub body: String,
    pub files: Vec<CommitFile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    /// The branch checked out in this worktree (`HEAD -> ...`).
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeView {
    pub files: Vec<FileChange>,
    /// Recent commits of every branch, newest first (`--date-order`).
    pub commits: Vec<CommitLine>,
    /// `git diff`: changes not staged yet.
    pub unstaged_diff: String,
    /// `git diff --cached`: changes in the index.
    pub staged_diff: String,
    /// `Name <email>` from git config; `None` when it is not set.
    pub author: Option<String>,
}

/// Global git option placed before `add` and `restore`: a file name from `status` is a path, not
/// a pattern. Without it a file called `*` or `:(exclude)x` matches other files too.
const LITERAL_PATHSPECS: &str = "--literal-pathspecs";

pub(crate) const HISTORY_LIMIT: &str = "200";
/// NUL opens every field. Git refuses NUL in a commit message or name, and prints nothing past
/// one in a stored object, so no commit text can start a record or move a field boundary the way
/// printable control bytes could. The text of `--name-status` follows the last field.
const LOG_FORMAT: &str =
    "--pretty=format:%x00%H%x00%h%x00%an%x00%ae%x00%ad%x00%P%x00%D%x00%s%x00%b%x00";

/// Values per commit: the nine of [`LOG_FORMAT`] and the `--name-status` text after them.
const LOG_FIELDS: usize = 10;

/// Status and recent history of one worktree. Does not write to the repository.
pub fn inspect_worktree(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<WorktreeView> {
    let (_, wt) = resolve_worktree_with(config, extra_roots, path)?;
    let status_args = [
        os("status"),
        os("--porcelain=v1"),
        os("-z"),
        os("--untracked-files=all"),
    ];
    let status = run_git_ok(config, &status_args, &wt.path)?;
    let commits = read_log(config, &wt.path)?;
    Ok(WorktreeView {
        files: parse_status_z(&status.stdout),
        commits,
        unstaged_diff: working_diff(config, &wt.path, false),
        staged_diff: working_diff(config, &wt.path, true),
        author: author_line(config, &wt.path),
    })
}

/// History of all branches, remotes and tags.
fn read_log(config: &Config, cwd: &Path) -> AppResult<Vec<CommitLine>> {
    read_log_with(config, cwd, &[])
}

/// The history query of the commit list; `filter` adds options such as `--grep=<text>`.
pub(crate) fn read_log_with(
    config: &Config,
    cwd: &Path,
    filter: &[&OsStr],
) -> AppResult<Vec<CommitLine>> {
    let head = [
        os("log"),
        os("--branches"),
        os("--remotes"),
        os("--tags"),
        os("HEAD"),
        os("--date-order"),
        os("-n"),
        os(HISTORY_LIMIT),
        os("--decorate=full"),
        os("--date=format:%a, %e %b, %H:%M"),
        os("--name-status"),
        os(LOG_FORMAT),
    ];
    let log_args: Vec<&OsStr> = head.iter().copied().chain(filter.iter().copied()).collect();
    let log = run_git(config, &log_args, cwd)?;
    if log.success() {
        Ok(parse_log(&log.stdout))
    } else if log.stderr.contains("does not have any commits yet")
        || log.stderr.contains("unknown revision")
    {
        Ok(Vec::new())
    } else {
        Err(failure(&log_args, &log))
    }
}

/// Staged and unstaged changes against HEAD; empty before the first commit or on failure.
/// `git diff` (unstaged) or `git diff --cached` (staged); empty on failure.
fn working_diff(config: &Config, cwd: &Path, staged: bool) -> String {
    let mut args = vec![
        os("diff"),
        os("--no-color"),
        os("--no-ext-diff"),
        os("--no-textconv"),
    ];
    if staged {
        args.push(os("--cached"));
    }
    match run_git(config, &args, cwd) {
        Ok(out) if out.success() => cap_patch(out.stdout, PATCH_CAP),
        _ => String::new(),
    }
}

/// `Name <email>` from git config, as the Summary pane shows the author.
fn author_line(config: &Config, cwd: &Path) -> Option<String> {
    let read = |key: &str| {
        let args = [os("config"), os("--get"), os(key)];
        run_git(config, &args, cwd)
            .ok()
            .filter(|out| out.success())
            .map(|out| out.stdout.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let name = read("user.name")?;
    Some(match read("user.email") {
        Some(email) => format!("{name} <{email}>"),
        None => name,
    })
}

pub fn parse_status_z(stdout: &str) -> Vec<FileChange> {
    let mut parts = stdout.split('\0').filter(|part| !part.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = parts.next() {
        let (Some(code), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        let moved = code.contains('R') || code.contains('C');
        let orig_path = if moved {
            parts.next().map(str::to_string)
        } else {
            None
        };
        if !path.is_empty() {
            files.push(FileChange {
                code: code.to_string(),
                path: path.to_string(),
                orig_path,
            });
        }
    }
    files
}

pub fn parse_log(stdout: &str) -> Vec<CommitLine> {
    let mut parts = stdout.split('\0');
    parts.next(); // before the first NUL: nothing
    let fields: Vec<&str> = parts.collect();
    // A trailing group with fewer than LOG_FIELDS values is dropped.
    let (records, _) = fields.as_chunks::<LOG_FIELDS>();
    records.iter().filter_map(parse_log_record).collect()
}

/// A full object id: 40 (SHA-1) or 64 (SHA-256) hex digits.
fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_log_record(fields: &[&str; LOG_FIELDS]) -> Option<CommitLine> {
    let [full, hash, author, email, date, parents, refs, subject, body, files] = *fields;
    let parents: Vec<&str> = parents.split_whitespace().collect();
    let ids_ok = is_object_id(full)
        && !hash.is_empty()
        && full.starts_with(hash)
        && parents.iter().all(|parent| is_object_id(parent));
    if !ids_ok {
        return None;
    }
    Some(CommitLine {
        full: full.to_string(),
        hash: hash.to_string(),
        author: author.to_string(),
        email: email.to_string(),
        date: date.split_whitespace().collect::<Vec<_>>().join(" "),
        parents: parents.into_iter().map(str::to_string).collect(),
        refs: parse_decorations(refs),
        subject: subject.to_string(),
        body: body.trim().to_string(),
        files: parse_name_status(files),
    })
}

/// `%D` with `--decorate=full`: `HEAD -> refs/heads/x, refs/remotes/o/x, tag: refs/tags/v1`.
/// `origin/HEAD` and a detached `HEAD` are left out.
pub fn parse_decorations(text: &str) -> Vec<RefLabel> {
    text.split(", ")
        .filter_map(|item| {
            let item = item.trim();
            let (head, item) = match item.strip_prefix("HEAD -> ") {
                Some(rest) => (true, rest),
                None => (false, item),
            };
            let label = |name: &str, kind| {
                Some(RefLabel {
                    name: name.to_string(),
                    kind,
                })
            };
            if let Some(name) = item.strip_prefix("tag: refs/tags/") {
                label(name, RefKind::Tag)
            } else if let Some(name) = item.strip_prefix(REFS_HEADS) {
                label(name, if head { RefKind::Head } else { RefKind::Local })
            } else if let Some(name) = item.strip_prefix("refs/remotes/") {
                (!name.ends_with("/HEAD")).then(|| RefLabel {
                    name: name.to_string(),
                    kind: RefKind::Remote,
                })
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn parse_name_status(blob: &str) -> Vec<CommitFile> {
    blob.lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.to_string();
            let path = match parts.next() {
                Some(first) => parts.next().unwrap_or(first).to_string(),
                None => return None,
            };
            Some(CommitFile { status, path })
        })
        .collect()
}

/// Patch of one commit. `hash` must be hex; the repository is not modified.
pub fn commit_patch(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    hash: &str,
) -> AppResult<String> {
    if hash.len() < 7 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::new(
            ErrorCode::GitFailed,
            format!("Not a commit id: {hash}"),
        ));
    }
    let (_, wt) = resolve_worktree_with(config, extra_roots, path)?;
    let args = [
        os("show"),
        os("-p"),
        os("--format="),
        os("--no-color"),
        os("--no-ext-diff"),
        os("--no-textconv"),
        os(hash),
    ];
    let out = run_git_ok(config, &args, &wt.path)?;
    Ok(cap_patch(out.stdout, PATCH_CAP))
}

/// Upper bound for the patch text shown in the window.
pub(crate) const PATCH_CAP: usize = 200_000;

/// Cuts at the last whole line before `cap` bytes, never inside a UTF-8 character.
pub fn cap_patch(text: String, cap: usize) -> String {
    if text.len() <= cap {
        return text;
    }
    let boundary = (0..=cap)
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(0);
    let cut = text[..boundary].rfind('\n').map_or(boundary, |at| at + 1);
    format!("{}{TRUNCATED_MARKER}\n", &text[..cut])
}

pub(crate) fn worktree_dir(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<PathBuf> {
    let (_, wt) = resolve_worktree_with(config, extra_roots, path)?;
    Ok(wt.path)
}

/// Rejects absolute paths, `..`, and arguments that git would treat as flags.
pub fn rel_path_ok(path: &str) -> bool {
    if path.is_empty() || path.starts_with('-') || path.contains('\0') {
        return false;
    }
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| !matches!(component, std::path::Component::ParentDir))
}

pub(crate) fn bad_path(path: &str) -> AppError {
    AppError::new(
        ErrorCode::OutsideRoots,
        format!("Refusing path outside the worktree: {path}"),
    )
}

pub(crate) fn run_in_worktree(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    args: &[&OsStr],
    timeout: Duration,
) -> AppResult<PathBuf> {
    let cwd = worktree_dir(config, extra_roots, path)?;
    let out = run_git_timeout(config, args, &cwd, timeout)?;
    if out.success() {
        Ok(cwd)
    } else {
        Err(failure(args, &out))
    }
}

/// `git add -- <path>` or `git add --all` when `rel` is `None`.
pub fn stage_file(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    rel: Option<&str>,
) -> AppResult<PathBuf> {
    let mut owned: Vec<OsString> = Vec::new();
    match rel {
        None => owned.extend([OsString::from("add"), OsString::from("--all")]),
        Some(rel) => {
            if !rel_path_ok(rel) {
                return Err(bad_path(rel));
            }
            owned.extend([
                OsString::from(LITERAL_PATHSPECS),
                OsString::from("add"),
                OsString::from("--"),
                OsString::from(rel),
            ]);
        }
    }
    let args: Vec<&OsStr> = owned.iter().map(OsString::as_os_str).collect();
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

/// `git restore --staged -- <path>`.
pub fn unstage_file(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    rel: &str,
) -> AppResult<PathBuf> {
    unstage_paths(config, extra_roots, path, &[rel])
}

/// `git restore --staged -- <paths>`; a rename passes both its new and its old path.
pub fn unstage_paths(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    rels: &[&str],
) -> AppResult<PathBuf> {
    if let Some(bad) = rels.iter().find(|rel| !rel_path_ok(rel)) {
        return Err(bad_path(bad));
    }
    if rels.is_empty() {
        return Err(bad_path(""));
    }
    let head = [
        os(LITERAL_PATHSPECS),
        os("restore"),
        os("--staged"),
        os("--"),
    ];
    let args: Vec<&OsStr> = head
        .into_iter()
        .chain(rels.iter().map(|rel| os(rel)))
        .collect();
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

/// Restores the worktree copy of a tracked file. Untracked files are refused.
pub fn discard_file(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    rel: &str,
) -> AppResult<PathBuf> {
    discard_paths(config, extra_roots, path, &[rel])
}

/// `git restore --worktree --source=HEAD -- <paths>`: tracked files only, after confirmation.
pub fn discard_paths(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    rels: &[&str],
) -> AppResult<PathBuf> {
    let head = [
        os(LITERAL_PATHSPECS),
        os("restore"),
        os("--worktree"),
        os("--source=HEAD"),
        os("--"),
    ];
    run_on_paths(config, extra_roots, path, &head, rels)
}

/// `git add -- <paths>`; a folder path stages everything under it.
pub fn stage_paths(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    rels: &[&str],
) -> AppResult<PathBuf> {
    let head = [os(LITERAL_PATHSPECS), os("add"), os("--")];
    run_on_paths(config, extra_roots, path, &head, rels)
}

/// `head` followed by `rels`, each checked to stay inside the worktree.
fn run_on_paths(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    head: &[&OsStr],
    rels: &[&str],
) -> AppResult<PathBuf> {
    if let Some(bad) = rels.iter().find(|rel| !rel_path_ok(rel)) {
        return Err(bad_path(bad));
    }
    if rels.is_empty() {
        return Err(bad_path(""));
    }
    let args: Vec<&OsStr> = head
        .iter()
        .copied()
        .chain(rels.iter().map(|rel| os(rel)))
        .collect();
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

pub fn commit_worktree(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    message: &str,
) -> AppResult<PathBuf> {
    if message.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::GitFailed,
            "Commit message is empty",
        ));
    }
    let args = [os("commit"), os("-m"), os(message)];
    run_in_worktree(config, extra_roots, path, &args, GIT_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_patch_keeps_utf8_and_whole_lines() {
        let text = "ab\nпривет\n".to_string();
        assert_eq!(cap_patch(text.clone(), 100), text);
        // Byte 5 is inside "п": the cut falls back to the end of the first line.
        assert_eq!(cap_patch(text.clone(), 5), "ab\n… truncated\n");
        assert_eq!(cap_patch("приветик".to_string(), 3), "п… truncated\n");
    }

    #[test]
    fn rel_path_rejects_escape() {
        assert!(rel_path_ok("note.txt"));
        assert!(rel_path_ok("dir/note.txt"));
        assert!(!rel_path_ok(""));
        assert!(!rel_path_ok("../note.txt"));
        assert!(!rel_path_ok("dir/../../x"));
        assert!(!rel_path_ok("/abs"));
        assert!(!rel_path_ok("-n"));
    }

    #[test]
    fn parse_status_and_log() {
        // Same order as `git status -z` prints a staged rename: new path first, then the old one.
        let files = parse_status_z(" M README\0?? note.txt\0R  new.txt\0old.txt\0 M  lead.txt\0");
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].code, " M");
        assert_eq!(files[0].path, "README");
        assert_eq!(files[1].path, "note.txt");
        assert_eq!(files[2].code, "R ");
        assert_eq!(files[2].path, "new.txt");
        assert_eq!(files[2].orig_path.as_deref(), Some("old.txt"));
        assert_eq!(
            files[3].path, " lead.txt",
            "leading spaces belong to the name"
        );
        let full = "a".repeat(40);
        let (p1, p2) = ("b".repeat(40), "c".repeat(40));
        let log = parse_log(&format!(
            "\0{full}\0aaaaaaa\0Ada\0ada@ex\0Fri,  2 Oct, 10:52\0{p1} {p2}\0\
             HEAD -> refs/heads/master, refs/remotes/origin/master, tag: refs/tags/v1, refs/heads/mr/1\
             \0init\0the body\0\nM\tREADME\n"
        ));
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].full, full);
        assert_eq!(log[0].hash, "aaaaaaa");
        assert_eq!(log[0].author, "Ada");
        assert_eq!(log[0].email, "ada@ex");
        assert_eq!(
            log[0].date, "Fri, 2 Oct, 10:52",
            "double spaces from %e are folded"
        );
        assert_eq!(log[0].parents, vec![p1, p2]);
        let refs: Vec<(&str, RefKind)> = log[0]
            .refs
            .iter()
            .map(|r| (r.name.as_str(), r.kind))
            .collect();
        assert_eq!(
            refs,
            vec![
                ("master", RefKind::Head),
                ("origin/master", RefKind::Remote),
                ("v1", RefKind::Tag),
                ("mr/1", RefKind::Local),
            ]
        );
        assert_eq!(log[0].subject, "init");
        assert_eq!(log[0].body, "the body");
        assert_eq!(log[0].files.len(), 1);
        assert_eq!(log[0].files[0].status, "M");
        assert_eq!(log[0].files[0].path, "README");
    }

    fn record(tokens: [&str; LOG_FIELDS]) -> String {
        format!("\0{}", tokens.join("\0"))
    }

    #[test]
    fn parse_log_drops_records_whose_ids_are_not_object_ids() {
        let real = "a".repeat(40);
        let text = [
            record([
                "FAKE\necho x",
                "deadbee",
                "Eve",
                "e@x",
                "d",
                "",
                "",
                "s",
                "b",
                "",
            ]),
            record([
                &real, "aaaaaaa", "Ada", "a@x", "d", "", "", "init", "", "\nM\tf\n",
            ]),
            record([
                &real,
                "bbbbbbb",
                "Eve",
                "e@x",
                "d",
                "",
                "",
                "prefix mismatch",
                "",
                "",
            ]),
            record([
                &real,
                "aaaaaaa",
                "Eve",
                "e@x",
                "d",
                "nothex",
                "",
                "bad parent",
                "",
                "",
            ]),
        ]
        .concat();
        let log = parse_log(&text);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].author, "Ada");
        assert_eq!(log[0].files.len(), 1);
    }

    #[test]
    fn parse_log_ignores_an_incomplete_trailing_record() {
        let real = "a".repeat(40);
        let mut text = record([&real, "aaaaaaa", "Ada", "a@x", "d", "", "", "init", "", ""]);
        text.push_str("\0half\0a record");
        assert_eq!(parse_log(&text).len(), 1);
        assert!(parse_log("").is_empty());
    }
}
