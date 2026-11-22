//! Codex processes that stop an xswap login, mapped to the Ghostex sessions that own them.
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use serde_json::{json, Value};
#[cfg(unix)]
use std::collections::HashMap;
use std::collections::HashSet;

/// CDXC:AgentProviders 2026-09-28 SEE-ALSO:
/// xswap's refusal "Codex is running (PID …)" (codex-swap src/platform/codex_blockers.rs) is the contract; xswap 0.3.3 and older said "Codex is still running (PID …)".
pub(crate) fn pids_in_refusal(output: &str) -> Vec<u32> {
    ["Codex is running (PID ", "Codex is still running (PID "]
        .iter()
        .filter_map(|marker| {
            let start = output.rfind(marker)? + marker.len();
            let list = output[start..].split(')').next()?;
            Some(
                list.split(',')
                    .filter_map(|pid| pid.trim().parse().ok())
                    .collect::<Vec<u32>>(),
            )
        })
        .find(|pids| !pids.is_empty())
        .unwrap_or_default()
}

/// Ghostex sessions whose zmx daemon is an ancestor of a blocking process, plus the count of blocking processes outside any session (the Codex app, a plain terminal).
pub(crate) fn describe(state: &AppState, pids: &[u32]) -> Value {
    let owners = zmx_owners(pids);
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();
    let mut others = 0;
    let db = crate::storage::open_gxserver_database(&state.paths).ok();
    let repository = db
        .as_ref()
        .map(|db| DomainRepository::new(db, &state.metadata.server_id));
    for owner in owners {
        let found = owner
            .as_deref()
            .and_then(ids_from_zmx_name)
            .and_then(|(project, session)| {
                let row = repository.as_ref()?.get_session(project, session).ok()??;
                Some((project.to_string(), session.to_string(), row))
            });
        let Some((project, session, row)) = found else {
            others += 1;
            continue;
        };
        if !seen.insert((project.clone(), session.clone())) {
            continue;
        }
        let title = crate::presentation::session_attributes::project_session_title_projection(&row);
        let title = title["displayTitle"]
            .as_str()
            .or_else(|| row["title"].as_str())
            .unwrap_or("Codex session")
            .to_string();
        sessions.push(json!({"projectId": project, "sessionId": session, "title": title}));
    }
    json!({"sessions": sessions, "others": others})
}

/// CDXC:AgentProviders 2026-09-28 DECISION:
/// The user asked for a clear error that offers to end the running Codex sessions when an xswap login cannot run beside them. Settings lists the blocking sessions and offers "Sleep N sessions and continue": Ghostex sleeps its own sessions (they resume on wake) and retries with XSWAP_STOP_CODEX=always, so xswap ends any Codex outside Ghostex that Settings named.
pub(crate) fn sleep_sessions(state: &AppState, sessions: &[Value]) -> Result<(), DomainStateError> {
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(super::store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    for session in sessions.iter().take(64) {
        let (Some(project), Some(id)) =
            (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        if repository.get_session(project, id)?.is_some() {
            super::endpoint::cycle(state, &repository, session, "/api/sleepSession")?;
        }
    }
    Ok(())
}

fn ids_from_zmx_name(name: &str) -> Option<(&str, &str)> {
    let mut parts = name.split('-');
    let (server, project, session) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none()
        && server.starts_with('S')
        && project.starts_with('P')
        && session.starts_with('G'))
    .then_some((project, session))
}

/// The zmx session name (`zmx run <name>`) above each pid, found by walking parent processes.
#[cfg(unix)]
fn zmx_owners(pids: &[u32]) -> Vec<Option<String>> {
    let listing = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,command="])
        .env("LC_ALL", "C")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let table: HashMap<u32, (u32, &str)> = listing
        .lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (ppid, command) = rest.trim_start().split_once(char::is_whitespace)?;
            Some((
                pid.parse().ok()?,
                (ppid.parse().ok()?, command.trim_start()),
            ))
        })
        .collect();
    pids.iter()
        .map(|pid| {
            let mut current = *pid;
            for _ in 0..32 {
                let (parent, command) = table.get(&current)?;
                let mut words = command.split_whitespace();
                if words
                    .next()
                    .is_some_and(|exe| exe.ends_with("/zmx") || exe == "zmx")
                    && words.next() == Some("run")
                {
                    return words.next().map(str::to_string);
                }
                if *parent <= 1 {
                    return None;
                }
                current = *parent;
            }
            None
        })
        .collect()
}

/// wmx sessions are not mapped yet, so every blocking Codex counts as outside Ghostex.
#[cfg(not(unix))]
fn zmx_owners(pids: &[u32]) -> Vec<Option<String>> {
    vec![None; pids.len()]
}
