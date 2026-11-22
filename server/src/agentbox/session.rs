//! The box a session runs in, read from `runtimeSettings.agentbox`.

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use super::location::{
    is_valid_box_name, provider_label, provider_spec, remote_docker_alias, AGENTBOX_AGENTS,
};
use crate::domain::DomainStateError;

/// `runtimeSettings.agentbox` of a box session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionAgentbox {
    /// `docker`, a cloud provider id, or `docker:<alias>`.
    pub(crate) provider: String,
    pub(crate) box_name: String,
    /// The agentbox agent: `claude`, `codex`, `opencode` or `pi`.
    pub(crate) agent: String,
    /// `agentbox list` has shown the box, so restores reattach instead of creating it.
    pub(crate) created: bool,
    /// The box's create command without the sign-in step, rerun until the box exists.
    pub(crate) launch_command: Option<String>,
}

impl SessionAgentbox {
    pub(crate) fn to_value(&self) -> Value {
        let mut value = json!({
            "agent": self.agent,
            "boxName": self.box_name,
            "provider": self.provider,
        });
        if self.created {
            value["created"] = json!(true);
        }
        if let Some(command) = &self.launch_command {
            value["launchCommand"] = json!(command);
        }
        value
    }
}

/// `runtimeSettings.agentbox.pending`: a draft whose Run on row picked this box. Nothing runs in a
/// box yet; the draft's first message creates it (`agents/draft_run_location.rs`).
pub(crate) const PENDING_KEY: &str = "pending";

/// The box a session runs in, or `None` for a session that runs on this computer. A record whose
/// box, agent or provider is not one Ghostex creates is not a box session, and neither is a draft
/// whose box only starts with its first message ([`pending_session_agentbox`]).
pub(crate) fn session_agentbox(session: &Value) -> Option<SessionAgentbox> {
    agentbox_record(session).and_then(|(agentbox, pending)| (!pending).then_some(agentbox))
}

/// The box a draft will start in with its first message, when the chat's Run on row picked one.
pub(crate) fn pending_session_agentbox(session: &Value) -> Option<SessionAgentbox> {
    agentbox_record(session).and_then(|(agentbox, pending)| pending.then_some(agentbox))
}

fn agentbox_record(session: &Value) -> Option<(SessionAgentbox, bool)> {
    let record = session.pointer("/runtimeSettings/agentbox")?.as_object()?;
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let box_name = text("boxName").filter(|name| is_valid_box_name(name))?;
    let agent = text("agent").filter(|agent| AGENTBOX_AGENTS.contains(&agent.as_str()))?;
    let provider = text("provider").filter(|provider| {
        provider_spec(provider).is_some() || remote_docker_alias(provider).is_some()
    })?;
    Some((
        SessionAgentbox {
            provider,
            box_name,
            agent,
            created: record.get("created").and_then(Value::as_bool) == Some(true),
            launch_command: text("launchCommand")
                .filter(|command| command.starts_with("agentbox ")),
        },
        record.get(PENDING_KEY).and_then(Value::as_bool) == Some(true),
    ))
}

/// Create params without a client-supplied `runtimeSettings.agentbox`.
pub(crate) fn without_client_agentbox_record(
    params: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut params = params.clone();
    if let Some(runtime_settings) = params
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        runtime_settings.remove("agentbox");
    }
    params
}

pub(crate) fn is_agentbox_session(session: &Value) -> bool {
    session_agentbox(session).is_some()
}

/// `PresentationSession.agentbox` (packages/gx-protocol/src/presentation.rs).
pub(crate) fn presentation_agentbox_value(session: &Value) -> Option<Value> {
    let (agentbox, pending) = agentbox_record(session)?;
    let mut value = json!({
        "boxName": agentbox.box_name,
        "provider": agentbox.provider,
        "providerLabel": provider_label(&agentbox.provider),
    });
    if pending {
        value["pending"] = json!(true);
    }
    Some(value)
}

/// Sets one key of a session's `runtimeSettings.agentbox` in a single statement, when `expected`
/// (if given) is the key's current string value. Returns whether the row changed.
///
/// CDXC:AgentBox 2026-10-01 WHY: the poller and the first-prompt claim own only these keys, so they patch them in place instead of reading and rewriting all runtime settings, which raced the hook, title and activity writers that rewrite the same JSON.
pub(crate) fn patch_agentbox_record(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    key: &str,
    value: &Value,
    expected: Option<&str>,
) -> Result<bool, DomainStateError> {
    let path = format!("$.agentbox.{key}");
    let changed = db
        .execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, ?1, json(?2)) \
             WHERE projectId = ?3 AND sessionId = ?4 \
             AND json_extract(runtimeSettingsJson, '$.agentbox.boxName') IS NOT NULL \
             AND (?5 IS NULL OR json_extract(runtimeSettingsJson, ?1) = ?5)",
            params![path, value.to_string(), project_id, session_id, expected],
        )
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite agentbox state error: {error}"),
        })?;
    Ok(changed > 0)
}

/// Refuses an operation a box session does not support yet.
///
/// CDXC:AgentBox 2026-10-01 WHY: fork, account switch and agent switch rebuild the agent's command from a local conversation and a local account, and a box session has neither: its conversation and sign-in live inside the box. They are refused with a clear sentence instead of starting a local agent beside the box.
pub(crate) fn refuse_for_agentbox_session(
    session: &Value,
    operation: &str,
) -> Result<(), DomainStateError> {
    if is_agentbox_session(session) {
        return Err(DomainStateError::bad_request(format!(
            "{operation} is not available yet for sessions that run in a box."
        )));
    }
    Ok(())
}
