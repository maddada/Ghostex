use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{Map, Value};

#[cfg(not(windows))]
use crate::platform::shell::command_shell;
use crate::domain::DomainStateError;

use super::config::GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS;
#[cfg(not(windows))]
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

/// What hook status says about an agent's CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliPresence {
    Installed,
    /// Only an install-on-demand launcher is on PATH (`on_demand.rs`): not installed yet.
    InstallsOnDemand,
    Missing,
}

/// CDXC:AgentHooks 2026-10-28 WHY:
/// Hook status counted any `gemini` on PATH as installed, so setup showed Omarchy's
/// install-on-first-run launcher (`mise use -g …` then `mise x …`) as "Installed / Connected" while
/// gxserver's own probes treated the same agent as not installed yet. Status now uses the probe rule
/// (`on_demand::probe_target`): a real install, or the install a launcher would run once mise has
/// it, is installed; a launcher alone is "installs on demand".
pub(crate) fn cli_presence(command: &str, home_dir: &Path) -> CliPresence {
    cli_presence_with(
        cached_resolve_command_path(command, home_dir).as_deref(),
        |path| super::on_demand::probe_target(path, command, home_dir),
    )
}

/// `cli_presence` with the resolved PATH entry and the probe rule passed in.
pub(crate) fn cli_presence_with(
    resolved: Option<&str>,
    probe_target: impl FnOnce(&str) -> Option<String>,
) -> CliPresence {
    match resolved {
        None => CliPresence::Missing,
        Some(path) if probe_target(path).is_some() => CliPresence::Installed,
        Some(_) => CliPresence::InstallsOnDemand,
    }
}

/// Same check, but re-probed now and written back to the cache. Install acts on
/// the answer, so it must not skip a provider whose CLI appeared inside the
/// cache window.
pub(crate) fn resolve_cli_command(command: &str, home_dir: &Path) -> Option<String> {
    refresh_resolved_command_path(command, home_dir)
}

/// The agent CLI a probe Ghostex runs on its own (model lineups, versions, sign-in state) may run:
/// `None` while `command` is only an install-on-demand wrapper, so looking never installs it.
/// Starting an agent the user picked keeps using the name on PATH, wrapper included.
pub(crate) fn resolve_probe_command(command: &str, home_dir: &Path) -> Option<String> {
    let path = resolve_cli_command(command, home_dir)?;
    super::on_demand::probe_target(&path, command, home_dir)
}

/// The PATH entries the user's login shell sets up, which is what a new session's shell starts with.
/// Waits longer for a slow shell than the quick probe agent launches use, because it backs decisions
/// that change the user's setup.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn login_shell_path_patient(home_dir: &Path) -> Vec<String> {
    super::probe_cache::login_shell_path_entries_patient(home_dir)
}

pub(crate) fn refresh_cli_environment(home_dir: &Path) {
    super::probe_cache::invalidate_login_shell_path(home_dir);
}

pub(crate) fn command_exists_uncached(command: &str, home_dir: &Path) -> bool {
    refresh_resolved_command_path(command, home_dir).is_some()
}

#[cfg(windows)]
pub(super) fn resolve_command_path(command: &str, _home_dir: &Path) -> Option<String> {
    super::windows::resolve_command(command)
}

#[cfg(not(windows))]
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

/// Stdout of a one-line CLI script run the way `resolve_command_path` finds CLIs, so a probe reaches the same binary a session would launch.
#[cfg(not(windows))]
pub(crate) fn cli_script_stdout(
    script: &str,
    home_dir: &Path,
    timeout: Duration,
) -> Option<String> {
    let shell = command_shell();
    let mut command_process = Command::new(&shell.executable);
    command_process.args(shell.profileless_script_args(script));
    apply_hook_command_environment(&mut command_process, home_dir);
    #[cfg(not(windows))]
    command_process.env(
        "PATH",
        normalize_gxserver_process_path(std::env::var("PATH").ok().as_deref(), home_dir),
    );
    run_command_stdout_with_timeout(command_process, timeout)
}

pub(crate) fn normalize_gxserver_process_path(
    current_path: Option<&str>,
    home_dir: &Path,
) -> String {
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
    // The tools Ghostex installed itself come last (user decision 1A in managed_tools/paths.rs).
    entries.extend(
        crate::managed_tools::paths::path_dirs()
            .iter()
            .map(|dir| path_string(dir)),
    );
    unique_path_entries(entries).join(":")
}

/// Raw login-shell PATH probe. Every caller goes through
/// `probe_cache::login_shell_path_entries`, which keeps the interactive shell
/// spawn to once per process.
pub(super) fn probe_login_shell_path_entries(home_dir: &Path, timeout: Duration) -> Vec<String> {
    let managed_paths = crate::managed_tools::paths::path_dirs();
    let initial_path = crate::managed_tools::paths::process_path_without_tools();
    ghostex_paths::tool_path::probe_login_shell_path_entries_with_path(
        home_dir,
        timeout,
        Some(initial_path.as_os_str()),
    )
    .into_iter()
    .filter(|entry| {
        !managed_paths
            .iter()
            .any(|dir| path_string(dir).as_str() == entry.as_str())
    })
    .collect()
}

fn apply_hook_command_environment(command: &mut Command, home_dir: &Path) {
    command.env("HOME", home_dir);
    for key in GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
}

/// Stdout of a command that exited successfully within `timeout`. A command that runs longer is
/// stopped with every process it started (`managed_tools::run::output_within`), so a looping
/// wrapper or shim leaves nothing behind (GitHub #336).
pub(crate) fn run_command_stdout_with_timeout(
    command: Command,
    timeout: Duration,
) -> Option<String> {
    successful_stdout(crate::managed_tools::run::output_within(command, timeout))
}

fn successful_stdout(
    result: Result<crate::managed_tools::run::Finished, crate::managed_tools::run::Unfinished>,
) -> Option<String> {
    result
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout)
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
