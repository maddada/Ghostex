//! `ghostex orchestrator sleep-inactive-threads`: what an orchestrator row's Sleep Inactive
//! Threads does on the phone (the desktop resolves the same set in gx-core).

use std::time::Duration;

use serde_json::{json, Value};

use super::command::{resolve_session, server_flags, target_coordinator};
use crate::coordinators::inactive_thread_sessions;
use crate::ghostex_cli::{
    agents,
    args::ParsedArgs,
    output::{is_failed_cli_result, print_json},
    rpc::{call_gxserver_rpc, CliResult},
};

/// The pause between two sleeps, the desktop's `BULK_SLEEP_INTERVAL_MS`: terminal teardowns are
/// not run concurrently.
const SLEEP_PACING: Duration = Duration::from_millis(350);

pub(super) fn sleep_inactive_threads(parsed: &ParsedArgs) -> CliResult<()> {
    let (coordinator, flags) = match parsed.rest.first() {
        Some(reference) => resolve_session(reference, &server_flags(&parsed.flags))?,
        None => target_coordinator(parsed)?,
    };
    let key = (
        agents::text(&coordinator, "projectId").to_string(),
        agents::text(&coordinator, "sessionId").to_string(),
    );
    let snapshot = call_gxserver_rpc("/api/readPresentationSnapshot", &json!({}), &flags)?;
    let sessions = snapshot["snapshot"]["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let targets = inactive_thread_sessions(&sessions, &key);
    let mut slept: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();
    for (index, (project_id, session_id)) in targets.into_iter().enumerate() {
        if index > 0 {
            std::thread::sleep(SLEEP_PACING);
        }
        let ids = json!({ "projectId": project_id, "sessionId": session_id });
        match call_gxserver_rpc("/api/sleepSession", &ids, &flags) {
            Ok(result) if !is_failed_cli_result(&result) => slept.push(ids),
            _ => failed.push(ids),
        }
    }
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ok": failed.is_empty(),
            "orchestrator": agents::text(&coordinator, "globalRef"),
            "slept": slept,
            "failed": failed,
        }));
    } else if slept.is_empty() && failed.is_empty() {
        println!(
            "{} has no inactive threads to sleep.",
            agents::text(&coordinator, "title")
        );
    } else {
        println!(
            "Slept {} inactive thread(s) of {}{}.",
            slept.len(),
            agents::text(&coordinator, "title"),
            if failed.is_empty() {
                String::new()
            } else {
                format!("; {} could not be slept", failed.len())
            }
        );
    }
    Ok(())
}
