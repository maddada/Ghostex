//! The panel's read for a session whose agent is not Claude: the sections that agent can fill.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q555-Q557): every chat agent gets Environment, Changes, Context, Skills and Tasks wherever the data exists; Codex and OpenCode come first and get every section their own records can fill; a section with no data for an agent is left out, never shown empty. Environment and Changes are read for every agent before this (`http.rs`), Context and Tasks come from what the chat already reads (its context meter and task list), and this adds Skills for every agent the `$` picker knows, then Codex's and OpenCode's own sections.
//! SEE-ALSO: packages/gx-chat-core/src/extras/session_details/ (which sections each agent shows), server/src/session_details/codex.rs, server/src/session_details/opencode.rs.

use std::path::Path;

use serde_json::Value;

use crate::server::AppState;

/// The argv and environment of the session's live agent process, when it runs.
pub(super) fn live_launch(
    session: &Value,
    home: &Path,
) -> Option<(Vec<String>, Vec<(String, String)>)> {
    let name = crate::zmx::provider_zmx_session_name(session).ok()?;
    let process_id =
        crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(&name), home)
            .ok()?
            .get(&name)?
            .process_id?;
    crate::accounts::process_launch(process_id).map(|live| (live.arguments, live.environment))
}

/// Fills `result` with what `agent` can show beyond Environment and Changes. Blocking.
pub(super) fn read_agent_details(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
    folder: Option<&str>,
    agent: &str,
    result: &mut Value,
) {
    let folder_path = folder.map(Path::new);
    if let Some(skills) = super::picker_skills::picker_skills(&state.paths, agent, folder_path) {
        result["skills"] = skills;
    }
    match agent {
        "codex" => super::codex::read_codex_details(
            state,
            project_id,
            session_id,
            session,
            folder_path,
            result,
        ),
        "opencode" => super::opencode::read_opencode_details(session, folder, result),
        _ => {}
    }
}
