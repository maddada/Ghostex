//! Downloading, verifying and unpacking the official archives Ghostex installs from.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use super::jobs::Log;

const MAX_ARCHIVE_BYTES: u64 = 400 * 1024 * 1024;

fn agent(redirects: u32) -> ureq::Agent {
    crate::http_agent::config()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .max_redirects(redirects)
        .user_agent("Ghostex")
        .build()
        .into()
}

pub(crate) fn get_text(url: &str) -> Result<String, String> {
    agent(8)
        .get(url)
        .call()
        .map_err(|error| describe(url, error))?
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("Could not read {url}: {error}"))
}

pub(crate) fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let response = agent(8)
        .get(url)
        .call()
        .map_err(|error| describe(url, error))?;
    crate::http_agent::read_json(response).map_err(|error| format!("Could not read {url}: {error}"))
}

fn describe(url: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("{url} answered HTTP {code}."),
        transport => format!(
            "Could not reach {}. Check the internet connection and try again. ({transport})",
            url.split('/').take(3).collect::<Vec<_>>().join("/")
        ),
    }
}

/// CDXC:ManagedTools 2026-09-29 WHY:
/// The newest release is read from the `releases/latest` redirect instead of the GitHub (or GitLab) API: the API allows 60 unauthenticated requests an hour per address, and the audit of vendor installers found OpenCode, Beads and codex-swap failing on exactly that limit behind shared or office networks.
pub(crate) fn latest_release_tag(latest_url: &str) -> Result<String, String> {
    let response = agent(0)
        .head(latest_url)
        .call()
        .map_err(|error| describe(latest_url, error))?;
    let location = crate::http_agent::header(&response, "location")
        .ok_or_else(|| format!("{latest_url} did not point to a release."))?;
    location
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|tag| !tag.is_empty() && *tag != "latest")
        .map(str::to_string)
        .ok_or_else(|| format!("{latest_url} did not point to a release."))
}

/// Downloads `url` into `destination`, logging progress every tenth of the way.
pub(crate) fn download(
    url: &str,
    destination: &Path,
    label: &str,
    log: &Log,
) -> Result<(), String> {
    let response = agent(8)
        .get(url)
        .call()
        .map_err(|error| describe(url, error))?;
    let total = crate::http_agent::header(&response, "content-length")
        .and_then(|value| value.parse::<u64>().ok());
    match total {
        Some(bytes) => log.line(&format!("Downloading {label} ({})…", megabytes(bytes))),
        None => log.line(&format!("Downloading {label}…")),
    }
    let mut reader = response.into_body().into_reader().take(MAX_ARCHIVE_BYTES);
    let mut file = File::create(destination)
        .map_err(|error| format!("Could not write {}: {error}", destination.display()))?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut received = 0u64;
    let mut next_report = 1u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("The download of {label} stopped: {error}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not write {}: {error}", destination.display()))?;
        received += read as u64;
        if let Some(total) = total.filter(|total| *total > 0) {
            let tenth = received * 10 / total;
            if tenth >= next_report && tenth < 10 {
                log.line(&format!("  {}0%", tenth));
                next_report = tenth + 1;
            }
        }
    }
    if total.is_some_and(|total| total != received) {
        return Err(format!("The download of {label} was cut short. Try again."));
    }
    Ok(())
}

fn megabytes(bytes: u64) -> String {
    format!("{:.0} MB", (bytes as f64 / 1_048_576.0).max(1.0))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

/// The hash for `filename` in a `sha256sum`-style list (`<hash>  <name>` or `<hash> *<name>`).
pub(crate) fn checksum_for(list: &str, filename: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        let name = name.rsplit('/').next().unwrap_or(name);
        (name == filename && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

pub(crate) fn verify(path: &Path, expected: &str, label: &str, log: &Log) -> Result<(), String> {
    let actual = sha256_file(path)?;
    if actual != expected.to_lowercase() {
        return Err(format!(
            "The {label} download did not match its published checksum, so Ghostex did not install it. Try again."
        ));
    }
    log.line("Checksum verified.");
    Ok(())
}

/// Unpacks a `.zip` or `.tar.gz` archive into `into`.
pub(crate) fn extract(archive: &Path, into: &Path) -> Result<(), String> {
    fs::create_dir_all(into).map_err(|error| error.to_string())?;
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        let file = File::open(archive).map_err(|error| error.to_string())?;
        zip::ZipArchive::new(file)
            .and_then(|mut zip| zip.extract(into))
            .map_err(|error| format!("Could not unpack {name}: {error}"))?;
        return Ok(());
    }
    // `tar` ships with macOS, every Linux distribution and Windows 10+.
    let output = crate::platform::process::background_command("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not run tar to unpack {name}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not unpack {name}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// The first file called `name` anywhere under `root`.
pub(crate) fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|file| file.to_str()) == Some(name) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(unix)]
pub(crate) fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
pub(crate) fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Moves `staged` to `target`, replacing what was there. The old copy is renamed away first so a
/// failed move can put it back.
pub(crate) fn replace_path(staged: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let previous = target.with_file_name(format!(
        ".{}.old-{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("tool"),
        uuid::Uuid::new_v4()
    ));
    let had_previous = target.exists();
    if had_previous {
        fs::rename(target, &previous).map_err(|error| {
            format!(
                "Could not replace {} (is it still running? Close sessions that use it and try again): {error}",
                target.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(staged, target) {
        if had_previous {
            let _ = fs::rename(&previous, target);
        }
        return Err(format!(
            "Could not install into {}: {error}",
            target.display()
        ));
    }
    if had_previous {
        let _ = if previous.is_dir() {
            fs::remove_dir_all(&previous)
        } else {
            fs::remove_file(&previous)
        };
    }
    Ok(())
}

/// A scratch folder next to the tools, removed when dropped.
pub(crate) struct Scratch(pub PathBuf);

impl Scratch {
    pub(crate) fn new() -> Result<Self, String> {
        let dir = super::paths::tools_root().join(format!(".download-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir)
            .map_err(|error| format!("Could not create {}: {error}", dir.display()))?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
