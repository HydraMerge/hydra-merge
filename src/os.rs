//! Operating-system details: home and settings directories, shortcuts, and how a file is judged
//! runnable. Launching apps stays in `git::editor`.

use std::path::{Path, PathBuf};

pub const APP_DIRNAME: &str = "hydra-merge";

/// `$HOME` on Unix, `%USERPROFILE%` (then `%HOMEDRIVE%%HOMEPATH%`) on Windows.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let rest = std::env::var_os("HOMEPATH")?;
                let mut path = PathBuf::from(drive);
                path.push(rest);
                (!path.as_os_str().is_empty()).then_some(path)
            })
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
    }
}

/// Where theme, session and skins are stored.
pub fn config_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library")
            .join("Application Support")
            .join(APP_DIRNAME)
    }
    #[cfg(windows)]
    {
        home.join("AppData").join("Roaming").join(APP_DIRNAME)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        home.join(".config").join(APP_DIRNAME)
    }
}

/// `PATH` and `HYDRA_MERGE_ROOTS` use `:` on Unix and `;` on Windows (`C:\` contains `:`).
pub fn list_sep() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

/// A regular file git or the OS will run.
pub fn file_is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        // Git for Windows runs a hook when the file exists. The executable bit is not stored
        // on NTFS the way Unix `mode` stores it.
        let _ = meta;
        true
    }
}

pub fn file_manager_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Finder"
    }
    #[cfg(windows)]
    {
        "Explorer"
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        "Files"
    }
}

pub fn reveal_in_file_manager() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Reveal in Finder"
    }
    #[cfg(windows)]
    {
        "Reveal in Explorer"
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        "Show in Files"
    }
}

pub fn shortcut_palette() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘⇧P"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+Shift+P"
    }
}

pub fn shortcut_back() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘["
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+["
    }
}

pub fn shortcut_forward() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘]"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+]"
    }
}

pub fn shortcut_log() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘⇧L"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+Shift+L"
    }
}

pub fn shortcut_find_repo() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘L"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+L"
    }
}

pub fn shortcut_find() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘F"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+F"
    }
}

/// Sublime Text on macOS when `HYDRA_MERGE_SMERGE` is unset.
pub const DEFAULT_SMERGE: &str = "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl";

/// Sublime Text command-line tool when `HYDRA_MERGE_SMERGE` is unset.
pub fn default_smerge() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(DEFAULT_SMERGE)
    }
    #[cfg(windows)]
    {
        std::env::var_os("ProgramFiles")
            .map(|dir| PathBuf::from(dir).join("Sublime Text").join("subl.exe"))
            .unwrap_or_else(|| PathBuf::from("subl.exe"))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        PathBuf::from("subl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_follows_the_os() {
        let home = Path::new("/h");
        let dir = config_dir(home);
        assert!(dir.ends_with(APP_DIRNAME));
        #[cfg(target_os = "macos")]
        assert_eq!(dir, PathBuf::from("/h/Library/Application Support/hydra-merge"));
        #[cfg(windows)]
        assert!(dir.ends_with(Path::new("AppData").join("Roaming").join(APP_DIRNAME)));
        #[cfg(not(any(target_os = "macos", windows)))]
        assert_eq!(dir, PathBuf::from("/h/.config/hydra-merge"));
    }

    #[test]
    fn list_sep_matches_the_path_variable() {
        let joined = std::env::join_paths([Path::new("a"), Path::new("b")]).expect("join");
        let text = joined.to_string_lossy();
        assert!(text.contains(list_sep()));
    }
}
