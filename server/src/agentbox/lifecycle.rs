//! What happens to a session's box when the session itself goes away.

use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use serde_json::Value;

use super::cli::{run_agentbox, STOP_TIMEOUT};
use super::session::session_agentbox;
use crate::domain::DomainRepository;

fn stopping_boxes() -> &'static Mutex<HashSet<String>> {
    static STOPPING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    STOPPING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Whether another session that is not closed runs in the same box (a session reopened from
/// Previous Sessions is a new row on the same box until the old row is deleted).
fn box_in_use_elsewhere(
    repository: &DomainRepository<'_>,
    session: &Value,
    box_name: &str,
) -> bool {
    let key = |row: &Value| {
        (
            row.get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string),
            row.get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string),
        )
    };
    repository
        .list_agentbox_sessions(false)
        .unwrap_or_default()
        .iter()
        .any(|row| {
            key(row) != key(session)
                && row.get("lifecycleState").and_then(Value::as_str) != Some("stopped")
                && session_agentbox(row).is_some_and(|agentbox| agentbox.box_name == box_name)
        })
}

/// Stops the box of a session that was closed or deleted, in the background.
///
/// CDXC:AgentBox 2026-10-01 WHY: a closed or deleted session leaves nothing that shows or reattaches to its box, so the box is stopped (not destroyed): its work is kept, Docker frees the CPU and memory, and reopening the session from Previous Sessions restarts it through `agentbox <agent> attach`. Called from the one place every close passes (`kill_and_cache_session_provider` with a `stopped` target: sidebar close, Running Sessions, close-after-done, `ghostex kill`) and from delete; never from sleep, because waking only reattaches. A box another open session still uses (the reopened copy of a closed one) is left running. A cloud VM may keep billing until it is destroyed, which stays an explicit user action.
pub(crate) fn stop_session_box_in_background(repository: &DomainRepository<'_>, session: &Value) {
    let Some(agentbox) = session_agentbox(session) else {
        return;
    };
    if box_in_use_elsewhere(repository, session, &agentbox.box_name) {
        return;
    }
    let Ok(home) = crate::accounts::launch::home() else {
        return;
    };
    let Ok(mut stopping) = stopping_boxes().lock() else {
        return;
    };
    if !stopping.insert(agentbox.box_name.clone()) {
        return;
    }
    drop(stopping);
    std::thread::spawn(move || {
        if let Err(error) = run_agentbox(&home, &["stop", &agentbox.box_name], STOP_TIMEOUT, None)
            .and_then(|output| {
                output
                    .success
                    .then_some(())
                    .ok_or_else(|| output.failure_message())
            })
        {
            eprintln!("agentbox stop {} failed: {error}", agentbox.box_name);
        }
        if let Ok(mut stopping) = stopping_boxes().lock() {
            stopping.remove(&agentbox.box_name);
        }
    });
}
