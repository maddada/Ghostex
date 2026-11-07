//! Claude hooks fired inside Claude Code's background daemon, routed to the Ghostex terminal
//! attached to their conversation.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::event_mapping::env_string;

/// What a Claude hook from Claude's daemon should do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClaudeDaemonHook {
    /// Not fired inside Claude's daemon (or a fork, which keeps its own rule): route as usual.
    NotDaemonHosted,
    /// Routed to the Ghostex terminal attached to this conversation.
    Attached,
    /// A daemon conversation no Ghostex terminal shows (a pre-warmed spare, a background agent
    /// nobody opened): it is not any session's agent.
    Unattached,
}

/*
CDXC:SessionIdentity 2026-10-16 WHY:
Claude Code 2.1.296 runs some conversations inside its own per-user daemon (`claude daemon run`, one `--bg-pty-host` per conversation) and attaches the terminal's `claude` to them: opening the agents view (←) on a conversation that has no messages yet and pressing Escape starts a new one there, as does attaching to a background agent. The daemon process inherits the environment of whichever Ghostex terminal started it, so those hooks named another session (GHOSTEX_GLOBAL_SESSION_REF), and the nested-agent rule dropped them anyway because their Claude does not hang below a zmx pane. The session kept the conversation id it was started with, its chat showed nothing, the send watchdog retyped a message it thought was lost, and an error retry sent a second continuation after the first had worked (throwaway G8yfk, 2026-10-15).
The daemon's hooks carry `CLAUDE_JOB_DIR` (`…/jobs/<job>`), and the terminal's own `claude` records the job it shows as `parkedJobId` in `<config>/sessions/<pid>.json`. That client hangs below the pane's `zmx run <name>` through shells only, so the hook is routed to that pane's session. A conversation no terminal is parked on (the daemon's pre-warmed spares, which fire SessionStart too, or a background agent nobody opened) reports nothing. A fork (`CLAUDE_CODE_SESSION_KIND=bg`) keeps the 2026-09-27 rule in notify_runtime.rs.
SEE-ALSO: server/src/agent_hooks/notify_runtime.rs (applies it before the nested-agent rule), server/src/agent_hooks/ancestor_session.rs (the pane walk), server/src/agents/identity/conflicts.rs (the unwritten-conversation guard, which lets the session adopt the new id).
*/
pub(crate) fn route_claude_daemon_hook(agent_key: &str) -> ClaudeDaemonHook {
    if !matches!(agent_key, "claude" | "openclaude")
        || env_string("CLAUDE_CODE_SESSION_KIND").as_deref() == Some("bg")
    {
        return ClaudeDaemonHook::NotDaemonHosted;
    }
    let Some(job) = env_string("CLAUDE_JOB_DIR").and_then(|dir| {
        Path::new(&dir)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }) else {
        return ClaudeDaemonHook::NotDaemonHosted;
    };
    let attached = parked_client(&claude_config_dir().join("sessions"), &job)
        .and_then(super::ancestor_session::zmx_session_owning_agent)
        .is_some_and(|zmx_name| super::ancestor_session::adopt_zmx_session_routing(&zmx_name));
    if attached {
        ClaudeDaemonHook::Attached
    } else {
        ClaudeDaemonHook::Unattached
    }
}

/// CDXC:SessionIdentity 2026-10-16 WHY:
/// A terminal's own `claude` stays alive while it shows a daemon conversation and still fires hooks under its own, older conversation id: opening the agents view over a conversation that needs input sends a `Notification` (`agent_needs_input`) from it. Taken as identity, that moved the session back to the conversation it had left (3acb8360 instead of b8c05580, throwaway G5abf on 2026-10-16), so a failed turn's retry read the wrong transcript. While its session record says it is parked on a job, its events still count but never name the conversation.
pub(crate) fn terminal_is_parked(agent_key: &str) -> bool {
    if !matches!(agent_key, "claude" | "openclaude") || env_string("CLAUDE_JOB_DIR").is_some() {
        return false;
    }
    let Some(pid) = env_string("CLAUDE_PID") else {
        return false;
    };
    let path = claude_config_dir()
        .join("sessions")
        .join(format!("{}.json", pid.trim()));
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .is_some_and(|record| {
            record["kind"] == "interactive"
                && record["parkedJobId"]
                    .as_str()
                    .is_some_and(|job| !job.trim().is_empty())
        })
}

fn claude_config_dir() -> PathBuf {
    env_string("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ghostex_paths::agent_homes::claude_default_home(&crate::resume_lookup::home_dir())
        })
}

/// The live interactive `claude` whose session record says it shows daemon job `job`, newest
/// first when (briefly, during a hand-over) two do.
fn parked_client(sessions_dir: &Path, job: &str) -> Option<u32> {
    fs::read_dir(sessions_dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let record: Value =
                serde_json::from_str(&fs::read_to_string(entry.path()).ok()?).ok()?;
            parked_on(&record, job)
        })
        .filter(|(pid, _)| process_alive(*pid))
        .max_by_key(|(_, updated_at)| *updated_at)
        .map(|(pid, _)| pid)
}

/// `(pid, updatedAt)` of a Claude session record that is a terminal parked on `job`.
fn parked_on(record: &Value, job: &str) -> Option<(u32, i64)> {
    (record["kind"] == "interactive" && record["parkedJobId"].as_str() == Some(job))
        .then_some(())?;
    let pid = u32::try_from(record["pid"].as_u64()?).ok()?;
    Some((pid, record["updatedAt"].as_i64().unwrap_or_default()))
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    pid > 1 && unsafe { libc::kill(pid as libc::pid_t, 0) } == 0
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{parked_on, terminal_is_parked};
    use serde_json::json;

    /// The records Claude 2.1.296 wrote during the 2026-10-16 reproduction, trimmed.
    /// The terminal pid 16335 parked on job f58a7ec8 sent `Notification` under its own id 4e5ce152.
    #[test]
    fn a_parked_terminal_does_not_name_the_conversation() {
        let dir = std::env::temp_dir().join(format!("gx-parked-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        std::fs::write(
            dir.join("sessions/16335.json"),
            r#"{"pid":16335,"sessionId":"4e5ce152-873b-47bf-9f24-5e56e0a59db9","kind":"interactive","parkedJobId":"f58a7ec8"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("sessions/16400.json"),
            r#"{"pid":16400,"sessionId":"9791f6b0-1ccf-4e08-a7c1-b9a1badfd84b","kind":"interactive"}"#,
        )
        .unwrap();
        std::env::set_var("CLAUDE_CONFIG_DIR", &dir);
        std::env::remove_var("CLAUDE_JOB_DIR");
        std::env::set_var("CLAUDE_PID", "16335");
        assert!(terminal_is_parked("claude"));
        std::env::set_var("CLAUDE_PID", "16400");
        assert!(!terminal_is_parked("claude"));
        assert!(!terminal_is_parked("codex"));
        std::env::remove_var("CLAUDE_PID");
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_terminal_parked_on_the_job_counts() {
        let terminal = json!({"pid": 69203, "sessionId": "19262612-b6dd-4b79-8a23-c258d0658184", "kind": "interactive", "parkedJobId": "54aed79c", "updatedAt": 1792099898273_i64});
        let hosted = json!({"pid": 69415, "sessionId": "54aed79c-7633-4f8e-88f1-861bbe54d04d", "kind": "bg", "jobId": "54aed79c", "updatedAt": 1792099898941_i64});
        let spare = json!({"pid": 62409, "sessionId": "78426f59-d9b7-4901-b93c-384a27ab4648", "kind": "bg", "jobId": "78426f59", "spare": true});
        let unparked = json!({"pid": 17250, "sessionId": "73da0b6a-f4ca-4d17-a67b-795be907e246", "kind": "interactive"});
        assert_eq!(
            parked_on(&terminal, "54aed79c"),
            Some((69203, 1792099898273))
        );
        assert_eq!(parked_on(&terminal, "78426f59"), None);
        for record in [hosted, spare, unparked] {
            assert_eq!(parked_on(&record, "54aed79c"), None);
            assert_eq!(parked_on(&record, "78426f59"), None);
        }
    }
}
