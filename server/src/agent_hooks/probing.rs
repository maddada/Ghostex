use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{Map, Value};

use crate::{
    domain::DomainStateError,
    platform::shell::{command_shell, command_shell_for_path, login_shell_candidates},
};

use super::config::{GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS, SHELL_PATH_SENTINEL};
use super::plugin_sources::shell_quote;
use super::probe_cache::{
    cached_resolve_command_path, login_shell_path_entries, refresh_resolved_command_path,
};

pub(crate) fn parse_global_session_ref(value: &str) -> (Option<String>, Option<String>) {
    let parts = value.trim().split(':').collect::<Vec<_>>();
    if parts.len() == 3 && !parts[1].is_empty() && !parts[2].is_empty() {
        (Some(parts[1].to_string()), Some(parts[2].to_string()))
    } else {
        (None, None)
    }
}

pub(crate) fn decode_base64_text(value: &str) -> String {
    BASE64_STANDARD
        .decode(value)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn normalize_prompt_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn expand_home_path(value: &str) -> PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        return dirs_home();
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return dirs_home().join(relative);
    }
    PathBuf::from(trimmed)
}

fn dirs_home() -> PathBuf {
    env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("hook-state");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ))
}

pub(crate) fn insert_json_string(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(crate) fn unique_path_bufs(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut output = Vec::new();
    for path in paths {
        if !output.contains(&path) {
            output.push(path);
        }
    }
    output
}

pub(crate) fn normalize_environment_path(value: Option<&str>, home_dir: &Path) -> Option<PathBuf> {
    let trimmed = value.map(str::trim).filter(|value| !value.is_empty())?;
    if trimmed == "~" {
        return Some(home_dir.to_path_buf());
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return Some(home_dir.join(relative));
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(home_dir.join(path))
    }
}

pub(crate) fn list_profile_hook_paths(
    home_dir: &Path,
    profile_dir: &str,
    file_name: &str,
) -> Vec<PathBuf> {
    let profiles_path = home_dir.join(profile_dir);
    let Ok(entries) = fs::read_dir(&profiles_path) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|file_type| file_type.is_dir())
                .map(|_| entry.path().join(file_name))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Status-freshness check for a provider CLI. Served from the 60 second probe
/// cache in `probe_cache`, because the subprocess probes underneath cost more
/// than every other part of a hook status read combined.
pub(crate) fn command_exists(command: &str, home_dir: &Path) -> bool {
    cached_resolve_command_path(command, home_dir).is_some()
}

/// Same check, but re-probed now and written back to the cache. Install acts on
/// the answer, so it must not skip a provider whose CLI appeared inside the
/// cache window.
pub(crate) fn command_exists_uncached(command: &str, home_dir: &Path) -> bool {
    refresh_resolved_command_path(command, home_dir).is_some()
}

pub(super) fn resolve_command_path(command: &str, home_dir: &Path) -> Option<String> {
    /*
    CDXC:AgentHooks 2026-06-23-07:52:
    Hook status must discover the same agent CLIs on macOS and Ubuntu before reporting cliMissing. Merge login-shell PATH entries, GUI/default tool directories, and user PATH, then run the final command-v probe in the platform shell so startup files cannot overwrite the normalized PATH and Linux does not require zsh.
    */
    let path_value =
        normalize_gxserver_process_path(std::env::var("PATH").ok().as_deref(), home_dir);
    let shell = command_shell();
    let mut command_process = Command::new(&shell.executable);
    command_process.args(shell.script_args(&format!("command -v {}", shell_quote(command))));
    apply_hook_command_environment(&mut command_process, home_dir);
    command_process.env("PATH", path_value);
    let stdout = run_command_stdout_with_timeout(command_process, Duration::from_millis(2_000))?;
    stdout
        .trim()
        .split('\n')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_gxserver_process_path(current_path: Option<&str>, home_dir: &Path) -> String {
    let mut entries = Vec::new();
    entries.extend(login_shell_path_entries(home_dir));
    entries.extend(split_path(current_path));
    entries.extend([
        path_string(&home_dir.join(".opencode").join("bin")),
        path_string(
            &home_dir
                .join(".local")
                .join("share")
                .join("mise")
                .join("shims"),
        ),
        path_string(&home_dir.join(".local").join("bin")),
        path_string(&home_dir.join(".asdf").join("shims")),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ]);
    unique_path_entries(entries).join(":")
}

/// Raw login-shell PATH probe. Every caller goes through
/// `probe_cache::login_shell_path_entries`, which keeps the interactive shell
/// spawn to once per process.
pub(super) fn probe_login_shell_path_entries(home_dir: &Path) -> Vec<String> {
    for candidate in login_shell_candidates() {
        let candidate_path = PathBuf::from(&candidate);
        if !is_executable(&candidate_path) {
            continue;
        }
        let entries = run_login_shell_path_probe(&candidate, home_dir);
        if !entries.is_empty() {
            return entries;
        }
    }
    Vec::new()
}

fn run_login_shell_path_probe(shell_path: &str, home_dir: &Path) -> Vec<String> {
    let shell = command_shell_for_path(shell_path);
    let mut command = Command::new(&shell.executable);
    command.args(
        shell.interactive_script_args(&format!("printf '\\n{SHELL_PATH_SENTINEL}%s\\n' \"$PATH\"")),
    );
    apply_hook_command_environment(&mut command, home_dir);
    let Some(stdout) = run_command_stdout_with_timeout(command, Duration::from_millis(2_000))
    else {
        return Vec::new();
    };
    stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(SHELL_PATH_SENTINEL))
        .map(|path| split_path(Some(path)))
        .unwrap_or_default()
}

fn apply_hook_command_environment(command: &mut Command, home_dir: &Path) {
    command.env("HOME", home_dir);
    for key in GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
}

fn run_command_stdout_with_timeout(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut stdout = String::new();
                if let Some(mut child_stdout) = child.stdout.take() {
                    let _ = child_stdout.read_to_string(&mut stdout);
                }
                let _ = child.wait();
                return Some(stdout);
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn split_path(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn unique_path_entries(entries: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    let mut output = Vec::new();
    for entry in entries {
        if entry.is_empty() || seen.contains(&entry) {
            continue;
        }
        seen.push(entry.clone());
        output.push(entry);
    }
    output
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        return fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub(crate) fn read_file_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

pub(crate) fn display_path(path: &str, home_dir: &Path) -> String {
    let home = path_string(home_dir);
    path.strip_prefix(&format!("{home}/"))
        .map(|relative| format!("~/{relative}"))
        .unwrap_or_else(|| path.to_string())
}

pub(crate) fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(crate) fn push_unique_path(paths: &mut Vec<String>, path: String) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn json_error(error: serde_json::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Agent hook JSON operation failed: {error}"),
    }
}

pub(crate) fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Agent hook file operation failed: {error}"),
    }
}
