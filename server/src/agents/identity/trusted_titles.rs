use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;
use crate::session_status::parse_iso_ms;

pub(crate) struct TrustedTitleCandidate {
    pub(super) reason: String,
    pub(super) title: String,
    pub(super) title_source: String,
    updated_at: Option<String>,
    /// True when the candidate provably belongs to this session's own
    /// conversation (matched by agent session id, or the event carried it).
    /// Path-only matches are NOT proof for agents whose store is one shared
    /// file, so their titles must never pin as user-sourced.
    pub(super) same_conversation: bool,
}

/// `project_sessions` is only hydrated once the event itself carried no trusted
/// title, i.e. exactly when the sibling-session scan below actually runs.
pub(crate) fn select_trusted_title_for_identity(
    project: &Value,
    project_sessions: &mut LazyProjectSessions<'_, '_>,
    current_session: &Value,
    event_title: Option<&Value>,
    event_title_source: Option<&Value>,
    event_agent_session_id: Option<&str>,
    identity: &ResolvedIdentity,
) -> Result<Option<TrustedTitleCandidate>, DomainStateError> {
    let same_conversation = event_agent_session_id.is_some()
        && event_agent_session_id == identity.agent_session_id.as_deref();
    if let Some(candidate) = create_trusted_title_candidate(
        event_title,
        event_title_source,
        "event-title",
        None,
        same_conversation,
    ) {
        return Ok(Some(candidate));
    }

    let current_session_id = read_text_value(current_session, "sessionId");
    /*
    CDXC:SessionIdentity 2026-09-11 WHY:
    `live_process_identity_update_is_noop` deliberately re-runs this pass on
    every poll while the title is still a placeholder, so this hunt must not
    hydrate the whole project. The rows are narrowed in SQL to the ones that
    share the agent session id or path; `identities_match_strength` still
    decides, including the shared-store carve-out.
    */
    let identity_sessions = project_sessions.matching_identity(identity)?;
    let live_candidate = select_newest_candidate(
        identity_sessions
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
                let match_strength = identities_match_strength(identity, &candidate_identity)?;
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
                    same_conversation: match_strength == IdentityMatchStrength::ConversationId,
                })
            })
            .collect(),
    );
    if live_candidate.is_some() {
        return Ok(live_candidate);
    }

    Ok(select_newest_candidate(
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
    ))
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
    let match_strength = identities_match_strength(identity, &candidate_identity)?;
    let same_conversation = match_strength == IdentityMatchStrength::ConversationId;

    let updated_at = read_text_from_record(record, "lastInteractionAt")
        .or_else(|| read_text_from_record(record, "closedAt"));
    if let Some(session_record) = session_record {
        if let Some(candidate) = create_trusted_title_candidate(
            session_record.get("title"),
            session_record.get("titleSource"),
            "previous-session-record-title",
            updated_at.clone(),
            same_conversation,
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
        same_conversation,
    )
    .or_else(|| {
        create_trusted_title_candidate(
            record.get("terminalTitle"),
            Some(&json!("terminal-auto")),
            "previous-session-terminal-title",
            updated_at,
            same_conversation,
        )
    })
}

pub(crate) fn create_trusted_title_candidate(
    title: Option<&Value>,
    title_source: Option<&Value>,
    reason: &str,
    updated_at: Option<String>,
    same_conversation: bool,
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
        same_conversation,
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
