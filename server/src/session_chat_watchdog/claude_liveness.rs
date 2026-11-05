use super::*;

// ---------------------------------------------------------------------------
// Claude liveness evidence
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClaudeAgentLiveness {
    Alive,
    Exited,
    /// No registry to read, so the question was never answered — say nothing.
    Unknown,
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Every live Claude CLI keeps `~/.claude/sessions/<pid>.json` describing itself
(pid, sessionId, cwd, status). gxserver knows the session's agentSessionId, so a
matching record whose pid is gone — or no record at all on a machine that keeps
this registry — is the only hard evidence the server can get that the agent
process itself died, since zmx only knows whether the PANE exists.

`updatedAt` is deliberately NOT used as the freshness test: it is written on
status changes, not as a heartbeat, so a busy Claude routinely shows minutes of
"staleness" and a time-based rule would invent exits. The pid is the truth.

Bounded on purpose: read only at watchdog escalation, never in the fingerprint
loop or a frame path.
*/
pub(super) fn probe_claude_agent_liveness(
    agent: Option<&str>,
    agent_session_id: Option<&str>,
) -> ClaudeAgentLiveness {
    if !matches!(agent.map(str::trim), Some("claude" | "openclaude")) {
        return ClaudeAgentLiveness::Unknown;
    }
    let Some(agent_session_id) = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ClaudeAgentLiveness::Unknown;
    };
    let mut registry_seen = false;
    let mut exited_record_seen = false;
    let mut scanned = 0usize;
    let mut truncated = false;
    for directory in claude_session_registry_dirs() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if scanned >= CLAUDE_REGISTRY_SCAN_LIMIT {
                truncated = true;
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            scanned += 1;
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(record) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if record.get("pid").and_then(Value::as_u64).is_none() {
                continue;
            }
            registry_seen = true;
            if record.get("sessionId").and_then(Value::as_str) != Some(agent_session_id) {
                continue;
            }
            // CDXC:SessionIdentity 2026-11-05: a record whose pid a later process took is a dead
            // Claude's leftover (agent_hooks/claude_session_record.rs), and the conversation's
            // current process (a resume, another account) may have its own record further on.
            // Windows keeps never claiming an exit from here, as before.
            if cfg!(not(unix)) || crate::agent_hooks::claude_session_record_live(&record) {
                return ClaudeAgentLiveness::Alive;
            }
            exited_record_seen = true;
        }
    }
    if exited_record_seen {
        return ClaudeAgentLiveness::Exited;
    }
    /*
    No record for our session. That only means "exited" for stock `claude`,
    whose registry we just proved this machine keeps: an `openclaude` fork may
    simply not write one, and the neighbouring stock entries would then frame it
    for an exit it never had. A scan cut short at the limit never saw the rest
    of the registry, so it proves nothing either.
    */
    if registry_seen && !truncated && agent.map(str::trim) == Some("claude") {
        ClaudeAgentLiveness::Exited
    } else {
        ClaudeAgentLiveness::Unknown
    }
}

/// Every Claude config dir Ghostex may have launched the agent under: the default home, each
/// `~/.claude-profiles/<profile>`, and each Claude Swap account.
/// CDXC:AgentScreenDetection 2026-10-09 WHY:
/// `cswap run <N> --share-history` shares `projects/` and `history.jsonl` but gives every account its own `CLAUDE_CONFIG_DIR` (`<swap root>/sessions/<N>-<email>/`), so its live record is in that folder's `sessions/`. Reading only `~/.claude/sessions` found the other sessions' records but never this one, and every send whose transcript stayed quiet for 10 seconds showed "Claude Code is no longer running in this terminal" for a Claude that was running (50 such cards on one Windows machine by 2026-10-09).
fn claude_session_registry_dirs() -> Vec<PathBuf> {
    let home = crate::resume_lookup::home_dir();
    let mut directories = vec![home.join(".claude").join("sessions")];
    for parent in [
        home.join(".claude-profiles"),
        crate::accounts::claude_resets::swap_root(&home).join("sessions"),
    ] {
        if let Ok(profiles) = std::fs::read_dir(parent) {
            for profile in profiles.flatten() {
                directories.push(profile.path().join("sessions"));
            }
        }
    }
    directories
}
