use crate::{
    agents,
    domain::{DomainRepository, DomainStateError},
};
use serde_json::{json, Map, Value};

/// CDXC:Drafts 2026-09-09 WHY:
/// Claude and Codex publish a conversation ID before receiving a prompt. Account changes must rebuild a draft's fresh launch even when that ID exists, or deleting its old plan leaves a sleeping draft with no agent command to reopen.
pub(crate) fn needs_fresh_launch(session: &Value) -> bool {
    agents::session_is_draft(session)
        || session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str)
            .is_none_or(|id| id.trim().is_empty())
}

pub(crate) fn rebuild_launch(
    repository: &DomainRepository<'_>,
    project: &Value,
    session: &Value,
    runtime: &mut Map<String, Value>,
    settings: &mut Map<String, Value>,
) -> Result<(), DomainStateError> {
    let is_draft = agents::session_is_draft(session);
    if is_draft {
        for key in ["agentSessionId", "agentSessionPath"] {
            runtime.remove(key);
        }
    }
    let draft_status = runtime
        .get(agents::FIRST_USER_INPUT_DRAFT_STATUS_KEY)
        .cloned();
    let params = json!({
        "agentId": session["agentId"],
        "draft": is_draft,
        "requireLaunchCommand": true,
        "runtimeSettings": runtime,
        "launchSettings": settings,
    });
    let fresh = agents::create_agent_session_params_for_project(
        repository.db,
        project,
        params.as_object().unwrap(),
    )?;
    *runtime = fresh["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    // CDXC:NativeApprovals 2026-11-25 WHY: GitHub #750, a create drops the approval level a
    // client sends, so an account change put a Native session back on its settings' level (Ask
    // under Keep default) and lost a level picked in the Mode menu. This is the session's own saved
    // level, read from its row, so it stays; a newly started Native host is handed it
    // (`pi_chat_channel.rs`).
    for key in [
        crate::acp::SESSION_ACP_APPROVAL_LEVEL_KEY,
        crate::acp::approval::SESSION_ACP_APPROVAL_LEVEL_PICKED_KEY,
    ] {
        if let Some(saved) = session.pointer(&format!("/runtimeSettings/{key}")) {
            runtime.insert(key.to_string(), saved.clone());
        }
    }
    *settings = fresh["launchSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    // Creation arms the initial input handoff; rebuilding must retain its receipt
    // so the original seed cannot overwrite the draft the user has since edited.
    if let Some(status) = draft_status {
        runtime.insert(agents::FIRST_USER_INPUT_DRAFT_STATUS_KEY.into(), status);
    }
    Ok(())
}

/// CDXC:Drafts 2026-09-09 DECISION:
/// User: account changes on drafts happen in the background like agent changes, keeping the chat page and its unsent input in place.
/// SEE-ALSO: agents/drafts.rs owns the shared in-place CLI switch sequence; apps/desktop/src/app/render/session_chat_and_drop_feedback.rs keeps the draft page visible.
pub(crate) fn prepare_live_switch(
    repository: &DomainRepository<'_>,
    session: &Value,
    settings: &mut Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    let lifecycle = crate::zmx::LifecycleParams {
        project_id: session["projectId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        session_id: session["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    };
    let (probe, _, _, _) = crate::zmx::probe_and_cache_session_provider(repository, &lifecycle)
        .map_err(|error| match error {
            crate::zmx::ZmxEndpointError::Domain(error) => error,
            crate::zmx::ZmxEndpointError::DependencyUnavailable(message) => {
                super::store::error(message)
            }
        })?;
    if probe.lifecycle_state != "exists" {
        return Ok(None);
    }
    let resolved = json!({"launchSettings": settings});
    let command = agents::draft_switch_reuse_command(resolved.as_object().unwrap())?;
    // This command is delivered to the existing shell below, so an attach must
    // not queue the same launch a second time.
    if let Some(runtime) = settings
        .get_mut("runtimeRelevant")
        .and_then(Value::as_object_mut)
    {
        runtime.insert("queueProviderStartupText".into(), json!(false));
    }
    Ok(Some(command))
}

/// The account switch inside the live pane of a session with no conversation yet: a draft, a
/// coordinator before Start, an agent that recorded no prompt (agents/no_conversation.rs). It holds
/// the same switch guard as the agent switch, so a coordinator's waiting first message is typed
/// into the new CLI's box only once the switch is done.
pub(crate) fn switch_in_live_provider(
    state: &crate::server::AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    command: &str,
) -> Result<(), DomainStateError> {
    let project_id = session["projectId"].as_str().unwrap_or_default();
    let session_id = session["sessionId"].as_str().unwrap_or_default();
    let launched_acp = crate::acp::acp_family_from_launch_command(command).map(|family| {
        (
            family,
            crate::acp::acp_approval_level_from_launch_command(command),
        )
    });
    let is_acp = crate::acp::session_is_acp(session) || launched_acp.is_some();
    let (typed_command, mut cleanup_paths) = if is_acp {
        let (prepared, guard_files) = super::restart::prepare_non_cli_in_place_launch_command(
            &state.paths,
            &state.auth_token,
            project_id,
            session_id,
            command.to_string(),
        )?;
        (format!(" {prepared}"), guard_files)
    } else {
        (
            command.to_string(),
            super::restart::TempFilesGuard(Vec::new()),
        )
    };
    let guard = agents::DraftAgentSwitch::begin(project_id, session_id)?;
    crate::session_chat_first_message::retype_after_agent_switch(
        repository.db,
        project_id,
        session_id,
    );
    let session = if let Some((family, approval_level)) = launched_acp.as_ref() {
        let mut runtime = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if crate::acp::apply_acp_runtime_settings(&mut runtime, family, *approval_level) {
            super::endpoint::update_session_changes(repository, session, &runtime, Map::new())?
        } else {
            session.clone()
        }
    } else {
        session.clone()
    };
    let updated = agents::arm_launch_activity_suppression(repository, &session)?;
    crate::session_chat_send::cancel_session_chat_sends(project_id, session_id);
    let completion = crate::session_chat_send::enqueue_session_write_sequence_with_completion(
        &updated,
        project_id,
        session_id,
        "draft-account-switch",
        agents::build_draft_agent_switch_steps(&typed_command),
        None,
    )?;
    // Disarm the early-error guard so the script and one-shot secret file self-unlink without
    // racing `acp-host` startup; daemon startup sweep cleans up any crash leftovers.
    cleanup_paths.disarm();
    guard.finish_after(completion);
    Ok(())
}
