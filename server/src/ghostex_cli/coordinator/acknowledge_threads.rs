//! `ghostex orchestrator acknowledge-threads`: what viewing an orchestrator does to its finished
//! threads on the phone (the desktop and the web build ask gxserver for the same from gx-core).

use serde_json::json;

use super::command::{resolve_session, server_flags, target_coordinator};
use crate::ghostex_cli::{
    agents,
    args::ParsedArgs,
    output::print_json,
    rpc::{call_gxserver_rpc, CliResult},
};

pub(super) fn acknowledge_threads(parsed: &ParsedArgs) -> CliResult<()> {
    let (coordinator, flags) = match parsed.rest.first() {
        Some(reference) => resolve_session(reference, &server_flags(&parsed.flags))?,
        None => target_coordinator(parsed)?,
    };
    let ids = json!({
        "projectId": agents::text(&coordinator, "projectId"),
        "sessionId": agents::text(&coordinator, "sessionId"),
    });
    let result = call_gxserver_rpc("/api/acknowledgeCoordinatorThreads", &ids, &flags)?;
    let acknowledged = result["acknowledged"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "orchestrator": agents::text(&coordinator, "globalRef"),
            "acknowledged": acknowledged,
        }));
    } else {
        println!(
            "Marked {} finished thread(s) of {} as seen.",
            acknowledged.len(),
            agents::text(&coordinator, "title")
        );
    }
    Ok(())
}
