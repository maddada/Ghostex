use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Pi 1.0.0 added `--session-id`; older releases reject it as an unknown option.
const FIRST_VERSION_WITH_SESSION_ID: (u64, u64, u64) = (1, 0, 0);
const VERSION_CACHE_TTL: Duration = Duration::from_secs(60);

/// CDXC:SessionIdentity 2026-10-06 DECISION:
/// User: "yes, pass the session ID at launch so restore/history work from the first message." A new local Pi session gets its id from Ghostex: the id is stored as the session's `agentSessionId` when the row is created and the launch runs `pi --session-id <id>`, so resume, chat and history know the conversation before Pi or its hooks report anything. A command that already chooses a session (`--session`, `--session-id`, `--fork`, `--continue`, `--resume`, `--no-session`), an AgentBox launch and a Pi older than 1.0 keep launching as before, and the hooks report the id.
/// SEE-ALSO: create_agent_session_params_for_project in server/src/agents/launch_plan.rs, build_pi_resume_command in server/src/agents/resume_plan.rs.
pub(crate) fn mint_pi_session_id(command: &str) -> Option<String> {
    if command_chooses_pi_session(command) || !installed_pi_supports_session_id() {
        return None;
    }
    Some(uuid::Uuid::new_v4().to_string())
}

/// `command` with Pi told which session id to use.
pub(crate) fn with_pi_session_id(command: &str, id: &str) -> String {
    format!("{} --session-id {id}", command.trim_end())
}

/// Whether `id` is a session id Pi accepts for `--session-id`.
pub(crate) fn is_pi_session_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    let edge = |byte: &u8| byte.is_ascii_alphanumeric();
    !bytes.is_empty()
        && bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn command_chooses_pi_session(command: &str) -> bool {
    let mut offset = 0;
    let mut skip_value = false;
    while !command[offset..].trim().is_empty() {
        let Some((start, end, word)) = super::command_word(command, offset) else {
            // Unfinished quoting: leave the command alone.
            return true;
        };
        offset = end;
        if skip_value {
            skip_value = false;
            continue;
        }
        if !super::is_option_word(&command[start..end], &word) {
            continue;
        }
        let flag = word.split_once('=').map_or(word.as_str(), |(flag, _)| flag);
        if matches!(
            flag,
            "--session"
                | "--session-id"
                | "--fork"
                | "--continue"
                | "-c"
                | "--resume"
                | "-r"
                | "--no-session"
        ) {
            return true;
        }
        skip_value = !word.contains('=') && super::option_takes_value("pi", &word);
    }
    false
}

/// Successful probes are cached for a minute; a failed probe tries again on the next launch.
fn installed_pi_supports_session_id() -> bool {
    // Unit tests assert exact commands, which must not depend on the Pi installed on the machine.
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
    let home = crate::resume_lookup::home_dir();
    let Some(program) = crate::agent_hooks::probing::resolve_probe_command("pi", &home) else {
        return false;
    };
    let mut command = std::process::Command::new(program);
    command.arg("--version").current_dir(&home);
    let Some(version) = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(3),
    )
    .and_then(|output| parse_pi_version(&output)) else {
        return false;
    };
    let supported = version >= FIRST_VERSION_WITH_SESSION_ID;
    *CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((Instant::now(), supported));
    supported
}

/// `pi --version` prints `1.0.2`.
fn parse_pi_version(output: &str) -> Option<(u64, u64, u64)> {
    let version = crate::agent_cli::latest::version_in(output)?;
    let mut parts = version
        .split(|ch: char| !ch.is_ascii_digit())
        .map(str::parse::<u64>);
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ))
}
