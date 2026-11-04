use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{default_agent_icon_to_id, default_agent_session_title_name};

const NO_DAEMON_FLAG: &str = "--no-daemon";
/// Codex 0.156.0 added `--no-daemon`; older releases reject it as an unknown argument.
const FIRST_VERSION_WITH_NO_DAEMON: (u64, u64, u64) = (0, 156, 0);
const VERSION_CACHE_TTL: Duration = Duration::from_secs(60);

/// CDXC:AgentProviders 2026-09-28 DECISION:
/// User: every Codex launch, resume and fork runs with `--no-daemon` on all platforms, for Codex 0.156 and newer. Codex 0.157 turned on its shared background server by default, and that one process keeps the environment of whichever Ghostex terminal started it, so every other Codex's hooks and tool commands reported as that session and the newest Codex took over the first session's conversation (or reached another Ghostex instance's gxserver). Supersedes the 2026-09-26 PlatformSupport decision to keep Codex on its shared server on Windows.
/// SEE-ALSO: server/src/platform/standard_user.rs.
pub(crate) fn with_codex_no_daemon(agent_id: &str, icon: Option<&str>, command: &str) -> String {
    let is_codex = agent_id == "codex"
        || (default_agent_session_title_name(agent_id).is_none()
            && icon.and_then(default_agent_icon_to_id) == Some("codex"));
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    // `--remote` connects to a chosen server, which Codex refuses to combine with `--no-daemon`.
    if !is_codex
        || tokens.is_empty()
        || tokens
            .iter()
            .any(|token| *token == NO_DAEMON_FLAG || token.starts_with("--remote"))
        || !installed_codex_supports_no_daemon()
    {
        return command.to_string();
    }
    format!("{command} {NO_DAEMON_FLAG}")
}

/// Successful probes are cached for a minute; failed probes can retry on the next launch.
pub(crate) fn installed_codex_supports_no_daemon() -> bool {
    // Unit tests assert exact commands, which must not depend on the Codex installed on the machine.
    if cfg!(test) {
        return false;
    }
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    {
        let cached = CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((checked_at, supported)) = *cached {
            if checked_at.elapsed() < VERSION_CACHE_TTL {
                return supported;
            }
        }
    }
    // An install-on-demand wrapper is not run: the probe would install Codex (agent_hooks/on_demand.rs).
    #[cfg(windows)]
    let output = crate::platform::live_path::find("codex", &[])
        .and_then(|executable| {
            crate::agent_hooks::on_demand::probe_target_path(
                executable,
                "codex",
                &crate::resume_lookup::home_dir(),
            )
        })
        .and_then(|executable| {
            // Probe the native launcher that new sessions resolve, without PowerShell's stale PATH or .ps1 precedence.
            let mut command = crate::platform::process::background_command(executable);
            command.arg("--version");
            crate::agent_hooks::probing::run_command_stdout_with_timeout(
                command,
                Duration::from_secs(3),
            )
        });
    #[cfg(not(windows))]
    let output = {
        let home = crate::resume_lookup::home_dir();
        crate::agent_hooks::probing::resolve_probe_command("codex", &home).and_then(|codex| {
            crate::agent_hooks::probing::cli_script_stdout(
                &format!(
                    "{} --version",
                    crate::agent_hooks::plugin_sources::shell_quote(&codex)
                ),
                &home,
                Duration::from_secs(3),
            )
        })
    };
    let Some(version) = output.and_then(|output| parse_codex_version(&output)) else {
        return false;
    };
    let supported = version >= FIRST_VERSION_WITH_NO_DAEMON;
    let mut cached = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cached = Some((Instant::now(), supported));
    supported
}

/// `codex --version` prints `codex-cli 0.157.0`; prerelease suffixes such as `0.158.0-alpha.3` compare by their release numbers.
fn parse_codex_version(output: &str) -> Option<(u64, u64, u64)> {
    let version = output
        .split_whitespace()
        .find(|token| token.starts_with(|ch: char| ch.is_ascii_digit()))?;
    let mut parts = version
        .split(|ch: char| !ch.is_ascii_digit())
        .map(str::parse::<u64>);
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ))
}
