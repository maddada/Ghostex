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
    let workspace_pool = super::workspace_pool::session_pool(&db, &project, &session, provider);
    let Some(target) = recovery::ranked_in_pool(
        &registry,
        &snapshot,
        provider,
        current.as_deref(),
        "",
        policy.priority,
        workspace_pool.as_deref(),
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

/// Whether a usage window applies to `model`: shared windows (`window.model.is_none()`) always
/// apply; model-specific windows (e.g. `seven_day_sonnet`) apply only when `model` is known and
/// names that model, so a full Sonnet window never poisons Opus or provider-wide checks.
pub(super) fn window_applies_to_model(window: &UsageWindow, model: Option<&str>) -> bool {
    let Some(window_model) = window
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return true;
    };
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let model_lower = model.to_lowercase();
    let window_lower = window_model.to_lowercase();
    model_lower.contains(&window_lower) || window_lower.contains(&model_lower)
}

/// Whether `provider` has at least one auto-switchable account (`eligible && !user_disabled`,
/// signed in, with a ready reading and every applicable usage window under 100%).
///
/// CDXC:Autopilot 2026-11-22 WHY:
/// `agent_has_room` used to read only the raw discovered snapshot and ignore `SavedAccount::auto_switchable()`, so a manual-only account at 0% tricked Autopilot into thinking Claude still had room while every eligible account was at 100%.
pub(crate) fn provider_has_eligible_room(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
) -> bool {
    provider_has_eligible_room_for_model_in_pool(registry, snapshot, provider, None, None)
}

/// Like `provider_has_eligible_room`, scoped to `allowed_ids` when a workspace pins a Claude pool.
pub(super) fn provider_has_eligible_room_in_pool(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    allowed_ids: Option<&[String]>,
) -> bool {
    provider_has_eligible_room_for_model_in_pool(registry, snapshot, provider, None, allowed_ids)
}

/// Like `provider_has_eligible_room`, scoped to `model` when known (shared windows always count;
/// model-specific windows count only when `model` matches them).
#[allow(dead_code)]
pub(super) fn provider_has_eligible_room_for_model(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    model: Option<&str>,
) -> bool {
    provider_has_eligible_room_for_model_in_pool(registry, snapshot, provider, model, None)
}

/// Like `provider_has_eligible_room_for_model`, additionally scoped to `allowed_ids` when a
/// workspace pins a Claude account pool.
pub(super) fn provider_has_eligible_room_for_model_in_pool(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    model: Option<&str>,
    allowed_ids: Option<&[String]>,
) -> bool {
    // Before the first accounts discovery completes, assume room so startup does not park early.
    if snapshot.fetched_at.is_none() {
        return true;
    }
    let has_window_room = |account: &DiscoveredAccount| {
        account.status == "ready"
            && account.usage_error.is_none()
            && account
                .usage
                .iter()
                .filter(|window| window_applies_to_model(window, model))
                .all(|window| window.used_percent < 100.0)
    };
    let scoped = allowed_ids.is_some_and(|ids| !ids.is_empty());
    let saved: Vec<_> = registry
        .accounts
        .iter()
        .filter(|account| {
            account.provider == provider
                && super::workspace_pool::in_pool(&account.id, allowed_ids)
        })
        .collect();
    if saved.is_empty() {
        if scoped {
            return false;
        }
        let discovered: Vec<_> = snapshot
            .accounts
            .iter()
            .filter(|account| account.provider == provider)
            .collect();
        return discovered.is_empty() || discovered.iter().any(|account| has_window_room(account));
    }
    saved
        .into_iter()
        .filter(|account| account.auto_switchable())
        .any(|saved| {
            snapshot.accounts.iter().any(|discovered| {
                super::endpoint::account_id(discovered) == saved.id && has_window_room(discovered)
            })
        })
}

/// Whether an agent (`claude`, `codex`) has an eligible auto-switchable account with room to start
/// or wake on: no saved account at all (it runs on the plain login), or an `auto_switchable()`
/// ready one with every usage window under its limit. Never counts manual-only or disabled accounts.
pub(crate) fn agent_has_room(paths: &crate::paths::GxserverPaths, agent: &str) -> bool {
    let provider = match crate::agents::identity::normalize_agent_id(Some(agent)).as_deref() {
        Some("claude" | "openclaude") => Provider::Claude,
        Some("codex") => Provider::Codex,
        _ => return true,
    };
    let snapshot = super::runtime::current_snapshot();
    if snapshot.fetched_at.is_none() {
        return true;
    }
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return true;
    };
    let Ok(registry) = store::read(&db) else {
        return true;
    };
    provider_has_eligible_room(&registry, &snapshot, provider)
}
