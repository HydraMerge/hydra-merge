//! Process execution: the only place git and the editor CLI are started, argv only.

use super::*;

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to wait for pipe readers after the child exited (a detached grandchild may keep a pipe open).
pub(crate) const PIPE_GRACE: Duration = Duration::from_secs(2);

pub struct GitOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl GitOutput {
    pub(crate) fn success(&self) -> bool {
        self.status == Some(0)
    }
}

// ---------------------------------------------------------------------------
// Process execution
// ---------------------------------------------------------------------------

/// Text used in messages: `git <subcommand>` for git, the file name for anything else.
pub(crate) fn command_label(cmd: &Command) -> String {
    let program = cmd.get_program();
    if program != "git" {
        return Path::new(program)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.to_string_lossy().into_owned());
    }
    let args: Vec<&OsStr> = cmd.get_args().collect();
    format!("git {}", subcommand(&args))
}

pub(crate) fn subcommand(args: &[&OsStr]) -> String {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if *arg == "-C" {
            iter.next();
        } else if !arg.to_string_lossy().starts_with('-') {
            return arg.to_string_lossy().into_owned();
        }
    }
    String::new()
}

pub(crate) fn drain<R: Read + Send + 'static>(reader: Option<R>) -> Receiver<Vec<u8>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = reader {
            // A read error keeps whatever was read so far; the exit status still reports failure.
            let _ = r.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

pub(crate) fn collect(rx: &Receiver<Vec<u8>>) -> String {
    let bytes = rx.recv_timeout(PIPE_GRACE).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Shared by git and smerge. Pipes are drained on two threads; the parent polls `try_wait`.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> AppResult<GitOutput> {
    let label = command_label(&cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::io(&format!("failed to run {}", label_program(&label)), &e))?;
    let out_rx = drain(child.stdout.take());
    let err_rx = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                // Best effort: the process may have exited between the poll and the kill.
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::new(
                    ErrorCode::GitTimeout,
                    format!("{label} timed out"),
                ));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(e) => return Err(AppError::io(&format!("failed to wait for {label}"), &e)),
        }
    };
    Ok(GitOutput {
        status: status.code(),
        stdout: collect(&out_rx),
        stderr: collect(&err_rx),
    })
}

pub(crate) fn label_program(label: &str) -> &str {
    label.split(' ').next().unwrap_or(label)
}

/// The only place git is started.
pub fn run_git(config: &Config, args: &[&OsStr], cwd: &Path) -> AppResult<GitOutput> {
    run_git_timeout(config, args, cwd, GIT_TIMEOUT)
}

pub(crate) fn run_git_timeout(
    config: &Config,
    args: &[&OsStr],
    cwd: &Path,
    timeout: Duration,
) -> AppResult<GitOutput> {
    let inherited = std::env::var("GIT_CONFIG_COUNT")
        .ok()
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("PATH", tool_path())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .envs(neutralising_env(inherited))
        .envs(config.git_env.iter().map(|(k, v)| (k, v)));
    run_with_timeout(cmd, timeout)
}

/// Repository config that makes a plain read run a program: `status` asks `core.fsmonitor`, and
/// `log`, `show` and `stash list` call `gpg.program` when `log.showSignature` is on. A repository
/// that arrived as a whole folder brings its own `config`, so the app switches both off.
const READ_ONLY_CONFIG: [(&str, &str); 2] =
    [("core.fsmonitor", "false"), ("log.showSignature", "false")];

/// `GIT_CONFIG_COUNT/KEY/VALUE` for [`READ_ONLY_CONFIG`], numbered after the `inherited` entries
/// of the user's own environment. This scope outranks the repository's config, and the variables
/// also reach the processes git starts itself (for example the ones of `submodule status`).
pub(crate) fn neutralising_env(inherited: usize) -> Vec<(String, String)> {
    let mut vars = vec![(
        "GIT_CONFIG_COUNT".to_string(),
        (inherited + READ_ONLY_CONFIG.len()).to_string(),
    )];
    for (offset, (key, value)) in READ_ONLY_CONFIG.iter().enumerate() {
        let index = inherited + offset;
        vars.push((format!("GIT_CONFIG_KEY_{index}"), (*key).to_string()));
        vars.push((format!("GIT_CONFIG_VALUE_{index}"), (*value).to_string()));
    }
    vars
}

/// Directories where Homebrew and other installers put tools that hooks call (node for husky,
/// git-lfs, gpg). An app started from Finder gets only `/usr/bin:/bin:/usr/sbin:/sbin`.
#[cfg(target_os = "macos")]
const TOOL_DIRS: [&str; 4] = [
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];

/// `current` with `extra` directories appended when missing; order kept, no duplicates.
pub fn augment_path(current: &str, extra: &[String]) -> String {
    let sep = crate::os::list_sep();
    let mut dirs: Vec<&str> = Vec::new();
    for dir in current.split(sep).chain(extra.iter().map(String::as_str)) {
        if !dir.is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs.into_iter().collect::<Vec<_>>().join(&sep.to_string())
}

/// Directories installers use for git, hooks and helpers, added when the app is started
/// from a file manager (its PATH is then only the system directories).
fn system_tool_dirs() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let mut files = vec![PathBuf::from("/etc/paths")];
        if let Ok(entries) = std::fs::read_dir("/etc/paths.d") {
            let mut more: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            more.sort();
            files.extend(more);
        }
        let listed = files
            .iter()
            .filter_map(|file| std::fs::read_to_string(file).ok())
            .flat_map(|text| {
                text.lines()
                    .map(|l| l.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|line| line.starts_with('/'));
        TOOL_DIRS
            .iter()
            .map(|d| d.to_string())
            .chain(listed)
            .collect()
    }
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            let Some(base) = std::env::var_os(key) else {
                continue;
            };
            let base = PathBuf::from(base);
            dirs.push(base.join("Git").join("cmd").display().to_string());
            if key == "LocalAppData" {
                dirs.push(
                    base.join("Programs")
                        .join("Git")
                        .join("cmd")
                        .display()
                        .to_string(),
                );
            }
        }
        dirs
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut dirs = vec![
            "/usr/local/bin".to_string(),
            "/usr/local/sbin".to_string(),
            "/home/linuxbrew/.linuxbrew/bin".to_string(),
        ];
        if let Some(home) = crate::os::home_dir() {
            dirs.push(home.join(".local/bin").display().to_string());
        }
        dirs
    }
}

/// PATH for every child process: the inherited one plus the tool directories, computed once.
pub fn tool_path() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let current = std::env::var("PATH").unwrap_or_default();
        augment_path(&current, &system_tool_dirs())
    })
}

pub(crate) fn failure(args: &[&OsStr], out: &GitOutput) -> AppError {
    let code = out
        .status
        .map_or_else(|| "signal".to_string(), |c| c.to_string());
    AppError::new(
        ErrorCode::GitFailed,
        format!("git {} failed (exit {code})", subcommand(args)),
    )
    .with_detail(&out.stderr)
}

/// Runs git and turns a non-zero exit into `GitFailed`.
pub(crate) fn run_git_ok(config: &Config, args: &[&OsStr], cwd: &Path) -> AppResult<GitOutput> {
    let out = run_git(config, args, cwd)?;
    if out.success() {
        Ok(out)
    } else {
        Err(failure(args, &out))
    }
}

pub(crate) fn git_dir_arg(git_dir: &Path) -> OsString {
    let mut arg = OsString::from("--git-dir=");
    arg.push(git_dir);
    arg
}

pub(crate) fn os(s: &str) -> &OsStr {
    OsStr::new(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_gets_tool_dirs_once() {
        let sep = crate::os::list_sep();
        let extra = vec!["/opt/homebrew/bin".to_string(), "/usr/bin".to_string()];
        let current = format!("/usr/bin{sep}/bin");
        assert_eq!(
            augment_path(&current, &extra),
            format!("/usr/bin{sep}/bin{sep}/opt/homebrew/bin")
        );
        assert_eq!(
            augment_path("", &extra),
            format!("/opt/homebrew/bin{sep}/usr/bin")
        );
        #[cfg(unix)]
        assert!(tool_path().split(sep).any(|d| d == "/usr/local/bin"));
    }

    #[test]
    fn run_with_timeout_kills() {
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let started = Instant::now();
        let err = run_with_timeout(cmd, Duration::from_millis(100)).err();
        assert_eq!(err.map(|e| e.code), Some(ErrorCode::GitTimeout));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn run_with_timeout_captures_output() {
        let mut cmd = Command::new("echo");
        cmd.arg("hi");
        let out = run_with_timeout(cmd, Duration::from_secs(5)).expect("echo runs");
        assert_eq!(out.status, Some(0));
        assert_eq!(out.stdout.trim(), "hi");
    }

    #[test]
    fn neutralising_env_follows_the_users_own_config_entries() {
        let vars = neutralising_env(2);
        let get = |name: &str| {
            vars.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(get("GIT_CONFIG_COUNT"), Some("4"));
        assert_eq!(get("GIT_CONFIG_KEY_2"), Some("core.fsmonitor"));
        assert_eq!(get("GIT_CONFIG_VALUE_2"), Some("false"));
        assert_eq!(get("GIT_CONFIG_KEY_3"), Some("log.showSignature"));
        assert_eq!(
            get("GIT_CONFIG_KEY_0"),
            None,
            "entries 0 and 1 stay the user's"
        );
    }

    #[test]
    fn subcommand_skips_options_and_c_value() {
        let args = [
            os("--git-dir=/x"),
            os("-C"),
            os("/p"),
            os("status"),
            os("--porcelain"),
        ];
        assert_eq!(subcommand(&args), "status");
    }
}
