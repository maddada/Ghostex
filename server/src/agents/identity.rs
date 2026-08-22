use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;
use crate::session_status::parse_iso_ms;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionIdentityUpdateSource {
    Lifecycle,
    LiveProcess,
    Passive,
    TerminalTitle,
}

pub(crate) struct SessionIdentityConflict {
    agent_id: String,
    current_agent_session_id: Option<String>,
    incoming_agent_session_id: String,
    owner_project_id: Option<String>,
    owner_session_id: Option<String>,
    reason: &'static str,
    source: SessionIdentityUpdateSource,
}

pub(crate) fn apply_session_state_update(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    identity_update_source: SessionIdentityUpdateSource,
) -> Result<(Map<String, Value>, Value), DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let project_sessions = repository.list_sessions(Some(&lifecycle.project_id))?;
    let observed_identity = align_observed_identity_with_launch_profile(
        &session,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: read_text(params, "agentName"),
            agent_session_id: read_text(params, "agentSessionId"),
            agent_session_path: read_text(params, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: read_text(params, "startupText"),
        }),
    );
    if identity_update_source != SessionIdentityUpdateSource::LiveProcess
        && launch_agent_mismatch(&session, observed_identity.agent_id.as_deref())
    {
        let result = json!({
            "changed": false,
            "projection": project_session_title_projection(&session),
            "reason": "launch-agent-mismatch",
            "session": session.clone(),
        });
        return Ok((object_from_value(result), session));
    }
    let current_identity = resolve_stored_session_identity(&session);
    let resolved_identity = merge_observed_session_identity(&observed_identity, &current_identity);
    let (identity, identity_conflict) = resolve_allowed_session_identity(
        &current_identity,
        &session,
        &observed_identity,
        &resolved_identity,
        &project_sessions,
        identity_update_source,
    );
    if identity_conflict.is_some() && identity_update_source == SessionIdentityUpdateSource::Passive
    {
        let mut result = object_from_value(json!({
            "changed": false,
            "projection": project_session_title_projection(&session),
            "reason": "passive-session-identity-conflict",
            "session": session.clone(),
        }));
        if let Some(conflict) = identity_conflict {
            result.insert(
                "identityConflict".to_string(),
                session_identity_conflict_value(&conflict),
            );
        }
        return Ok((result, session));
    }

    let next_agent = identity
        .agent_id
        .clone()
        .or_else(|| read_text_value(&session, "agentId"));
    let mut runtime_settings = apply_session_identity_runtime_settings(
        &current_identity,
        &identity,
        object_field(&session, "runtimeSettings"),
        identity_update_source,
    );
    insert_truthy_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationAgent",
    );
    insert_optional_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationCommand",
    );
    insert_truthy_from_params(&mut runtime_settings, params, "firstUserMessage");

    let should_promote_agent = next_agent.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some();
    let mut title =
        read_text_value(&session, "title").unwrap_or_else(|| "Terminal Session".to_string());
    let mut reason = "identity-updated".to_string();
    let mut current_with_identity = session.clone();
    if let Some(object) = current_with_identity.as_object_mut() {
        if let Some(agent_id) = next_agent.clone() {
            object.insert("agentId".to_string(), json!(agent_id));
        }
        if should_promote_agent {
            object.insert("kind".to_string(), json!("agent"));
        }
        object.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings.clone()),
        );
    }

    if trusted_resume_title(&current_with_identity).is_none() {
        if let Some(candidate) = select_trusted_title_for_identity(
            &project,
            &project_sessions,
            &current_with_identity,
            params.get("title"),
            params.get("titleSource"),
            &identity,
        ) {
            title = candidate.title;
            runtime_settings.insert("titleSource".to_string(), json!(candidate.title_source));
            reason = candidate.reason;
        } else if let Some(agent_id) = next_agent.as_deref() {
            /*
            Plain terminals promoted by a live WSL agent process or its first
            hook should immediately gain the same neutral agent-aware title as
            sessions created from the agent launcher. Keep it a placeholder so
            first-prompt auto-title generation remains eligible to replace it.
            */
            title = create_agent_session_default_title(None, Some(agent_id));
            runtime_settings.insert("titleSource".to_string(), json!("placeholder"));
            reason = "agent-default-title-applied".to_string();
        }
    } else {
        reason = "current-title-already-trusted".to_string();
    }

    let mut update = lifecycle_update(lifecycle);
    if let Some(agent_id) = next_agent.clone() {
        update.insert("agentId".to_string(), json!(agent_id));
    }
    if should_promote_agent {
        update.insert("kind".to_string(), json!("agent"));
    }
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings.clone()),
    );
    update.insert("title".to_string(), json!(title));
    let needs_update = update.get("title") != session.get("title")
        || next_agent != read_text_value(&session, "agentId")
        || (should_promote_agent && session.get("kind").and_then(Value::as_str) != Some("agent"))
        || runtime_settings.get("agentName")
            != object_field(&session, "runtimeSettings").get("agentName")
        || runtime_settings.get("agentId")
            != object_field(&session, "runtimeSettings").get("agentId")
        || runtime_settings.get("agentSessionId")
            != object_field(&session, "runtimeSettings").get("agentSessionId")
        || runtime_settings.get("agentSessionPath")
            != object_field(&session, "runtimeSettings").get("agentSessionPath")
        || runtime_settings.get("launchAgentId")
            != object_field(&session, "runtimeSettings").get("launchAgentId")
        || runtime_settings.get("firstPromptTitleGenerationAgent")
            != object_field(&session, "runtimeSettings").get("firstPromptTitleGenerationAgent")
        || runtime_settings.get("firstPromptTitleGenerationCommand")
            != object_field(&session, "runtimeSettings").get("firstPromptTitleGenerationCommand")
        || runtime_settings.get("firstUserMessage")
            != object_field(&session, "runtimeSettings").get("firstUserMessage")
        || runtime_settings.get("agentActivity")
            != object_field(&session, "runtimeSettings").get("agentActivity")
        || runtime_settings.get("titleSource")
            != object_field(&session, "runtimeSettings").get("titleSource");
    let updated = if needs_update {
        repository.update_session(&update)?
    } else {
        current_with_identity
    };
    let mut result = object_from_value(json!({
        "changed": needs_update,
        "projection": project_session_title_projection(&updated),
        "reason": if needs_update { reason } else { "unchanged".to_string() },
        "session": updated.clone(),
    }));
    if let Some(conflict) = identity_conflict {
        result.insert(
            "identityConflict".to_string(),
            session_identity_conflict_value(&conflict),
        );
    }
    Ok((result, updated))
}

pub(crate) fn resolve_allowed_session_identity(
    current_identity: &ResolvedIdentity,
    current_session: &Value,
    observed_identity: &ResolvedIdentity,
    resolved_identity: &ResolvedIdentity,
    sessions: &[Value],
    source: SessionIdentityUpdateSource,
) -> (ResolvedIdentity, Option<SessionIdentityConflict>) {
    let observed_agent_id = normalize_agent_id(observed_identity.agent_id.as_deref());
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let resolved_agent_id = normalize_agent_id(resolved_identity.agent_id.as_deref());
    let incoming_agent_session_id = observed_identity
        .agent_session_id
        .as_deref()
        .and_then(normalize_codex_session_id);
    let current_codex_session_id = if current_agent_id.as_deref() == Some("codex") {
        current_identity
            .agent_session_id
            .as_deref()
            .and_then(normalize_codex_session_id)
    } else {
        None
    };
    let is_passive_codex_observation = source == SessionIdentityUpdateSource::Passive
        && incoming_agent_session_id.is_some()
        && (observed_agent_id.as_deref() == Some("codex")
            || (observed_agent_id.is_none()
                && current_agent_id.as_deref() == Some("codex")
                && resolved_agent_id.as_deref() == Some("codex")));
    if !is_passive_codex_observation {
        return (resolved_identity.clone(), None);
    }
    let incoming_agent_session_id = incoming_agent_session_id.expect("checked above");
    if let Some(current_codex_session_id) = current_codex_session_id {
        if current_codex_session_id != incoming_agent_session_id {
            let conflict = SessionIdentityConflict {
                agent_id: "codex".to_string(),
                current_agent_session_id: Some(current_codex_session_id),
                incoming_agent_session_id,
                owner_project_id: None,
                owner_session_id: None,
                reason: "passive-agent-session-id-replacement",
                source,
            };
            return (
                keep_current_session_identity(resolved_identity, current_identity),
                Some(conflict),
            );
        }
        return (resolved_identity.clone(), None);
    }
    if let Some(owner) =
        find_active_codex_identity_owner(sessions, current_session, &incoming_agent_session_id)
    {
        let conflict = SessionIdentityConflict {
            agent_id: "codex".to_string(),
            current_agent_session_id: None,
            incoming_agent_session_id,
            owner_project_id: read_text_value(&owner, "projectId"),
            owner_session_id: read_text_value(&owner, "sessionId"),
            reason: "active-agent-session-id-owned",
            source,
        };
        return (
            keep_current_session_identity(resolved_identity, current_identity),
            Some(conflict),
        );
    }
    (resolved_identity.clone(), None)
}

pub(crate) fn keep_current_session_identity(
    resolved_identity: &ResolvedIdentity,
    current_identity: &ResolvedIdentity,
) -> ResolvedIdentity {
    ResolvedIdentity {
        agent_id: resolved_identity.agent_id.clone(),
        agent_session_id: current_identity.agent_session_id.clone(),
        agent_session_path: current_identity.agent_session_path.clone(),
    }
}

pub(crate) fn merge_observed_session_identity(
    observed_identity: &ResolvedIdentity,
    current_identity: &ResolvedIdentity,
) -> ResolvedIdentity {
    let observed_agent_id = normalize_agent_id(observed_identity.agent_id.as_deref());
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let agent_changed = observed_agent_id.is_some()
        && current_agent_id.is_some()
        && observed_agent_id != current_agent_id;
    ResolvedIdentity {
        agent_id: observed_agent_id.or(current_agent_id),
        agent_session_id: observed_identity.agent_session_id.clone().or_else(|| {
            (!agent_changed)
                .then(|| current_identity.agent_session_id.clone())
                .flatten()
        }),
        agent_session_path: observed_identity.agent_session_path.clone().or_else(|| {
            (!agent_changed)
                .then(|| current_identity.agent_session_path.clone())
                .flatten()
        }),
    }
}

pub(crate) fn resolve_stored_session_identity(session: &Value) -> ResolvedIdentity {
    let runtime_settings = object_field(session, "runtimeSettings");
    let stored_identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: runtime_settings.clone(),
        startup_text: None,
    });
    let transcript_path_identity = resolve_session_identity(&IdentityInput {
        agent_id: None,
        agent_name: None,
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: Map::new(),
        startup_text: None,
    });
    merge_observed_session_identity(&transcript_path_identity, &stored_identity)
}

pub(crate) fn apply_session_identity_runtime_settings(
    current_identity: &ResolvedIdentity,
    identity: &ResolvedIdentity,
    mut runtime_settings: Map<String, Value>,
    source: SessionIdentityUpdateSource,
) -> Map<String, Value> {
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let next_agent_id = normalize_agent_id(identity.agent_id.as_deref());
    let agent_changed =
        current_agent_id.is_some() && next_agent_id.is_some() && current_agent_id != next_agent_id;
    let activity_agent_id = read_agent_activity_agent_id(runtime_settings.get("agentActivity"));
    let activity_owner_changed = next_agent_id.is_some()
        && activity_agent_id.is_some()
        && activity_agent_id != next_agent_id;
    if let Some(agent_id) = identity.agent_id.clone() {
        runtime_settings.insert("agentName".to_string(), json!(agent_id));
    }
    if source == SessionIdentityUpdateSource::LiveProcess {
        if let Some(agent_id) = next_agent_id.clone() {
            runtime_settings.insert("launchAgentId".to_string(), json!(agent_id));
        }
    }
    if let Some(agent_session_id) = identity.agent_session_id.clone() {
        runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    } else if agent_changed {
        runtime_settings.remove("agentSessionId");
    }
    if let Some(agent_session_path) = identity.agent_session_path.clone() {
        runtime_settings.insert("agentSessionPath".to_string(), json!(agent_session_path));
    } else if agent_changed {
        runtime_settings.remove("agentSessionPath");
    }
    if agent_changed {
        runtime_settings.remove("agentId");
    }
    if agent_changed || activity_owner_changed {
        runtime_settings.remove("agentActivity");
    }
    runtime_settings
}

pub(crate) fn read_agent_activity_agent_id(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("agentName"))
        .and_then(Value::as_str)
        .and_then(|value| normalize_agent_id(Some(value)))
}

pub(crate) fn find_active_codex_identity_owner(
    sessions: &[Value],
    current_session: &Value,
    incoming_agent_session_id: &str,
) -> Option<Value> {
    sessions.iter().find_map(|session| {
        if read_text_value(session, "sessionId") == read_text_value(current_session, "sessionId")
            && read_text_value(session, "projectId")
                == read_text_value(current_session, "projectId")
        {
            return None;
        }
        if !is_active_identity_owner(session) {
            return None;
        }
        let runtime_settings = object_field(session, "runtimeSettings");
        let identity = resolve_session_identity(&IdentityInput {
            agent_id: read_text_value(session, "agentId"),
            agent_name: None,
            agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
            agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
            runtime_settings,
            startup_text: None,
        });
        let is_match = normalize_agent_id(identity.agent_id.as_deref()).as_deref() == Some("codex")
            && identity
                .agent_session_id
                .as_deref()
                .and_then(normalize_codex_session_id)
                .as_deref()
                == Some(incoming_agent_session_id);
        is_match.then(|| session.clone())
    })
}

/// A session that could still be tailing the provider conversation it is bound
/// to. Stopped history rows are NOT owners — the registry keeps every session
/// ever created, so treating them as owners blocks legitimate re-binding.
pub(crate) fn is_active_identity_owner(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        || session.get("lifecycleState").and_then(Value::as_str) == Some("sleeping")
        || (session.get("lifecycleState").and_then(Value::as_str) != Some("stopped")
            && object_field(session, "providerState")
                .get("lifecycleState")
                .and_then(Value::as_str)
                == Some("exists"))
}

pub(crate) fn session_identity_conflict_value(conflict: &SessionIdentityConflict) -> Value {
    let mut output = Map::new();
    output.insert("agentId".to_string(), json!(conflict.agent_id));
    insert_optional_string(
        &mut output,
        "currentAgentSessionId",
        conflict.current_agent_session_id.clone(),
    );
    output.insert(
        "incomingAgentSessionId".to_string(),
        json!(conflict.incoming_agent_session_id),
    );
    insert_optional_string(
        &mut output,
        "ownerProjectId",
        conflict.owner_project_id.clone(),
    );
    insert_optional_string(
        &mut output,
        "ownerSessionId",
        conflict.owner_session_id.clone(),
    );
    output.insert("reason".to_string(), json!(conflict.reason));
    output.insert(
        "source".to_string(),
        json!(identity_update_source_name(conflict.source)),
    );
    Value::Object(output)
}

pub(crate) fn identity_update_source_name(source: SessionIdentityUpdateSource) -> &'static str {
    match source {
        SessionIdentityUpdateSource::Lifecycle => "lifecycle",
        SessionIdentityUpdateSource::LiveProcess => "live-process",
        SessionIdentityUpdateSource::Passive => "passive",
        SessionIdentityUpdateSource::TerminalTitle => "terminal-title",
    }
}

pub(crate) struct TrustedTitleCandidate {
    reason: String,
    title: String,
    title_source: String,
    updated_at: Option<String>,
}

pub(crate) fn select_trusted_title_for_identity(
    project: &Value,
    sessions: &[Value],
    current_session: &Value,
    event_title: Option<&Value>,
    event_title_source: Option<&Value>,
    identity: &ResolvedIdentity,
) -> Option<TrustedTitleCandidate> {
    if let Some(candidate) =
        create_trusted_title_candidate(event_title, event_title_source, "event-title", None)
    {
        return Some(candidate);
    }

    let current_session_id = read_text_value(current_session, "sessionId");
    let live_candidate = select_newest_candidate(
        sessions
            .iter()
            .filter(|session| read_text_value(session, "sessionId") != current_session_id)
            .filter_map(|session| {
                let runtime_settings = object_field(session, "runtimeSettings");
                let candidate_identity = ResolvedIdentity {
                    agent_id: read_text_value(session, "agentId")
                        .or_else(|| read_text_from_map(&runtime_settings, "agentName")),
                    agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
                    agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
                };
                if !identities_match(identity, &candidate_identity) {
                    return None;
                }
                let title = trusted_resume_title(session)?;
                Some(TrustedTitleCandidate {
                    reason: format!(
                        "matching-live-session:{}",
                        read_text_value(session, "sessionId").unwrap_or_default()
                    ),
                    title_source: normalize_title_source(
                        object_field(session, "runtimeSettings")
                            .get("titleSource")
                            .and_then(Value::as_str),
                        &title,
                    ),
                    updated_at: read_text_value(session, "lastActiveAt")
                        .or_else(|| read_text_value(session, "updatedAt")),
                    title,
                })
            })
            .collect(),
    );
    if live_candidate.is_some() {
        return live_candidate;
    }

    select_newest_candidate(
        project
            .get("previousSessionHistory")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| create_history_title_candidate(item, identity))
                    .collect()
            })
            .unwrap_or_default(),
    )
}

pub(crate) fn create_history_title_candidate(
    value: &Value,
    identity: &ResolvedIdentity,
) -> Option<TrustedTitleCandidate> {
    let record = value.as_object()?;
    let session_record = record.get("sessionRecord").and_then(Value::as_object);
    let hidden_record = record
        .get("hiddenRestoreMetadata")
        .and_then(Value::as_object)
        .and_then(|hidden| hidden.get("sessionRecord"))
        .and_then(Value::as_object);
    let candidate_identity = ResolvedIdentity {
        agent_id: read_text_from_record(record, "agentId")
            .or_else(|| read_text_from_record(record, "agentName"))
            .or_else(|| session_record.and_then(|item| read_text_from_record(item, "agentName")))
            .or_else(|| hidden_record.and_then(|item| read_text_from_record(item, "agentName")))
            .and_then(|value| normalize_agent_id(Some(&value))),
        agent_session_id: read_text_from_record(record, "agentSessionId")
            .or_else(|| {
                session_record.and_then(|item| read_text_from_record(item, "agentSessionId"))
            })
            .or_else(|| {
                hidden_record.and_then(|item| read_text_from_record(item, "agentSessionId"))
            }),
        agent_session_path: read_text_from_record(record, "agentSessionPath")
            .or_else(|| {
                session_record.and_then(|item| read_text_from_record(item, "agentSessionPath"))
            })
            .or_else(|| {
                hidden_record.and_then(|item| read_text_from_record(item, "agentSessionPath"))
            }),
    };
    if !identities_match(identity, &candidate_identity) {
        return None;
    }

    let updated_at = read_text_from_record(record, "lastInteractionAt")
        .or_else(|| read_text_from_record(record, "closedAt"));
    if let Some(session_record) = session_record {
        if let Some(candidate) = create_trusted_title_candidate(
            session_record.get("title"),
            session_record.get("titleSource"),
            "previous-session-record-title",
            updated_at.clone(),
        ) {
            return Some(candidate);
        }
    }
    create_trusted_title_candidate(
        record.get("primaryTitle"),
        Some(&json!(if record
            .get("isPrimaryTitleTerminalTitle")
            .and_then(Value::as_bool)
            == Some(true)
        {
            "terminal-auto"
        } else {
            "user"
        })),
        "previous-session-primary-title",
        updated_at.clone(),
    )
    .or_else(|| {
        create_trusted_title_candidate(
            record.get("terminalTitle"),
            Some(&json!("terminal-auto")),
            "previous-session-terminal-title",
            updated_at,
        )
    })
}

pub(crate) fn create_trusted_title_candidate(
    title: Option<&Value>,
    title_source: Option<&Value>,
    reason: &str,
    updated_at: Option<String>,
) -> Option<TrustedTitleCandidate> {
    let normalized_title = get_visible_terminal_title(title?.as_str()?)?
        .trim()
        .to_string();
    if normalized_title.is_empty() || is_rejected_resume_title(&normalized_title) {
        return None;
    }
    let normalized_source =
        normalize_title_source(title_source.and_then(Value::as_str), &normalized_title);
    if normalized_source == "placeholder" {
        return None;
    }
    Some(TrustedTitleCandidate {
        reason: reason.to_string(),
        title: normalized_title,
        title_source: normalized_source,
        updated_at,
    })
}

pub(crate) fn select_newest_candidate(
    mut candidates: Vec<TrustedTitleCandidate>,
) -> Option<TrustedTitleCandidate> {
    candidates.sort_by(|left, right| {
        timestamp_value(right.updated_at.as_deref())
            .cmp(&timestamp_value(left.updated_at.as_deref()))
    });
    candidates.into_iter().next()
}

pub(crate) fn timestamp_value(value: Option<&str>) -> i64 {
    value.and_then(parse_iso_ms).unwrap_or(0)
}

pub(crate) fn normalize_title_source(source: Option<&str>, title: &str) -> String {
    match source {
        Some("browser-auto") => "browser-auto".to_string(),
        Some("generated") => "generated".to_string(),
        Some("terminal-auto") => "terminal-auto".to_string(),
        Some("user") => "user".to_string(),
        Some("placeholder") => "placeholder".to_string(),
        _ if is_temporary_title(title) => "placeholder".to_string(),
        _ => "user".to_string(),
    }
}

pub(crate) fn read_text_from_record(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn identities_match(left: &ResolvedIdentity, right: &ResolvedIdentity) -> bool {
    let left_agent = normalize_agent_id(left.agent_id.as_deref());
    let right_agent = normalize_agent_id(right.agent_id.as_deref());
    if left_agent.is_some() && right_agent.is_some() && left_agent != right_agent {
        return false;
    }
    if left.agent_session_id.is_some()
        && right.agent_session_id.is_some()
        && left.agent_session_id == right.agent_session_id
    {
        return true;
    }
    left.agent_session_path.is_some()
        && right.agent_session_path.is_some()
        && left.agent_session_path == right.agent_session_path
}

pub(crate) fn normalize_codex_session_id(value: &str) -> Option<String> {
    is_uuid(value.trim()).then(|| value.trim().to_ascii_lowercase())
}

#[derive(Clone)]
pub(crate) struct IdentityInput {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) agent_session_path: Option<String>,
    pub(crate) runtime_settings: Map<String, Value>,
    pub(crate) startup_text: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct ResolvedIdentity {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) agent_session_path: Option<String>,
}

pub(crate) fn resolve_session_identity(input: &IdentityInput) -> ResolvedIdentity {
    let resume = parse_agent_resume_identity(input.startup_text.as_deref());
    let agent_session_path = input
        .agent_session_path
        .clone()
        .or_else(|| read_text_from_map(&input.runtime_settings, "agentSessionPath"));
    let agent_session_id = input
        .agent_session_id
        .clone()
        .or_else(|| read_text_from_map(&input.runtime_settings, "agentSessionId"))
        .or(resume.agent_session_id);
    let agent_id = normalize_agent_id(input.agent_id.as_deref())
        .or_else(|| normalize_agent_id(input.agent_name.as_deref()))
        .or_else(|| {
            normalize_agent_id(read_text_from_map(&input.runtime_settings, "agentName").as_deref())
        })
        .or_else(|| {
            normalize_agent_id(read_text_from_map(&input.runtime_settings, "agentId").as_deref())
        })
        .or_else(|| infer_agent_id_from_path(agent_session_path.as_deref()))
        .or(resume.agent_id);
    ResolvedIdentity {
        agent_id,
        agent_session_id,
        agent_session_path,
    }
}

pub(crate) fn parse_agent_resume_identity(text: Option<&str>) -> ResolvedIdentity {
    let text = text.unwrap_or_default();
    for (agent_id, needle) in [
        ("codex", "codex"),
        ("claude", "claude"),
        ("cursor", "cursor-agent"),
        ("opencode", "opencode"),
        ("pi", "pi"),
        ("kiro", "kiro-cli"),
        ("omp", "omp"),
    ] {
        let lower = text.to_ascii_lowercase();
        if !lower.contains(needle) {
            continue;
        }
        if let Some(reference) = quoted_or_next_resume_reference(text, needle) {
            return ResolvedIdentity {
                agent_id: Some(agent_id.to_string()),
                agent_session_id: Some(reference),
                agent_session_path: None,
            };
        }
    }
    ResolvedIdentity::default()
}

pub(crate) fn quoted_or_next_resume_reference(text: &str, command: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let index = lower.find(command)?;
    let tail = &text[index + command.len()..];
    let tokens = tail
        .split_whitespace()
        .map(|token| token.trim_matches(['"', '\'', '\r', '\n', ';']))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            *token,
            "resume" | "fork" | "--resume" | "--session" | "-s" | "--resume-id"
        ) {
            return tokens.get(index + 1).map(|value| (*value).to_string());
        }
    }
    None
}

pub(crate) fn normalize_agent_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase().replace('_', " ");
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let mapped = match normalized.as_str() {
        "codex" | "openai codex" | "codex cli" => "codex",
        "claude" | "claude code" => "claude",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "opencode" | "open code" => "opencode",
        "pi" | "π" => "pi",
        "omp" => "omp",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "amp" | "amp cli" => "amp",
        "copilot" | "github copilot" => "copilot",
        "droid" | "factory" | "factory droid" => "droid",
        "grok" | "grok build" => "grok",
        "kiro" | "kiro cli" | "kiro-cli" => "kiro",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        "codebuddy" | "code buddy" => "codebuddy",
        "qoder" | "qodercli" => "qoder",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                char
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

pub(crate) fn normalize_status_agent_name(value: Option<&str>) -> Option<String> {
    let agent = normalize_agent_id(value)?;
    matches!(
        agent.as_str(),
        "antigravity" | "claude" | "codex" | "copilot" | "cursor" | "gemini" | "opencode" | "pi"
    )
    .then_some(agent)
}

pub(crate) fn infer_agent_id_from_path(path: Option<&str>) -> Option<String> {
    let lower = path?.to_ascii_lowercase();
    if lower.contains("/.cursor/") && (lower.ends_with(".json") || lower.ends_with(".jsonl")) {
        return Some("cursor".to_string());
    }
    if lower.contains("/.claude/") && lower.ends_with(".jsonl") {
        return Some("claude".to_string());
    }
    if lower.contains("/.codex/") || lower.contains("/.codex-profiles/") {
        return Some("codex".to_string());
    }
    if lower.contains("/.opencode/") || lower.contains("/.config/opencode/") {
        return Some("opencode".to_string());
    }
    if lower.contains("/.pi/agent/") {
        return Some("pi".to_string());
    }
    None
}

/*
CDXC:GxserverSessionIdentity 2026-06-24-04:49:
Passive hook and sidecar events must not let stale Droid metadata replace a row gxserver already owns as Pi or another agent.
Treat stored agentId/runtime agentName as an identity lock when older rows do not yet have launchAgentId, while still allowing unowned terminal rows to be promoted by first matching observations.
*/
pub(crate) fn locked_session_agent_id(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    normalize_agent_id(
        runtime_settings
            .get("launchAgentId")
            .and_then(Value::as_str),
    )
    .or_else(|| normalize_agent_id(read_text_value(session, "agentId").as_deref()))
    .or_else(|| normalize_agent_id(runtime_settings.get("agentName").and_then(Value::as_str)))
    .or_else(|| {
        launch_settings
            .get("agentLaunchPlan")
            .and_then(Value::as_object)
            .and_then(|plan| {
                plan.get("agentCommand")
                    .and_then(Value::as_str)
                    .or_else(|| plan.get("command").and_then(Value::as_str))
            })
            .and_then(infer_agent_id_from_command)
    })
    .or_else(|| {
        launch_settings
            .get("startupText")
            .and_then(Value::as_str)
            .and_then(infer_agent_id_from_command)
    })
}

pub(crate) fn launch_agent_mismatch(session: &Value, incoming_agent_id: Option<&str>) -> bool {
    let Some(incoming) = normalize_agent_id(incoming_agent_id) else {
        return false;
    };
    locked_session_agent_id(session)
        .map(|locked| {
            locked != incoming
                && session_launch_agent_provider_id(session).as_deref() != Some(incoming.as_str())
        })
        .unwrap_or(false)
}

pub(crate) fn session_launch_agent_provider_id(session: &Value) -> Option<String> {
    normalize_agent_id(
        object_field(session, "launchSettings")
            .get("icon")
            .and_then(Value::as_str),
    )
}

pub(crate) fn align_observed_identity_with_launch_profile(
    session: &Value,
    mut identity: ResolvedIdentity,
) -> ResolvedIdentity {
    let observed_agent_id = normalize_agent_id(identity.agent_id.as_deref());
    if observed_agent_id.is_some() && observed_agent_id == session_launch_agent_provider_id(session)
    {
        if let Some(launch_agent_id) = locked_session_agent_id(session) {
            identity.agent_id = Some(launch_agent_id);
        }
    }
    identity
}

pub(crate) fn infer_agent_id_from_command(command: &str) -> Option<String> {
    let command = command.to_ascii_lowercase();
    for (agent, needle) in [
        ("cursor", "cursor-agent"),
        ("hermes-agent", "hermes"),
        ("codebuddy", "codebuddy"),
        ("antigravity", "agy"),
        ("opencode", "opencode"),
        ("rovodev", "rovodev"),
        ("qoder", "qodercli"),
        ("claude", "claude"),
        ("copilot", "copilot"),
        ("gemini", "gemini"),
        ("codex", "codex"),
        ("droid", "droid"),
        ("grok", "grok"),
        ("amp", "amp"),
        ("pi", "pi"),
    ] {
        if command
            .split(|char: char| {
                char.is_whitespace() || matches!(char, ';' | '&' | '|' | '(' | ')' | '/')
            })
            .any(|token| token == needle)
        {
            return Some(agent.to_string());
        }
    }
    None
}

pub(crate) fn is_agent_associated(session: &Value, identity: &ResolvedIdentity) -> bool {
    session.get("kind").and_then(Value::as_str) == Some("agent")
        || session.get("agentId").and_then(Value::as_str).is_some()
        || identity.agent_id.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some()
        || read_text_from_map(&object_field(session, "runtimeSettings"), "agentName").is_some()
}

