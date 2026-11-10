//! The guarded idle write for an agent that left without a Stop hook: the stored activity is
//! settled idle only while it is still the working state the caller observed.

use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::session_status::{compute_activity_update, effective_agent_activity_value};

/// The internal dispatch key of [`settle_gone_agent_activity`]. It has no HTTP route: only
/// gxserver's own reconcile (session_chat_agent_gone_activity.rs) dispatches it.
pub(crate) const SETTLE_GONE_AGENT_ACTIVITY_ENDPOINT: &str = "/api/settleGoneAgentActivity";

/// What the caller saw of the session before it decided the agent is gone.
pub(crate) struct GoneActivityGuard {
    /// The stored `runtimeSettings.agentActivity`, exactly as read.
    pub(crate) observed_activity: Value,
    /// The session row's `updatedAt`, exactly as read.
    pub(crate) observed_updated_at: String,
}

impl GoneActivityGuard {
    pub(crate) fn into_params(self, project_id: &str, session_id: &str) -> Map<String, Value> {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("observedAgentActivity".to_string(), self.observed_activity);
        params.insert(
            "observedUpdatedAt".to_string(),
            json!(self.observed_updated_at),
        );
        params
    }

    fn from_params(params: &Map<String, Value>) -> Result<Self, DomainStateError> {
        let observed_activity = params
            .get("observedAgentActivity")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or_else(|| {
                DomainStateError::bad_request("observedAgentActivity must be an object.")
            })?;
        let observed_updated_at = params
            .get("observedUpdatedAt")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                DomainStateError::bad_request("observedUpdatedAt must be a timestamp.")
            })?;
        Ok(Self {
            observed_activity,
            observed_updated_at,
        })
    }
}

/// Whether the stored activity, as presentation would read it now, is `working`.
pub(crate) fn stored_activity_is_working(session: &Value) -> bool {
    let raw = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"));
    effective_agent_activity_value(raw, "idle", chrono::Utc::now().timestamp_millis())
        .get("activity")
        .and_then(Value::as_str)
        == Some("working")
}

/// Runs inside the agent endpoint's write transaction (`dispatch_agent_endpoint`), so the read,
/// the comparison and the write are one step. A stale observation or a session that is no longer
/// working writes nothing and answers `changed: false` without `previousActivity`, so none of the
/// activity follow-ups run for it.
pub(crate) fn settle_gone_agent_activity(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let guard = GoneActivityGuard::from_params(params)?;
    let current = require_session(repository, lifecycle)?;
    let stored_activity = object_field(&current, "runtimeSettings")
        .get("agentActivity")
        .cloned()
        .unwrap_or(Value::Null);
    let stored_updated_at = current.get("updatedAt").and_then(Value::as_str);
    let reason = if stored_activity != guard.observed_activity
        || stored_updated_at != Some(guard.observed_updated_at.as_str())
    {
        Some("observation-stale")
    } else if !stored_activity_is_working(&current) {
        Some("not-working")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Ok(json!({
            "changed": false,
            "reason": reason,
            "session": current,
        }));
    }
    let mut activity_params = Map::new();
    activity_params.insert("projectId".to_string(), json!(lifecycle.project_id));
    activity_params.insert("sessionId".to_string(), json!(lifecycle.session_id));
    let mut update = compute_activity_update(&current, &activity_params, Some("terminalExited"));
    carry_session_chat_prompt(&current, &mut update.activity);
    let mut runtime_settings = object_field(&current, "runtimeSettings");
    runtime_settings.insert("agentActivity".to_string(), update.activity.clone());
    let mut session_update = lifecycle_update(lifecycle);
    session_update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if let Some(last_active_at) = update.last_active_at.clone() {
        session_update.insert("lastActiveAt".to_string(), json!(last_active_at));
    }
    let session = repository.update_session(&session_update)?;
    Ok(json!({
        "activity": update.activity,
        "changed": true,
        "enteredAttention": update.entered_attention,
        "previousActivity": update.previous_activity,
        "session": session,
    }))
}
