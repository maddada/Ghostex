use std::path::Path;

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;
use crate::session_status::compute_activity_update;
use super::*;

pub(crate) fn ingest_session_state_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    home_dir: &Path,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let observed_identity = align_observed_identity_with_launch_profile(
        &current,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: read_text(params, "agentName"),
            agent_session_id: read_text(params, "agentSessionId"),
            agent_session_path: read_text(params, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: read_text(params, "startupText"),
        }),
    );
    if launch_agent_mismatch(&current, observed_identity.agent_id.as_deref()) {
        return Ok(json!({
            "changed": false,
            "projection": project_session_title_projection(&current),
            "reason": "session-state-agent-mismatch",
            "session": current,
        }));
    }
    let (mut result, session) = apply_session_state_update(
        repository,
        lifecycle,
        params,
        SessionIdentityUpdateSource::Passive,
    )?;
    if result.get("reason").and_then(Value::as_str) == Some("passive-session-identity-conflict") {
        return Ok(Value::Object(result));
    }
    let reconciled = reconcile_agent_metadata_title(repository, lifecycle, home_dir, "pending")?;
    let mut session = reconciled.session.unwrap_or(session);
    if reconciled.changed {
        result.insert("changed".to_string(), Value::Bool(true));
        result.insert(
            "projection".to_string(),
            project_session_title_projection(&session),
        );
        result.insert("reason".to_string(), Value::String(reconciled.reason));
        result.insert("session".to_string(), session.clone());
    }
    let claimed = claim_first_prompt_auto_title(
        repository,
        &session,
        read_text(params, "firstUserMessage"),
        false,
    )?;
    if let Some(claimed_session) = claimed {
        session = claimed_session;
        result.insert("changed".to_string(), Value::Bool(true));
        result.insert(
            "projection".to_string(),
            project_session_title_projection(&session),
        );
        result.insert(
            "reason".to_string(),
            Value::String("first-prompt-auto-title-claimed".to_string()),
        );
        result.insert("session".to_string(), session);
    }
    Ok(Value::Object(result))
}

pub(crate) fn apply_live_process_session_identity(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent_id: Option<String>,
    mut agent_session_id: Option<String>,
    agent_session_path: Option<String>,
) -> Result<bool, DomainStateError> {
    // A manually launched Codex can publish its UUID title before the process
    // scan identifies the terminal as Codex. Consume that already-observed
    // identity now so either event ordering reaches the same agent session.
    if agent_session_id.is_none()
        && normalize_agent_id(agent_id.as_deref()).as_deref() == Some("codex")
    {
        let current = repository.get_session(project_id, session_id)?;
        let runtime_settings = current
            .as_ref()
            .map(|session| object_field(session, "runtimeSettings"))
            .unwrap_or_default();
        if read_text_from_map(&runtime_settings, "agentSessionId").is_none() {
            agent_session_id = runtime_settings
                .get("agentActivity")
                .and_then(Value::as_object)
                .and_then(|activity| read_text_from_map(activity, "lastTitle"))
                .and_then(|title| get_codex_session_id_from_title(&title));
        }
    }
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let mut params = Map::new();
    insert_optional_string(&mut params, "agentName", agent_id);
    insert_optional_string(&mut params, "agentSessionId", agent_session_id);
    insert_optional_string(&mut params, "agentSessionPath", agent_session_path);
    let (result, _) = apply_session_state_update(
        repository,
        &lifecycle,
        &params,
        SessionIdentityUpdateSource::LiveProcess,
    )?;
    Ok(result.get("changed").and_then(Value::as_bool) == Some(true))
}

/*
CDXC:SessionChatIdentity 2026-08-02:
Transcript-proven identity repair. Claude Code writes a NEW transcript on
compaction/resume and only an agent hook tells the daemon about it — background
job continuations never fire hooks, so the stored `agentSessionId` can point at
a conversation that stopped receiving turns days ago. The Session Chat follower
proves the successor from the transcripts themselves (own-id + lineage to the
stale id) and lands it HERE, through the very same passive update path a hook
observation uses, so chat, title generation, prompts and the CLI all follow one
identity instead of each surface guessing.

`expected_agent_session_id` makes the write compare-and-set: the follower read
the stale id some milliseconds ago, and a real hook observation that landed in
between must always win.
*/
pub(crate) fn apply_transcript_successor_session_identity(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    expected_agent_session_id: Option<&str>,
    agent_session_id: &str,
    agent_session_path: &str,
) -> Result<bool, DomainStateError> {
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let current = require_session(repository, &lifecycle)?;
    let stored_agent_session_id =
        read_text_from_map(&object_field(&current, "runtimeSettings"), "agentSessionId");
    if stored_agent_session_id.as_deref() != expected_agent_session_id {
        return Ok(false);
    }
    if stored_agent_session_id.as_deref() == Some(agent_session_id) {
        return Ok(false);
    }
    let mut params = Map::new();
    params.insert(
        "agentSessionId".to_string(),
        json!(agent_session_id.to_string()),
    );
    params.insert(
        "agentSessionPath".to_string(),
        json!(agent_session_path.to_string()),
    );
    let (result, _) = apply_session_state_update(
        repository,
        &lifecycle,
        &params,
        SessionIdentityUpdateSource::Passive,
    )?;
    if result.get("reason").and_then(Value::as_str) == Some("passive-session-identity-conflict") {
        return Ok(false);
    }
    Ok(read_text_from_map(
        &object_field(
            result.get("session").unwrap_or(&Value::Null),
            "runtimeSettings",
        ),
        "agentSessionId",
    )
    .as_deref()
        == Some(agent_session_id))
}

pub(crate) fn apply_created_session_identity(
    repository: &DomainRepository<'_>,
    session: &Value,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let lifecycle = LifecycleParams {
        project_id: read_text_value(session, "projectId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing projectId."))?,
        session_id: read_text_value(session, "sessionId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing sessionId."))?,
    };
    let runtime_settings = params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let launch_settings = params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut identity_params = Map::new();
    insert_optional_string(
        &mut identity_params,
        "agentName",
        read_text_value(session, "agentId").or_else(|| read_text(params, "agentId")),
    );
    insert_optional_string(
        &mut identity_params,
        "agentSessionId",
        read_text_from_map(&runtime_settings, "agentSessionId"),
    );
    insert_optional_string(
        &mut identity_params,
        "agentSessionPath",
        read_text_from_map(&runtime_settings, "agentSessionPath"),
    );
    insert_optional_string(
        &mut identity_params,
        "startupText",
        read_text_from_map(&runtime_settings, "startupText")
            .or_else(|| read_text_from_map(&launch_settings, "startupText")),
    );
    insert_optional_string(
        &mut identity_params,
        "titleSource",
        read_text_from_map(&runtime_settings, "titleSource"),
    );
    insert_optional_string(
        &mut identity_params,
        "title",
        read_text(params, "title").or_else(|| read_text_value(session, "title")),
    );
    let (_, updated) = apply_session_state_update(
        repository,
        &lifecycle,
        &identity_params,
        SessionIdentityUpdateSource::Lifecycle,
    )?;
    Ok(updated)
}

pub(crate) struct TerminalTitleIngestOutput {
    pub(crate) result: Value,
    pub(crate) schedule_presentation_delta: bool,
}

#[cfg(test)]
pub(crate) fn ingest_terminal_title_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<TerminalTitleIngestOutput, DomainStateError> {
    ingest_terminal_title_event_with_home(repository, lifecycle, params, Path::new(""))
}

pub(crate) fn ingest_terminal_title_event_with_home(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    home_dir: &Path,
) -> Result<TerminalTitleIngestOutput, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let raw_title = read_text(params, "rawTitle");
    let visible_title = raw_title.as_deref().and_then(get_visible_terminal_title);
    let title_detected_agent = raw_title
        .as_deref()
        .and_then(get_terminal_title_detected_agent_name);
    let mut runtime_settings = object_field(&current, "runtimeSettings");
    let decision_agent_name = read_text(params, "agentName")
        .or_else(|| read_text_from_map(&runtime_settings, "agentName"))
        .or_else(|| read_text_value(&current, "agentId"));
    let promotion_agent_name = read_text(params, "agentName").or(title_detected_agent.clone());
    let captured_agent_session_id = raw_title
        .as_deref()
        .and_then(get_codex_session_id_from_title)
        .filter(|_| normalize_agent_id(decision_agent_name.as_deref()).as_deref() == Some("codex"))
        .filter(|session_id| {
            read_text_from_map(&runtime_settings, "agentSessionId").as_deref()
                != Some(session_id.as_str())
        });
    /*
    CDXC:GxserverSessionIdentity 2026-06-21-18:25:
    Terminal-title agent/session-id observations must flow through the shared identity reducer, matching TypeScript gxserver. This keeps launch-agent mismatch protection and Codex thread conflict rules identical while still allowing zmx title streams to promote recognized CLI rows for every client.

    CDXC:GxserverSessionIdentity 2026-06-22-07:42:
    TypeScript returns the terminal-title reducer's decision reason for `/api/ingestTerminalTitleEvent`; the follow-up identity reducer only mutates the session and changed flag unless metadata reconciliation later wins. Preserve reasons such as `captured-agent-session-id` even when identity promotion reports `current-title-already-trusted`.

    CDXC:GxserverSessionTitles 2026-06-22-07:59:
    Match TypeScript terminal-title ingestion gates exactly: session kind, visible-title normalization, ellipsized rejection, protected trusted titles, zmx/agent-title trust, and previous title-source reasons all belong in gxserver-rs before status or identity promotion runs.
    */
    if let Some(agent_session_id) = captured_agent_session_id.clone() {
        runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    let mut decision_changed = captured_agent_session_id.is_some();
    let mut reason = terminal_title_skip_reason(
        &current,
        params,
        visible_title.as_deref(),
        &runtime_settings,
        captured_agent_session_id.as_deref(),
    );
    let mut update = lifecycle_update(lifecycle);
    if reason.is_none() {
        if let Some(title) = visible_title.clone() {
            let previous_title_source = session_title_source(&current, &runtime_settings);
            let sync_reason = terminal_title_sync_reason(
                &current,
                title.as_str(),
                decision_agent_name.as_deref(),
                read_text(params, "previousTerminalTitle").as_deref(),
                read_text(params, "sessionPersistenceProvider").as_deref(),
            );
            if let Some(sync_reason) = sync_reason {
                reason = Some(format!("{sync_reason}-from-{previous_title_source}"));
                runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
                decision_changed = true;
            } else {
                reason = Some("terminal-title-not-trusted".to_string());
            }
        }
    }
    if reason.as_deref().is_some_and(|value| {
        value.starts_with("valid-agent-terminal-title-from-")
            || value.starts_with("zmx-terminal-title-from-")
    }) {
        if let Some(title) = visible_title.clone() {
            update.insert("title".to_string(), json!(title));
        }
    }
    let should_update_title_decision = decision_changed;
    let mut session = if should_update_title_decision {
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update)?
    } else {
        current
    };
    let mut changed = decision_changed;
    let decision_changed_for_metadata = changed;
    if captured_agent_session_id.is_some() || title_detected_agent.is_some() {
        let mut identity_params = Map::new();
        insert_optional_string(&mut identity_params, "agentName", promotion_agent_name);
        insert_optional_string(
            &mut identity_params,
            "agentSessionId",
            captured_agent_session_id.clone(),
        );
        let (identity_result, identity_session) = apply_session_state_update(
            repository,
            lifecycle,
            &identity_params,
            SessionIdentityUpdateSource::TerminalTitle,
        )?;
        if identity_result
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            changed = true;
        }
        session = identity_session;
    }
    let mut activity_params = params.clone();
    if let Some(raw_title) = raw_title.clone() {
        activity_params.insert("title".to_string(), json!(raw_title));
    }
    insert_optional_string(
        &mut activity_params,
        "agentName",
        read_text(params, "agentName").or(title_detected_agent.clone()),
    );
    insert_optional_string(
        &mut activity_params,
        "settledTitle",
        read_agent_metadata_settled_title(&session),
    );
    let mut activity_update = compute_activity_update(&session, &activity_params, Some("title"));
    // Title observation must not erase a pending Session Chat card (a session
    // sitting on an AskUserQuestion produces no output, so title ticks keep
    // firing while the question waits).
    carry_session_chat_prompt(&session, &mut activity_update.activity);
    let should_update_activity = should_persist_activity_update(&session, &activity_update);
    if should_update_activity {
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert(
            "agentActivity".to_string(),
            activity_update.activity.clone(),
        );
        if let Some(last_active_at) = activity_update.last_active_at.clone() {
            update.insert("lastActiveAt".to_string(), json!(last_active_at));
        }
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        session = repository.update_session(&update)?;
    }
    /*
    CDXC:GxserverAgentTitles 2026-08-04:
    zmx terminal-title observation is the cross-platform notification source
    for Agent CLI `/rename`. WSL Codex may replace the renamed title almost
    immediately with a generic repository/spinner title, which is correctly
    rejected as display text above. That rejection must not suppress the
    canonical Codex metadata reconciliation: every settled zmx title event is
    still evidence that the CLI title state may have changed.
    */
    let should_check_metadata = decision_changed_for_metadata
        || changed
        || read_text(params, "sessionPersistenceProvider").as_deref() == Some("zmx");
    let reconciled = if should_check_metadata {
        Some(reconcile_agent_metadata_title(
            repository, lifecycle, home_dir, "pending",
        )?)
    } else {
        None
    };
    let reconciled_changed = reconciled.as_ref().is_some_and(|result| result.changed);
    let reconciled_reason = reconciled.as_ref().map(|result| result.reason.clone());
    if let Some(reconciled_session) = reconciled.and_then(|result| result.session) {
        session = reconciled_session;
    }
    let response_reason = if reconciled_changed {
        reconciled_reason.unwrap_or_else(|| "metadata-title-applied".to_string())
    } else {
        reason.unwrap_or_else(|| "terminal-title-not-visible".to_string())
    };
    let response_changed = changed || reconciled_changed;
    let result = json!({
        "agentSessionId": captured_agent_session_id,
        "activity": activity_update.activity,
        "changed": response_changed,
        "enteredAttention": activity_update.entered_attention,
        "previousActivity": activity_update.previous_activity,
        "projection": project_session_title_projection(&session),
        "reason": response_reason,
        "session": session,
        "visibleTitle": visible_title,
    });
    let result_activity = result
        .get("activity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let schedule_presentation_delta =
        response_changed || activity_update.previous_activity != result_activity;
    Ok(TerminalTitleIngestOutput {
        result,
        schedule_presentation_delta,
    })
}

pub(crate) fn terminal_title_skip_reason(
    session: &Value,
    params: &Map<String, Value>,
    visible_title: Option<&str>,
    runtime_settings: &Map<String, Value>,
    captured_agent_session_id: Option<&str>,
) -> Option<String> {
    if !matches!(
        session.get("kind").and_then(Value::as_str),
        Some("terminal" | "agent")
    ) {
        return Some("invalid-session-kind".to_string());
    }
    let Some(visible_title) = visible_title else {
        return Some(
            if captured_agent_session_id.is_some() {
                "captured-agent-session-id"
            } else {
                "terminal-title-not-visible"
            }
            .to_string(),
        );
    };
    if is_ellipsized_terminal_window_title(visible_title) {
        return Some("terminal-title-already-ellipsized".to_string());
    }
    if params
        .get("protectStoredTitleFromAutomation")
        .and_then(Value::as_bool)
        == Some(true)
        && trusted_resume_title_with_runtime(session, runtime_settings).is_some()
    {
        return Some("protected-stored-title".to_string());
    }
    if session
        .get("title")
        .and_then(Value::as_str)
        .is_some_and(|title| title.trim() == visible_title)
    {
        return Some(
            if captured_agent_session_id.is_some() {
                "captured-agent-session-id"
            } else {
                "already-synced"
            }
            .to_string(),
        );
    }
    None
}

pub(crate) fn terminal_title_sync_reason(
    session: &Value,
    visible_title: &str,
    agent_name: Option<&str>,
    previous_terminal_title: Option<&str>,
    session_persistence_provider: Option<&str>,
) -> Option<&'static str> {
    if is_valid_agent_terminal_title(visible_title, agent_name) {
        return Some("valid-agent-terminal-title");
    }
    let provider = session_persistence_provider.unwrap_or("zmx");
    if provider != "off" && !is_rejected_resume_title(visible_title) {
        return Some("zmx-terminal-title");
    }
    let previous_visible = previous_terminal_title.and_then(get_visible_terminal_title);
    if previous_visible.as_deref().is_some_and(|previous| {
        session
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|title| title.trim() == previous)
    }) {
        return None;
    }
    None
}

pub(crate) fn session_title_source(session: &Value, runtime_settings: &Map<String, Value>) -> String {
    normalize_title_source(
        read_text_from_map(runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(runtime_settings, "restoreTitleSource"))
            .as_deref(),
        session
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

pub(crate) fn trusted_resume_title_with_runtime(
    session: &Value,
    runtime_settings: &Map<String, Value>,
) -> Option<String> {
    let title = read_text_value(session, "title")?;
    if session_title_source(session, runtime_settings) == "placeholder"
        || is_temporary_title(&title)
    {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_resume_title(&visible)).then_some(visible)
}

pub(crate) fn read_agent_metadata_settled_title(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    if read_text_from_map(&runtime_settings, "titleMetadataSource").as_deref()
        != Some("agent-metadata")
    {
        return None;
    }
    let title = read_text_value(session, "title")?;
    get_visible_terminal_title(&title).map(|value| value.trim().to_string())
}

pub(crate) fn is_ellipsized_terminal_window_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.ends_with('\u{2026}') || trimmed.ends_with("...")
}

pub(crate) fn is_valid_agent_terminal_title(title: &str, agent_name: Option<&str>) -> bool {
    supports_terminal_title_session_sync(agent_name)
        && title.trim().chars().count() > 1
        && title.chars().any(|ch| ch.is_alphanumeric())
        && get_visible_terminal_title(title).is_some()
        && !is_rejected_resume_title(title)
}

pub(crate) fn supports_terminal_title_session_sync(agent_name: Option<&str>) -> bool {
    let Some(normalized) = agent_name.map(|value| value.trim().to_ascii_lowercase()) else {
        return false;
    };
    matches!(
        normalized.as_str(),
        "antigravity"
            | "claude"
            | "codex"
            | "copilot"
            | "cursor"
            | "gemini"
            | "hermes-agent"
            | "opencode"
            | "pi"
            | "qoder"
            | "rovodev"
            | "claude code"
            | "codex cli"
            | "agy"
            | "antigravity cli"
            | "cursor agent"
            | "cursor cli"
            | "cursor-agent"
            | "github copilot"
            | "hermes"
            | "hermes agent"
            | "open code"
            | "qodercli"
            | "rovo"
            | "rovo dev"
            | "\u{03c0}"
    )
}

