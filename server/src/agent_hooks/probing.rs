use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{Map, Value};

use crate::domain::DomainStateError;
#[cfg(not(windows))]
use crate::platform::shell::command_shell;

use super::config::GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS;
use super::probe_cache::{
    cached_resolve_command_path, login_shell_path_entries, refresh_resolved_command_path,
};

/// CDXC:AgentProviders 2026-11-02 WHY:
/// Antigravity's `agy` starts its own detached updater whenever its last update check is 15 minutes old, whatever the command (even `agy models` or `agy --version`), and on Windows that updater runs `agy --version` in a new visible console, so the lineup probe popped a Windows Terminal window every ~20 minutes (issue #397). gxserver sets this on every background `agy` it runs (the lineup read, the Agents Hub version and update checks, title, commit-message and catch-up jobs), never on a session the user starts, so agy keeps updating itself there. agy reads only the value `true` (`1` is ignored).
pub(crate) const AGY_BACKGROUND_AUTO_UPDATE_OFF: (&str, &str) =
    ("AGY_CLI_DISABLE_AUTO_UPDATE", "true");

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
    // The counter keeps two writes of one file in the same millisecond apart.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    path.with_file_name(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_millis(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

/// CDXC:AgentHooks 2026-11-06 WHY:
/// Discord report (Faris Radifan A, 31 Oct): an agent installed during onboarding showed "Install" again when the Install guide was reopened. The install state (`agent_cli::process::resolve_agent`) finds a CLI on PATH or in the folders its own installer uses (`~/.amp/bin`, `~/.grok/bin`, `~/.empryo/bin`, …), but this scan only looked on PATH, so an agent in its installer's folder that no shell had put on PATH yet read as installed in one place and missing in the other, and the scan (which decides what a reopened window lists) won. The scan now accepts the same folders, with the same install-on-demand rule as `cli_presence`. `fresh` re-probes PATH now (the hook install gate) instead of using the 60 second cache.
pub(crate) fn agent_cli_presence(
    agent_id: &str,
    command: &str,
    home_dir: &Path,
    fresh: bool,
) -> CliPresence {
    let resolved = if fresh {
        refresh_resolved_command_path(command, home_dir)
    } else {
        cached_resolve_command_path(command, home_dir)
    };
    let on_path = cli_presence_with(resolved.as_deref(), |path| {
        super::on_demand::probe_target(path, command, home_dir)
    });
    if on_path != CliPresence::Missing {
        return on_path;
    }
    let Some(definition) = crate::agent_cli::catalog::CATALOG
        .iter()
        .find(|definition| definition.agent_id == agent_id)
    else {
        return on_path;
    };
    let found =
        crate::platform::live_path::find_in(&definition.binary, &definition.install_dirs(home_dir))
            .map(|path| path.to_string_lossy().into_owned())
            .and_then(|path| super::on_demand::probe_target(&path, &definition.binary, home_dir));
    if found.is_some() {
        CliPresence::Installed
    } else {
        on_path
    }
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
    crate::platform::live_path::clear_cache();
}

/// Result of asking the user's real shell whether a command resolves (including PATH entries,
/// shell aliases, shell functions, and profile/rc scripts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellCommandProbe {
    /// The user's shell confirmed the command resolves.
    Found,
    /// The user's shell finished loading cleanly and confirmed the command is absent.
    Missing,
    /// The probe timed out, failed to spawn, hit an unsupported shell, or the profile exited
    /// before the check ran; callers must fail open and allow the launch.
    Inconclusive,
}

const SHELL_CLI_PROBE_FOUND_SENTINEL: &str = "__GHOSTEX_CLI_PROBE__:FOUND";
const SHELL_CLI_PROBE_MISSING_SENTINEL: &str = "__GHOSTEX_CLI_PROBE__:MISSING";

/// Whether a recent [`probe_command_in_user_shell`] call confirmed `command` is runnable in
/// `home_dir`'s shell (non-blocking cache lookup only).
pub(crate) fn cached_shell_command_found(command: &str, home_dir: &Path) -> bool {
    super::probe_cache::cached_shell_command_found(command, home_dir)
}

/// Strict validation so a binary name passed to the shell probe via env can never look like an
/// option flag or shell metacharacter.
fn is_safe_probe_binary(binary: &str) -> bool {
    let mut chars = binary.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphanumeric())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn parse_shell_probe_sentinel(stdout: &str) -> Option<ShellCommandProbe> {
    stdout.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        if trimmed == SHELL_CLI_PROBE_FOUND_SENTINEL {
            Some(ShellCommandProbe::Found)
        } else if trimmed == SHELL_CLI_PROBE_MISSING_SENTINEL {
            Some(ShellCommandProbe::Missing)
        } else {
            None
        }
    })
}

/// Self-healing final check run only after cached and fresh PATH/installDirs lookups report
/// `Missing`. Probes the user's real shell (seeing PATH, aliases, functions, and profile/rc
/// scripts) with a bounded timeout and fails open (`Inconclusive`) on any timeout or error.
pub(crate) fn probe_command_in_user_shell(binary: &str, home_dir: &Path) -> ShellCommandProbe {
    if !is_safe_probe_binary(binary) {
        return ShellCommandProbe::Inconclusive;
    }
    #[cfg(windows)]
    let outcome = probe_command_in_windows_shell(binary);
    #[cfg(not(windows))]
    let outcome = probe_command_in_unix_shell(binary, home_dir);
    if outcome == ShellCommandProbe::Found {
        super::probe_cache::record_shell_command_found(binary, home_dir);
    }
    outcome
}

#[cfg(not(windows))]
fn probe_command_in_unix_shell(binary: &str, home_dir: &Path) -> ShellCommandProbe {
    use std::{
        io::Read,
        os::unix::process::CommandExt,
        process::Stdio,
        sync::{Arc, Mutex, PoisonError},
        time::Instant,
    };

    const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
    // Static scripts only: the binary name travels via GHOSTEX_CLI_PROBE_BIN, never interpolated.
    const POSIX_SCRIPT: &str = "if command -v \"$GHOSTEX_CLI_PROBE_BIN\" >/dev/null 2>&1; then printf '\\n__GHOSTEX_CLI_PROBE__:FOUND\\n'; else printf '\\n__GHOSTEX_CLI_PROBE__:MISSING\\n'; fi";
    const FISH_SCRIPT: &str = "if type -q -- $GHOSTEX_CLI_PROBE_BIN; printf '\\n__GHOSTEX_CLI_PROBE__:FOUND\\n'; else; printf '\\n__GHOSTEX_CLI_PROBE__:MISSING\\n'; end";

    let shell_path = crate::platform::shell::user_login_shell_path();
    let shell_name = Path::new(&shell_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let mut command = Command::new(&shell_path);
    match shell_name {
        "zsh" | "bash" => {
            command.args(["-lic", POSIX_SCRIPT]);
        }
        "fish" => {
            command.args(["-l", "-i", "-c", FISH_SCRIPT]);
        }
        "sh" | "dash" | "ash" | "ksh" => {
            command.args(["-l", "-i", "-c", POSIX_SCRIPT]);
        }
        // Unknown or exotic shells (nu, csh, tcsh, elvish, ...): fail open rather than guessing.
        _ => return ShellCommandProbe::Inconclusive,
    }

    command
        .env("HOME", home_dir)
        .env(
            "PATH",
            crate::managed_tools::paths::process_path_without_tools(),
        )
        .env("GHOSTEX_CLI_PROBE_BIN", binary)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // When probing an isolated home (e.g. GHOSTEX_HOME), don't let the host's ZDOTDIR redirect
    // zsh away from home_dir/.zshrc.
    if env::var_os("HOME").as_deref() != Some(home_dir.as_os_str()) {
        command.env_remove("ZDOTDIR");
    }
    for key in GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }

    // Start a new session with no controlling terminal so interactive shells never stop on
    // SIGTTIN/SIGTTOU or hang reading /dev/tty, while leading their own process group so timeout
    // cleanup kills the shell and any child processes it spawned.
    // SAFETY: `setsid` is async-signal-safe and touches no Rust state.
    unsafe {
        command.pre_exec(|| {
            let _ = libc::setsid();
            Ok(())
        });
    }

    let deadline = Instant::now() + PROBE_TIMEOUT;
    let Ok(mut child) = command.spawn() else {
        return ShellCommandProbe::Inconclusive;
    };
    let pgid = libc::pid_t::try_from(child.id())
        .ok()
        .filter(|pid| *pid > 0);
    let kill_group = |child: &mut std::process::Child| {
        if let Some(group) = pgid {
            // SAFETY: `group` is the positive pid/pgid created by `setsid()` in the child.
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    };

    let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader = child.stdout.take().map(|mut pipe| {
        let buffer = Arc::clone(&buffer);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            // Keep the tail so a noisy profile banner cannot evict the final sentinel line.
            const MAX_BYTES: usize = 256 * 1024;
            while let Ok(read) = pipe.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let mut guard = buffer.lock().unwrap_or_else(PoisonError::into_inner);
                if guard.len() + read > MAX_BYTES {
                    let keep = MAX_BYTES.saturating_sub(read);
                    let drop_len = guard.len().saturating_sub(keep);
                    guard.drain(..drop_len);
                }
                guard.extend_from_slice(&chunk[..read]);
            }
        })
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(15));
            }
            _ => {
                kill_group(&mut child);
                return ShellCommandProbe::Inconclusive;
            }
        }
    };
    if !status.success() {
        return ShellCommandProbe::Inconclusive;
    }

    let read_output = || {
        String::from_utf8_lossy(&buffer.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    };
    // Do not wait for pipe EOF if a background child spawned by the user's profile inherited
    // stdout: return as soon as the sentinel line arrives or the reader finishes.
    let read_deadline = deadline.min(Instant::now() + Duration::from_millis(500));
    loop {
        if let Some(outcome) = parse_shell_probe_sentinel(&read_output()) {
            return outcome;
        }
        if reader.as_ref().is_none_or(|reader| reader.is_finished())
            || Instant::now() >= read_deadline
        {
            return parse_shell_probe_sentinel(&read_output())
                .unwrap_or(ShellCommandProbe::Inconclusive);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(windows)]
fn probe_command_in_windows_shell(binary: &str) -> ShellCommandProbe {
    const PROBE_TIMEOUT: Duration = Duration::from_secs(6);
    // Static script only: the binary name travels via GHOSTEX_CLI_PROBE_BIN, never interpolated.
    // Loads the user's PowerShell $PROFILE (no -NoProfile) so fnm/volta/nvm-windows PATH entries,
    // aliases, and functions are visible.
    const SCRIPT: &str = "if (Get-Command -Name $env:GHOSTEX_CLI_PROBE_BIN -ErrorAction SilentlyContinue) { [Console]::WriteLine('__GHOSTEX_CLI_PROBE__:FOUND'); exit 0 } else { [Console]::WriteLine('__GHOSTEX_CLI_PROBE__:MISSING'); exit 0 }";

    let powershell = crate::platform::shell::powershell_executable();
    let mut command = Command::new(&powershell);
    command
        .args(["-NoLogo", "-NonInteractive", "-Command", SCRIPT])
        .env("PATH", crate::platform::live_path::value())
        .env("GHOSTEX_CLI_PROBE_BIN", binary);
    for key in GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }

    let limits = crate::platform::process::RunLimits {
        timeout: PROBE_TIMEOUT,
        stdin: None,
        stdout: crate::platform::process::Keep::Tail(256 * 1024),
        stderr: crate::platform::process::Keep::Head(16 * 1024),
    };
    let Ok(output) = crate::platform::process::run_bounded(command, limits) else {
        return ShellCommandProbe::Inconclusive;
    };
    if !output.status.success() {
        return ShellCommandProbe::Inconclusive;
    }
    parse_shell_probe_sentinel(&String::from_utf8_lossy(&output.stdout))
        .unwrap_or(ShellCommandProbe::Inconclusive)
}

pub(crate) fn command_exists_uncached(command: &str, home_dir: &Path) -> bool {
    refresh_resolved_command_path(command, home_dir).is_some()
}

#[cfg(windows)]
pub(super) fn resolve_command_path(command: &str, _home_dir: &Path) -> Option<String> {
    super::windows::resolve_command(command)
}

/// CDXC:AgentHooks 2026-11-04 WHY:
/// macOS looked each CLI up with `zsh -lc "command -v <cli>"`: a login shell per lookup, which ran the user's `.zprofile` (`brew shellenv`, `path_helper`) every time, four processes and about 24 ms of CPU, 14 times in five minutes on a working Mac (measured 2026-11-04, issues-1027/macos-perf). The login shell's PATH is already part of the normalized PATH (`login_shell_path_entries`, probed once per process), so the lookup is the same `command -v` search done in-process: the first executable `<dir>/<cli>` on that PATH, or the path itself when it names one.
/// CDXC:AgentHooks 2026-11-21 FIX:
/// Linux now uses the same in-process lookup too: running `bash -lc "command -v ..."` on Debian sourced `/etc/profile`, which replaces PATH and dropped nvm, npm-global, bun and pnpm folders that `normalize_gxserver_process_path` added.
#[cfg(not(windows))]
pub(super) fn resolve_command_path(command: &str, home_dir: &Path) -> Option<String> {
    let path_value =
        normalize_gxserver_process_path(std::env::var("PATH").ok().as_deref(), home_dir);
    resolve_command_on_path(command, &path_value)
}

/// `command -v <command>` for an executable, against `path_value`.
#[cfg(not(windows))]
pub(super) fn resolve_command_on_path(command: &str, path_value: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    if command.contains('/') {
        let path = Path::new(command);
        return crate::platform::live_path::find_in(
            path.file_name()?.to_str()?,
            &[path.parent()?.to_path_buf()],
        )
        .map(|_| command.to_string());
    }
    let directories = split_path(Some(path_value))
        .into_iter()
        .filter(|entry| entry.starts_with('/'))
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    crate::platform::live_path::find_in(command, &directories)
        .map(|path| path.to_string_lossy().into_owned())
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
    ]);
    // nvm, fnm, npm's own prefix, bun and pnpm (CDXC:AgentHooks 2026-11-07 FIX at
    // `node_tool_dirs`): a Pi only the shell's startup put on PATH read as not installed, so its
    // chat never opened.
    entries.extend(ghostex_paths::tool_path::node_tool_dirs(home_dir));
    // Volta, nodenv, Nix/NixOS and standard tool folders so a standalone gxserver (not started by
    // the desktop app) finds the same tools as the desktop's PATH.
    entries.extend(ghostex_paths::tool_path::fallback_tool_dirs(home_dir));
    entries.extend([
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

/// An agent config's text: `Ok(None)` when the file does not exist, an error for anything else
/// (a refused read, or bytes that are not UTF-8), so a caller never takes an unreadable config
/// for an empty one and writes over it.
///
/// CDXC:AgentHooks 2026-11-18 WHY: review of c33dcd576: `read_file_text` turned every read error
/// into "", so a settings.json Ghostex could not read (UTF-16, invalid UTF-8, a passing error) was
/// replaced by `{}` plus the statusLine, losing the user's permissions, model and env. Only a
/// missing file reads as empty, as in packages/paths/src/settings_file.rs.
pub(crate) fn read_config_text(path: &Path) -> std::io::Result<Option<String>> {
    match fs::read(path) {
        // A leading UTF-8 byte order mark (some Windows editors add one) is not part of the text.
        Ok(bytes) => String::from_utf8(bytes)
            .map(|text| {
                Some(
                    text.strip_prefix('\u{feff}')
                        .map(str::to_string)
                        .unwrap_or(text),
                )
            })
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Replaces an agent config in one step: through a symlink to the file it points at, with a temp
/// file beside that file renamed over it, keeping the file's permissions. A file nobody may write
/// (read-only) is left alone and reported as an error.
///
/// CDXC:AgentHooks 2026-11-18 WHY: review of c33dcd576: `fs::write` truncated the config before
/// writing it (a crash in between left it empty), replaced a symlinked config (a dotfiles repo)
/// with a plain file, and a rewrite through a temp file gave Cursor's 0600 cli-config.json, which
/// holds its login, the default 0644.
/// CDXC:AgentHooks 2026-11-18 WHY: review of 67902fb2b: a config the temp-and-rename can't serve
/// is written in place as before: a hard-linked one (a rename would split the link), a dangling
/// symlink, a folder that refuses new files, a bind-mounted file the rename is refused on. The
/// rename retries while Windows holds the file a moment, and a failed write removes only the temp
/// file it created itself.
pub(crate) fn write_config_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let target = match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => match fs::canonicalize(path) {
            Ok(target) => target,
            // A dangling link: writing through it creates the file it names.
            Err(_) => return fs::write(path, bytes),
        },
        _ => path.to_path_buf(),
    };
    let metadata = match fs::metadata(&target) {
        Ok(meta) => Some(meta),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if metadata
        .as_ref()
        .is_some_and(|meta| meta.permissions().readonly())
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is read-only", target.display()),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.as_ref().is_some_and(|meta| meta.nlink() > 1) {
            return fs::write(&target, bytes);
        }
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let permissions = metadata.map(|meta| meta.permissions());
    match replace_through_temp(&target, bytes, permissions) {
        Ok(()) => Ok(()),
        Err(Refused::TempOrRename) => fs::write(&target, bytes),
        Err(Refused::Write(error)) => Err(error),
    }
}

/// Why [`replace_through_temp`] failed: a temp file or rename the system refused for a reason an
/// in-place write gets past, or anything else (no space, a quota, a failed write), which would
/// fail in place too, after truncating the config.
enum Refused {
    TempOrRename,
    Write(std::io::Error),
}

/// A temp-file or rename failure an in-place write gets past: permission denied (a folder that
/// refuses new files), a busy file, or a rename across devices (a bind-mounted file).
fn in_place_gets_past(error: std::io::Error) -> Refused {
    const EXDEV: i32 = 18;
    let fallback = matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ResourceBusy
    ) || (cfg!(unix) && error.raw_os_error() == Some(EXDEV));
    if fallback {
        Refused::TempOrRename
    } else {
        Refused::Write(error)
    }
}

fn replace_through_temp(
    target: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<(), Refused> {
    use std::io::Write as _;
    let temp = temp_path_for(target);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        options.mode(permissions.as_ref().map_or(0o644, |p| p.mode() & 0o7777));
    }
    // Only a temp file this call created is ever removed.
    let mut file = options.open(&temp).map_err(in_place_gets_past)?;
    let written = (|| {
        file.write_all(bytes)?;
        file.sync_all()
    })();
    drop(file);
    let result = match written {
        Err(error) => Err(Refused::Write(error)),
        Ok(()) => permissions
            .map_or(Ok(()), |permissions| {
                fs::set_permissions(&temp, permissions)
            })
            .and_then(|()| {
                ghostex_paths::durable_file::retry_while_busy(|| fs::rename(&temp, target))
            })
            .map_err(in_place_gets_past),
    };
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
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

#[cfg(all(test, not(windows)))]
mod command_lookup_tests {
    use super::resolve_command_on_path;
    use std::os::unix::fs::PermissionsExt;

    fn write(path: &std::path::Path, mode: u32) {
        std::fs::write(path, "#!/bin/sh\n").expect("write");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    }

    #[test]
    fn the_first_executable_on_path_wins_like_command_v() {
        let root = tempfile::tempdir().expect("tempdir");
        let (first, second) = (root.path().join("first"), root.path().join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        write(&first.join("agent"), 0o644);
        write(&second.join("agent"), 0o755);
        std::fs::create_dir_all(first.join("folder")).unwrap();
        let path = format!("relative:{}:{}", first.display(), second.display());
        assert_eq!(
            resolve_command_on_path("agent", &path).as_deref(),
            Some(second.join("agent").to_str().unwrap())
        );
        write(&first.join("agent"), 0o755);
        assert_eq!(
            resolve_command_on_path("agent", &path).as_deref(),
            Some(first.join("agent").to_str().unwrap())
        );
        assert_eq!(resolve_command_on_path("folder", &path), None);
        assert_eq!(resolve_command_on_path("missing", &path), None);
        assert_eq!(resolve_command_on_path("  ", &path), None);
    }

    #[test]
    fn a_path_names_itself_when_it_is_executable() {
        let root = tempfile::tempdir().expect("tempdir");
        let tool = root.path().join("tool");
        write(&tool, 0o755);
        let text = tool.to_str().unwrap();
        assert_eq!(resolve_command_on_path(text, "").as_deref(), Some(text));
        write(&tool, 0o644);
        assert_eq!(resolve_command_on_path(text, ""), None);
    }
}
