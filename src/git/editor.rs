//! Opening a worktree in the editor (Sublime Text CLI).

use super::*;

pub fn open_in_merge(config: &Config, path: &Path) -> AppResult<PathBuf> {
    open_in_merge_with(config, &[], path)
}

/// `extra_roots` are plain clones added by hand in the window.
pub fn open_in_merge_with(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<PathBuf> {
    let (_, wt) = resolve_worktree_for_launch(config, extra_roots, path)?;
    launch_editor(config, &wt.path)?;
    Ok(wt.path)
}

/// Opens a directory inside a worktree, for example a submodule checkout.
pub fn open_child_dir(
    config: &Config,
    extra_roots: &[PathBuf],
    worktree: &Path,
    rel: &str,
) -> AppResult<PathBuf> {
    if !rel_path_ok(rel) {
        return Err(AppError::new(
            ErrorCode::OutsideRoots,
            format!("Refusing path: {rel}"),
        ));
    }
    let (_, wt) = resolve_worktree_for_launch(config, extra_roots, worktree)?;
    let dest = wt.path.join(rel);
    let canonical = std::fs::canonicalize(&dest)
        .map_err(|e| AppError::io(&format!("cannot open {}", dest.display()), &e))?;
    if !canonical.starts_with(&wt.path) || !canonical.is_dir() {
        return Err(AppError::new(
            ErrorCode::OutsideRoots,
            format!("Not a directory inside the worktree: {rel}"),
        ));
    }
    launch_editor(config, &canonical)?;
    Ok(canonical)
}

/// Opens a directory inside a worktree (a submodule) with the chosen editor.
pub fn open_child_dir_with(
    config: &Config,
    extra_roots: &[PathBuf],
    worktree: &Path,
    rel: &str,
    editor: &Editor,
) -> AppResult<PathBuf> {
    if !rel_path_ok(rel) {
        return Err(AppError::new(
            ErrorCode::OutsideRoots,
            format!("Refusing path: {rel}"),
        ));
    }
    let (_, wt) = resolve_worktree_for_launch(config, extra_roots, worktree)?;
    let dest = wt.path.join(rel);
    let canonical = std::fs::canonicalize(&dest)
        .map_err(|e| AppError::io(&format!("cannot open {}", dest.display()), &e))?;
    if !canonical.starts_with(&wt.path) || !canonical.is_dir() {
        return Err(AppError::new(
            ErrorCode::OutsideRoots,
            format!("Not a directory inside the worktree: {rel}"),
        ));
    }
    open_with(config, editor, &canonical)?;
    Ok(canonical)
}

pub(crate) fn launch_editor(config: &Config, path: &Path) -> AppResult<()> {
    ensure_executable(&config.smerge)?;
    let mut cmd = Command::new(&config.smerge);
    let cwd = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    cmd.arg(path).current_dir(cwd);
    let out = run_with_timeout(cmd, SMERGE_TIMEOUT)?;
    if out.success() {
        return Ok(());
    }
    let code = out
        .status
        .map_or_else(|| "signal".to_string(), |c| c.to_string());
    Err(
        AppError::new(ErrorCode::GitFailed, format!("smerge failed (exit {code})"))
            .with_detail(&out.stderr),
    )
}

pub(crate) fn ensure_executable(smerge: &Path) -> AppResult<()> {
    // A bare name (`subl`, `subl.exe`) is looked up on PATH when the process starts.
    let bare = smerge.components().nth(1).is_none();
    if bare || crate::os::file_is_executable(smerge) {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::MergeNotFound,
            format!("Sublime Text CLI not found: {}", smerge.display()),
        ))
    }
}

/// Terminal app used by "Open in Terminal". `HYDRA_MERGE_TERMINAL` picks another.
pub const ENV_TERMINAL: &str = "HYDRA_MERGE_TERMINAL";

fn default_terminal() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Terminal"
    }
    #[cfg(windows)]
    {
        "wt.exe"
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        "x-terminal-emulator"
    }
}

fn spawn(program: &OsStr, args: &[&OsStr], ignore_status: bool) -> AppResult<()> {
    let mut cmd = Command::new(program);
    cmd.args(args).env("PATH", tool_path());
    let out = run_with_timeout(cmd, SMERGE_TIMEOUT)?;
    if ignore_status || out.success() {
        Ok(())
    } else {
        let name = Path::new(program)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.to_string_lossy().into_owned());
        Err(AppError::new(ErrorCode::Io, format!("{name} failed")).with_detail(&out.stderr))
    }
}

/// Opens the worktree in Terminal (or the app named in `HYDRA_MERGE_TERMINAL`).
pub fn open_terminal(config: &Config, extra_roots: &[PathBuf], path: &Path) -> AppResult<PathBuf> {
    let (_, wt) = resolve_worktree_for_launch(config, extra_roots, path)?;
    let app = std::env::var(ENV_TERMINAL)
        .ok()
        .filter(|name| !name.trim().is_empty() && !name.starts_with('-'))
        .unwrap_or_else(|| default_terminal().to_string());
    open_terminal_app(&app, &wt.path)?;
    Ok(wt.path)
}

fn open_terminal_app(app: &str, dir: &Path) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        spawn(os("/usr/bin/open"), &[os("-a"), os(app), dir.as_os_str()], false)
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-Command", WINDOWS_TERMINAL])
            .env("HYDRA_MERGE_CWD", dir)
            .env("HYDRA_MERGE_TERMINAL_APP", app)
            .env("PATH", tool_path());
        let out = run_with_timeout(cmd, SMERGE_TIMEOUT)?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::new(ErrorCode::Io, "terminal failed").with_detail(&out.stderr))
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut cmd = Command::new(app);
        match app {
            "gnome-terminal" | "kgx" | "xfce4-terminal" | "tilix" => {
                cmd.arg(format!("--working-directory={}", dir.display()));
            }
            "konsole" => {
                cmd.arg("--workdir").arg(dir);
            }
            "alacritty" | "kitty" => {
                cmd.arg("--working-directory").arg(dir);
            }
            "wezterm" => {
                cmd.args(["start", "--cwd"]).arg(dir);
            }
            _ => {
                cmd.current_dir(dir);
            }
        }
        cmd.env("PATH", tool_path());
        let out = run_with_timeout(cmd, SMERGE_TIMEOUT)?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::new(ErrorCode::Io, format!("{app} failed")).with_detail(&out.stderr))
        }
    }
}

#[cfg(windows)]
const WINDOWS_TERMINAL: &str = r#"
$dir = $env:HYDRA_MERGE_CWD
$app = $env:HYDRA_MERGE_TERMINAL_APP
if ($app -and (Get-Command $app -ErrorAction SilentlyContinue)) {
  if ($app -eq 'wt.exe' -or $app -eq 'wt') { Start-Process $app -ArgumentList '-d',$dir }
  else { Start-Process $app -WorkingDirectory $dir }
} elseif (Get-Command wt.exe -ErrorAction SilentlyContinue) {
  Start-Process wt.exe -ArgumentList '-d',$dir
} else {
  Start-Process cmd.exe -WorkingDirectory $dir
}
"#;

/// Shows the worktree folder in the file manager.
pub fn reveal_in_finder(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
) -> AppResult<PathBuf> {
    let (_, wt) = resolve_worktree_for_launch(config, extra_roots, path)?;
    reveal_folder(&wt.path)?;
    Ok(wt.path)
}

fn reveal_folder(folder: &Path) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        // `-R` selects the folder. Plain `open` would start a branch folder named `Tool.app`.
        spawn(os("/usr/bin/open"), &[os("-R"), folder.as_os_str()], false)
    }
    #[cfg(windows)]
    {
        // Explorer returns exit code 1 even when the window opens.
        spawn(os("explorer.exe"), &[folder.as_os_str()], true)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        spawn(os("xdg-open"), &[folder.as_os_str()], false)
    }
}

/// Shows a folder of the app (the skins folder) in the file manager.
pub fn open_folder(path: &Path) -> AppResult<()> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(AppError::new(
            ErrorCode::Io,
            format!("Not a folder: {}", path.display()),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        // Open the folder itself. `open -R` would only select it in its parent.
        spawn(os("/usr/bin/open"), &[path.as_os_str()], false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        reveal_folder(path)
    }
}

/// Opens one of the app's own files (a `.skin`) with the chosen editor.
pub fn open_file_in_editor(config: &Config, editor: &Editor, file: &Path) -> AppResult<()> {
    if !file.is_absolute() || !file.is_file() {
        return Err(AppError::new(
            ErrorCode::Io,
            format!("Not a file: {}", file.display()),
        ));
    }
    open_with(config, editor, file)
}

/// An `https://` link with no spaces or control characters, as printed by the server.
pub fn web_link_ok(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() > 8
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Opens a link from server output in the default browser.
pub fn open_link(url: &str) -> AppResult<()> {
    if !web_link_ok(url) {
        return Err(AppError::new(
            ErrorCode::Io,
            format!("Not a web link: {url}"),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        spawn(os("/usr/bin/open"), &[os(url)], false)
    }
    #[cfg(windows)]
    {
        spawn(
            os("rundll32.exe"),
            &[os("url.dll,FileProtocolHandler"), os(url)],
            false,
        )
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        spawn(os("xdg-open"), &[os(url)], false)
    }
}

/// The user may take a while to pick a folder.
const CHOOSER_TIMEOUT: Duration = Duration::from_secs(600);

/// The system folder picker, starting in `start`. `Ok(None)` when the user cancels.
pub fn choose_folder(start: &Path) -> AppResult<Option<PathBuf>> {
    if !start.is_absolute() {
        return Err(AppError::new(
            ErrorCode::Io,
            "The folder picker needs an absolute start folder",
        ));
    }
    #[cfg(target_os = "macos")]
    {
        choose_folder_macos(start)
    }
    #[cfg(windows)]
    {
        choose_with_powershell(WINDOWS_FOLDER, Some(start), "The folder picker failed")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        choose_folder_linux(start)
    }
}

#[cfg(target_os = "macos")]
fn choose_folder_macos(start: &Path) -> AppResult<Option<PathBuf>> {
    // A fixed script: the start folder comes in as an argument, never as script text.
    const CHOOSE_FOLDER: [&str; 5] = [
        "on run argv",
        "set startDir to POSIX file (item 1 of argv)",
        "set picked to choose folder with prompt \"Open Repository\" default location startDir",
        "return POSIX path of picked",
        "end run",
    ];
    let mut cmd = Command::new("/usr/bin/osascript");
    for line in CHOOSE_FOLDER {
        cmd.arg("-e").arg(line);
    }
    cmd.arg(start);
    finish_picker(cmd, "(-128)", "The folder picker failed")
}

#[cfg(not(any(target_os = "macos", windows)))]
fn choose_folder_linux(start: &Path) -> AppResult<Option<PathBuf>> {
    let mut zenity = Command::new("zenity");
    zenity
        .args(["--file-selection", "--directory", "--title=Open Repository"])
        .arg(format!("--filename={}/", start.display()));
    match run_with_timeout(zenity, CHOOSER_TIMEOUT) {
        Ok(out) if out.success() => return Ok(picked_folder(&out.stdout)),
        Ok(out) if out.status == Some(1) => return Ok(None),
        Ok(out) => {
            return Err(
                AppError::new(ErrorCode::Io, "The folder picker failed").with_detail(&out.stderr)
            );
        }
        Err(err) if err.message.contains("failed to run") => {}
        Err(err) => return Err(err),
    }
    let mut kdialog = Command::new("kdialog");
    kdialog
        .args(["--title", "Open Repository", "--getexistingdirectory"])
        .arg(start);
    match run_with_timeout(kdialog, CHOOSER_TIMEOUT) {
        Ok(out) if out.success() => Ok(picked_folder(&out.stdout)),
        Ok(out) if out.status == Some(1) => Ok(None),
        Ok(out) => {
            Err(AppError::new(ErrorCode::Io, "The folder picker failed").with_detail(&out.stderr))
        }
        Err(err) if err.message.contains("failed to run") => Err(AppError::new(
            ErrorCode::Io,
            "Install zenity or kdialog to pick a folder",
        )),
        Err(err) => Err(err),
    }
}

/// The folder printed by the script, without its trailing newline and slash.
fn picked_folder(stdout: &str) -> Option<PathBuf> {
    let line = stdout.trim_end_matches(['\n', '\r']);
    let line = if line.len() > 1 {
        line.trim_end_matches('/')
    } else {
        line
    };
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// What Open uses: the Sublime Text CLI (`subl`, from `HYDRA_MERGE_SMERGE`) or any macOS app,
/// started with `open -a <app> <folder>` (no command-line tool needed, e.g. for VS Code).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Editor {
    #[default]
    SublimeCli,
    App {
        name: String,
        path: PathBuf,
    },
}

impl Editor {
    pub fn name(&self) -> &str {
        match self {
            Editor::SublimeCli => "Sublime Text",
            Editor::App { name, .. } => name,
        }
    }

    /// An installed application: a macOS `.app`, or an executable file on Windows and Linux.
    pub fn app(path: &Path) -> Option<Editor> {
        let name = path.file_stem()?.to_string_lossy().into_owned();
        app_path_ok(path).then(|| Editor::App {
            name,
            path: path.to_path_buf(),
        })
    }
}

fn app_path_ok(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        path.extension().is_some_and(|ext| ext == "app") && path.is_dir()
    }
    #[cfg(windows)]
    {
        path.is_file()
            && path.extension().is_some_and(|ext| {
                let ext = ext.to_string_lossy().to_ascii_lowercase();
                ext == "exe" || ext == "cmd" || ext == "bat"
            })
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        crate::os::file_is_executable(path)
    }
}

/// Editors and IDEs offered without browsing, when installed.
const KNOWN_APPS: [&str; 13] = [
    "Visual Studio Code",
    "Cursor",
    "Zed",
    "Windsurf",
    "IntelliJ IDEA",
    "IntelliJ IDEA CE",
    "GoLand",
    "PyCharm",
    "PyCharm CE",
    "RustRover",
    "WebStorm",
    "Nova",
    "BBEdit",
];

/// Sublime Text first, then editors found in the usual install locations.
pub fn installed_editors(home: Option<&Path>) -> Vec<Editor> {
    let apps = known_editor_paths(home).into_iter().filter_map(|path| {
        let editor = Editor::app(&path)?;
        Some(editor)
    });
    std::iter::once(Editor::SublimeCli).chain(apps).collect()
}

fn known_editor_paths(home: Option<&Path>) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let dirs: Vec<PathBuf> = std::iter::once(PathBuf::from("/Applications"))
            .chain(home.map(|h| h.join("Applications")))
            .collect();
        KNOWN_APPS
            .iter()
            .filter_map(|name| {
                dirs.iter()
                    .map(|dir| dir.join(format!("{name}.app")))
                    .find(|path| path.is_dir())
            })
            .collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = KNOWN_APPS;
        known_cli_editors(home)
    }
}

/// Command names of editors that ship a CLI, with the label shown in Open With.
#[cfg(not(target_os = "macos"))]
const KNOWN_CLI: &[(&str, &str)] = &[
    ("code", "Visual Studio Code"),
    ("cursor", "Cursor"),
    ("zed", "Zed"),
    ("zeditor", "Zed"),
    ("windsurf", "Windsurf"),
    ("idea", "IntelliJ IDEA"),
    ("goland", "GoLand"),
    ("pycharm", "PyCharm"),
    ("rustrover", "RustRover"),
    ("webstorm", "WebStorm"),
];

#[cfg(not(target_os = "macos"))]
fn known_cli_editors(home: Option<&Path>) -> Vec<PathBuf> {
    #[cfg(windows)]
    let _ = home;
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    #[cfg(windows)]
    {
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Some(base) = std::env::var_os(key) {
                let base = PathBuf::from(base);
                dirs.push(base.join("Microsoft VS Code"));
                dirs.push(base.join("Programs").join("Microsoft VS Code"));
                dirs.push(base.join("Programs").join("cursor"));
                dirs.push(base.join("cursor"));
                dirs.push(base.join("Sublime Text"));
            }
        }
    }
    #[cfg(not(windows))]
    {
        dirs.extend([
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/snap/bin"),
        ]);
        if let Some(home) = home {
            dirs.push(home.join(".local/bin"));
        }
    }
    let mut found = Vec::new();
    let mut seen = Vec::new();
    for (bin, _label) in KNOWN_CLI {
        #[cfg(windows)]
        let names = [format!("{bin}.exe"), format!("{bin}.cmd"), (*bin).to_string()];
        #[cfg(not(windows))]
        let names = [(*bin).to_string()];
        for dir in &dirs {
            for name in &names {
                let path = dir.join(name);
                if Editor::app(&path).is_some() && !seen.contains(&path) {
                    seen.push(path.clone());
                    found.push(path);
                    break;
                }
            }
        }
    }
    found
}

/// Opens `path` (a folder or file of the app) with `editor`.
pub fn open_with(config: &Config, editor: &Editor, path: &Path) -> AppResult<()> {
    match editor {
        Editor::SublimeCli => launch_editor(config, path),
        Editor::App { path: app, .. } => {
            if Editor::app(app).is_none() {
                return Err(AppError::new(
                    ErrorCode::MergeNotFound,
                    format!("Application not found: {}", app.display()),
                ));
            }
            launch_app(app, path)
        }
    }
}

fn launch_app(app: &Path, path: &Path) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        spawn(
            os("/usr/bin/open"),
            &[os("-a"), app.as_os_str(), path.as_os_str()],
            false,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        spawn(app.as_os_str(), &[path.as_os_str()], false)
    }
}

/// Opens a worktree with the chosen editor, after the usual path checks.
pub fn open_worktree_with(
    config: &Config,
    extra_roots: &[PathBuf],
    path: &Path,
    editor: &Editor,
) -> AppResult<PathBuf> {
    let (_, wt) = resolve_worktree_for_launch(config, extra_roots, path)?;
    open_with(config, editor, &wt.path)?;
    Ok(wt.path)
}

/// The system application picker. `Ok(None)` when the user cancels.
pub fn choose_app() -> AppResult<Option<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("/usr/bin/osascript");
        cmd.arg("-e").arg(
            "POSIX path of (choose application with prompt \"Open worktrees with\" as alias)",
        );
        finish_picker(cmd, "(-128)", "The application picker failed")
    }
    #[cfg(windows)]
    {
        choose_with_powershell(WINDOWS_APP, None, "The application picker failed")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut cmd = Command::new("zenity");
        cmd.args([
            "--file-selection",
            "--title=Open worktrees with",
            "--file-filter=Programs | *",
        ]);
        match run_with_timeout(cmd, CHOOSER_TIMEOUT) {
            Ok(out) if out.success() => Ok(picked_folder(&out.stdout)),
            Ok(out) if out.status == Some(1) => Ok(None),
            Ok(out) => Err(AppError::new(ErrorCode::Io, "The application picker failed")
                .with_detail(&out.stderr)),
            Err(err) => Err(err),
        }
    }
}

#[cfg(target_os = "macos")]
fn finish_picker(cmd: Command, cancel_mark: &str, failure: &str) -> AppResult<Option<PathBuf>> {
    let out = run_with_timeout(cmd, CHOOSER_TIMEOUT)?;
    if out.success() {
        return Ok(picked_folder(&out.stdout));
    }
    if out.stderr.contains(cancel_mark) {
        return Ok(None);
    }
    Err(AppError::new(ErrorCode::Io, failure).with_detail(&out.stderr))
}

#[cfg(windows)]
const WINDOWS_FOLDER: &str = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.FolderBrowserDialog
$d.Description = 'Open Repository'
$d.SelectedPath = $env:HYDRA_MERGE_PICK_START
if ($d.ShowDialog() -eq 'OK') { Write-Output $d.SelectedPath }
"#;

#[cfg(windows)]
const WINDOWS_APP: &str = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.OpenFileDialog
$d.Filter = 'Applications (*.exe)|*.exe'
$d.Title = 'Open worktrees with'
if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }
"#;

/// The path is an environment variable, never text inside the script.
#[cfg(windows)]
fn choose_with_powershell(
    script: &str,
    start: Option<&Path>,
    failure: &str,
) -> AppResult<Option<PathBuf>> {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-STA", "-Command", script]);
    if let Some(start) = start {
        cmd.env("HYDRA_MERGE_PICK_START", start);
    }
    let out = run_with_timeout(cmd, CHOOSER_TIMEOUT)?;
    if out.success() {
        return Ok(picked_folder(&out.stdout));
    }
    Err(AppError::new(ErrorCode::Io, failure).with_detail(&out.stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_folder_drops_newline_and_trailing_slash() {
        assert_eq!(
            picked_folder("/Users/me/repos/x/\n"),
            Some(PathBuf::from("/Users/me/repos/x"))
        );
        assert_eq!(picked_folder("/\n"), Some(PathBuf::from("/")));
        assert_eq!(picked_folder("\n"), None);
    }

    #[test]
    fn app_editors_need_an_existing_app_bundle() {
        let tmp = tempfile::tempdir().expect("tmp");
        let bundle = tmp.path().join("Visual Studio Code.app");
        assert_eq!(Editor::app(&bundle), None, "missing");
        std::fs::create_dir_all(&bundle).expect("mkdir");
        #[cfg(target_os = "macos")]
        {
            let editor = Editor::app(&bundle).expect("app");
            assert_eq!(editor.name(), "Visual Studio Code");
            assert_eq!(Editor::app(tmp.path()), None, "not an .app");
        }
        #[cfg(not(target_os = "macos"))]
        assert_eq!(Editor::app(&bundle), None, "a directory is not an executable");
        assert_eq!(Editor::app(Path::new("Code.app")), None, "relative");
        let found = installed_editors(Some(tmp.path()));
        assert_eq!(found[0], Editor::SublimeCli, "Sublime Text always first");
    }

    #[test]
    fn only_plain_https_links_are_opened() {
        assert!(web_link_ok(
            "https://gitlab.example.com/g/r/-/merge_requests/new?a=b"
        ));
        assert!(!web_link_ok("http://plain.example"));
        assert!(!web_link_ok("https://a b"));
        assert!(!web_link_ok("file:///etc/passwd"));
        assert!(!web_link_ok("-a Terminal"));
    }

    #[test]
    fn an_editor_path_must_be_absolute() {
        assert_eq!(Editor::app(Path::new("Code.app")), None);
        assert_eq!(Editor::app(Path::new("code")), None);
    }
}
