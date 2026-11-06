//! Autopilot's account switch: a session that sits at a usage limit its own policy does not switch
//! away from (Continue automatically off, or "Wait" at a limit) moves to the ready account with
//! the most room, the way the recovery pass picks one, and gets "." so it carries on.
//!
//! SEE-ALSO: server/src/server/autopilot_runtime.rs (when it is called), server/src/accounts/recovery.rs (the pass that switches on its own when the policy says so).

use serde_json::{json, Map, Value};

use super::{cli, launch, model::*, recovery, store};
use crate::{domain::DomainRepository, domain::DomainStateError, server::AppState};

/// What the switch did.
pub(crate) enum AutopilotSwitch {
    /// Moved to this account (its number and email).
    Moved(String),
    /// The policy switches on its own; Autopilot leaves it to the recovery pass.
    PolicySwitches,
    /// A session on its own login, or not a Claude or Codex session.
    NotSwitchable,
    /// No other account is signed in with room.
    NoRoom,
}

/// BLOCKING (an accounts read, then the in-place restart, up to 90 seconds).
pub(crate) fn switch_at_limit(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<AutopilotSwitch, DomainStateError> {
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(AutopilotSwitch::NotSwitchable);
    };
    let Some(project) = repository.get_project(project_id)? else {
        return Ok(AutopilotSwitch::NotSwitchable);
    };
    let Some(provider) = launch::provider(&project, &session) else {
        return Ok(AutopilotSwitch::NotSwitchable);
    };
    if launch::session_uses_own_login(provider, &session) {
        return Ok(AutopilotSwitch::NotSwitchable);
    }
    let policy = launch::effective_policy(&registry, provider, &session);
    if policy.enabled && policy.at_limit == LimitAction::Switch {
        return Ok(AutopilotSwitch::PolicySwitches);
    }
    let current = super::session_identity::display_account_id(
        &registry,
        provider,
        &session,
        &state.paths.home_dir,
    )
    .map(str::to_string);
    let Some(target) = recovery::ranked(
        &registry,
        &snapshot,
        provider,
        current.as_deref(),
        "",
        policy.priority,
    )
    .into_iter()
    .next()
    .map(|account| account.id.clone()) else {
        return Ok(AutopilotSwitch::NoRoom);
    };
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    params.insert("account".into(), json!(target));
    params.insert("force".into(), json!(true));
    let result = cli::switch_session(state, &params)?;
    if result["status"].as_str() == Some("failed") {
        return Err(DomainStateError::bad_request(
            result["error"]
                .as_str()
                .or(result["message"].as_str())
                .unwrap_or("The switch did not finish.")
                .to_string(),
        ));
    }
    let account = &result["account"];
    let number = match &account["number"] {
        Value::String(number) => number.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let name = match account["email"].as_str().filter(|email| !email.is_empty()) {
        Some(email) if !number.is_empty() => format!("account {number} ({email})"),
        Some(email) => email.to_string(),
        None if !number.is_empty() => format!("account {number}"),
        None => "another account".to_string(),
    };
    Ok(AutopilotSwitch::Moved(name))
}
