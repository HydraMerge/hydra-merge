//! Looks up the latest GitHub release and replaces this copy after a checksum match.
//!
//! The check is opt-in from the window (and once a day at startup). Nothing is replaced
//! until Install is pressed.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const LATEST: &str = "https://api.github.com/repos/HydraMerge/hydra-merge/releases/latest";
const USER_AGENT: &str = "HydraMerge";
/// A release archive larger than this is refused.
const MAX_BYTES: u64 = 200 * 1024 * 1024;
const DAY: u64 = 86_400;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub asset_name: String,
    pub asset_url: String,
    pub sums_url: String,
}

/// `latest` is newer than `current`. Both are `major.minor.patch`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn parse_version(text: &str) -> (u64, u64, u64) {
    let text = text.trim().trim_start_matches('v');
    let mut parts = text.split('.');
    let n = |parts: &mut std::str::Split<char>| {
        parts
            .next()
            .unwrap_or("0")
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    };
    (n(&mut parts), n(&mut parts), n(&mut parts))
}

pub fn target_triple() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "aarch64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
}

pub fn asset_name(version: &str) -> String {
    let ext = if cfg!(target_os = "macos") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("hydra-merge-v{version}-{}.{ext}", target_triple())
}

/// `Some` when `body` is a stable release newer than this build and it has this platform's archive.
pub fn parse_latest(body: &str) -> Result<Option<Offer>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("cannot read the release list: {e}"))?;
    if value.get("draft").and_then(|v| v.as_bool()) == Some(true)
        || value.get("prerelease").and_then(|v| v.as_bool()) == Some(true)
    {
        return Ok(None);
    }
    let tag = value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "release has no tag".to_string())?;
    let version = tag.trim().trim_start_matches('v').to_string();
    if !is_newer(&version, CURRENT) {
        return Ok(None);
    }
    let assets = value
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "release has no files".to_string())?;
    let url_of = |name: &str| -> Option<String> {
        assets.iter().find_map(|asset| {
            let asset_name = asset.get("name")?.as_str()?;
            if asset_name != name {
                return None;
            }
            asset
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
    };
    let name = asset_name(&version);
    let asset_url = url_of(&name).ok_or_else(|| format!("no archive named {name}"))?;
    let sums_url = url_of("SHA256SUMS").ok_or_else(|| "release has no SHA256SUMS".to_string())?;
    Ok(Some(Offer {
        version,
        asset_name: name,
        asset_url,
        sums_url,
    }))
}

pub fn sums_match(sums: &str, name: &str, bytes: &[u8]) -> bool {
    let digest = hex_sha256(bytes);
    sums.lines().any(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next().unwrap_or("");
        let file = parts.next().unwrap_or("").trim_start_matches('*');
        hash.eq_ignore_ascii_case(&digest) && file == name
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn fetch_offer() -> Result<Option<Offer>, String> {
    let body = get_text(LATEST)?;
    parse_latest(&body)
}

/// Downloads the archive, checks `SHA256SUMS`, and unpacks it. Returns the new app or binary.
pub fn download(offer: &Offer) -> Result<PathBuf, String> {
    let sums = get_text(&offer.sums_url)?;
    let bytes = get_bytes(&offer.asset_url)?;
    if !sums_match(&sums, &offer.asset_name, &bytes) {
        return Err("the download does not match SHA256SUMS".to_string());
    }
    let dir = std::env::temp_dir().join(format!("hydra-merge-update-{}", offer.version));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("cannot prepare the update: {e}"))?;
    let archive = dir.join(&offer.asset_name);
    fs::write(&archive, &bytes).map_err(|e| format!("cannot store the download: {e}"))?;
    unpack(&archive, &dir)
}

fn unpack(archive: &Path, dir: &Path) -> Result<PathBuf, String> {
    if archive
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.ends_with(".zip"))
    {
        let status = Command::new("ditto")
            .args(["-x", "-k"])
            .arg(archive)
            .arg(dir)
            .status()
            .map_err(|e| format!("cannot unpack the app: {e}"))?;
        if !status.success() {
            return Err("cannot unpack the app".to_string());
        }
        let app = dir.join("HydraMerge.app");
        if !app.is_dir() {
            return Err("the archive has no HydraMerge.app".to_string());
        }
        let _ = Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&app)
            .status();
        return Ok(app);
    }
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(dir)
        .status()
        .map_err(|e| format!("cannot unpack the archive: {e}"))?;
    if !status.success() {
        return Err("cannot unpack the archive".to_string());
    }
    let bin = if cfg!(windows) {
        "hydra-merge.exe"
    } else {
        "hydra-merge"
    };
    let path = dir.join(bin);
    if !path.is_file() {
        return Err(format!("the archive has no {bin}"));
    }
    Ok(path)
}

/// Starts a helper that waits for this process to exit, then swaps in `staged` and relaunches.
pub fn handoff(staged: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let pid = std::process::id().to_string();
    #[cfg(target_os = "macos")]
    if let Some(app) = macos_bundle(&exe) {
        return spawn_shell(&macos_script(&pid, staged, &app));
    }
    #[cfg(target_os = "windows")]
    {
        return spawn_windows(&pid, staged, &exe);
    }
    #[cfg(not(target_os = "windows"))]
    spawn_shell(&unix_binary_script(&pid, staged, &exe))
}

fn macos_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (app.extension()? == "app").then(|| app.to_path_buf())
}

fn macos_script(pid: &str, staged: &Path, dest: &Path) -> String {
    format!(
        "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\nrm -rf {dest}\nditto {src} {dest}\nxattr -dr com.apple.quarantine {dest} || true\nopen {dest}\n",
        src = shell_quote(staged),
        dest = shell_quote(dest),
    )
}

fn unix_binary_script(pid: &str, staged: &Path, dest: &Path) -> String {
    format!(
        "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\ncp {src} {dest}\nchmod +x {dest}\nexec {dest}\n",
        src = shell_quote(staged),
        dest = shell_quote(dest),
    )
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn spawn_shell(script: &str) -> Result<(), String> {
    let file = std::env::temp_dir().join(format!("hydra-merge-update-{}.sh", std::process::id()));
    fs::write(&file, script).map_err(|e| format!("cannot write the updater: {e}"))?;
    Command::new("sh")
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start the updater: {e}"))
}

#[cfg(target_os = "windows")]
fn spawn_windows(pid: &str, staged: &Path, dest: &Path) -> Result<(), String> {
    let script = format!(
        "while (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}\nMove-Item -Force -LiteralPath '{dest}' '{dest}.old'\nMove-Item -Force -LiteralPath '{src}' '{dest}'\nStart-Process -LiteralPath '{dest}'\n",
        dest = dest.display(),
        src = staged.display(),
    );
    let file = std::env::temp_dir().join(format!("hydra-merge-update-{}.ps1", std::process::id()));
    fs::write(&file, script).map_err(|e| format!("cannot write the updater: {e}"))?;
    Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start the updater: {e}"))
}

fn get_text(url: &str) -> Result<String, String> {
    let bytes = get_bytes(url)?;
    String::from_utf8(bytes).map_err(|_| "the response was not text".to_string())
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("cannot reach GitHub: {e}"))?;
    let mut reader = response.into_reader().take(MAX_BYTES + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read the download: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("the download is too large".to_string());
    }
    Ok(bytes)
}

pub fn check_is_due(dir: &Path) -> bool {
    let now = unix_now();
    let stamp = fs::read_to_string(dir.join("update-checked.txt"))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(0);
    now.saturating_sub(stamp) >= DAY
}

pub fn mark_checked(dir: &Path) {
    let _ = fs::write(dir.join("update-checked.txt"), format!("{}\n", unix_now()));
}

pub fn dismissed_version(dir: &Path) -> Option<String> {
    fs::read_to_string(dir.join("update-later.txt"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

pub fn dismiss(dir: &Path, version: &str) {
    let _ = fs::write(dir.join("update-later.txt"), format!("{version}\n"));
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(tag: &str, names: &[&str]) -> String {
        let assets: Vec<String> = names
            .iter()
            .map(|name| {
                format!(
                    r#"{{"name":"{name}","browser_download_url":"https://example.test/{name}"}}"#
                )
            })
            .collect();
        format!(
            r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{}]}}"#,
            assets.join(",")
        )
    }

    #[test]
    fn a_newer_stable_release_is_offered_for_this_target() {
        let name = asset_name("9.9.9");
        let body = release_json("v9.9.9", &[&name, "SHA256SUMS"]);
        let offer = parse_latest(&body).expect("parse").expect("offer");
        assert_eq!(offer.version, "9.9.9");
        assert_eq!(offer.asset_name, name);
        assert!(offer.sums_url.ends_with("SHA256SUMS"));
    }

    #[test]
    fn the_same_version_prerelease_and_bad_checksum_are_declined() {
        let name = asset_name(CURRENT);
        let same = release_json(&format!("v{CURRENT}"), &[&name, "SHA256SUMS"]);
        assert_eq!(parse_latest(&same).expect("parse"), None);
        let pre = format!(
            r#"{{"tag_name":"v9.9.9","draft":false,"prerelease":true,"assets":[]}}"#
        );
        assert_eq!(parse_latest(&pre).expect("parse"), None);
        assert!(!is_newer("0.1.0", CURRENT));
        assert!(is_newer("99.0.0", "1.0.0"));
        let bytes = b"hello";
        let digest = hex_sha256(bytes);
        let sums = format!("{digest}  {name}\n");
        assert!(sums_match(&sums, &name, bytes));
        assert!(!sums_match(&sums, &name, b"nope"));
        assert!(!sums_match("abc  other\n", &name, bytes));
    }
}
