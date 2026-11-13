//! CDXC:AgentProviders 2026-11-13 DECISION:
//! User, after a Fable 5.2 chat waited with "7d limit reached on every account" while ST showed
//! 86% (its Fable window was full, and SP was set to manual only): the limit card names the real
//! reason in one sentence and offers "Continue with Opus 5.5 on ST" as its main button (Q2977 A,
//! Q2978 A), and "we also need a new toggle that continues on any open Opus when Fable runs out on
//! all accounts, in Settings for Claude accounts, since that's the most common case. If the user is
//! using Fable then we switch to another account that has Fable usage of course."
//! So a Fable session at a usage limit still moves first to another account with Auto-switch on
//! that has Fable room (recovery.rs `switch_target`). Only when none has any does it continue on
//! the strongest Opus in the model catalog, at the same effort, on an account that has room for
//! it: the account it is on first, then the accounts with Auto-switch on, in the session's Try
//! first order. An account set to manual only is never picked by itself. The setting
//! (`claudeFableFallbackToOpus`) is on by default. The session does not move back to Fable when
//! Fable room returns; the user picks Fable again from the model pill.
//! SEE-ALSO: recovery.rs (`plan_session` calls `alternative` and `continue_with`), limit_wait.rs
//! (the card's facts carry the alternative as its button), packages/gx-chat-core/src/questions/account_wait.rs.
use super::{continuation::SwitchSource, endpoint, model::*};
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use serde_json::{json, Map, Value};

/// The Settings > Accounts switch, a shared setting gxserver reads (default on).
pub(crate) const SETTING: &str = "claudeFableFallbackToOpus";

/// Whether a Fable session that finds no account with Fable room continues on Opus by itself.
pub(super) fn enabled(state: &AppState) -> bool {
    crate::session_lifecycle::read_sidebar_settings(&state.paths)
        .as_ref()
        .and_then(|settings| settings.get(SETTING))
        .and_then(Value::as_bool)
        != Some(false)
}

fn is_fable(model: &str) -> bool {
    model.to_ascii_lowercase().contains("fable")
}

/// The name chat shows for a model value: the catalog's label, else what the agent printed.
pub(super) fn model_label(model: &str, detected: Option<&str>) -> Option<String> {
    crate::agent_model_catalog::catalog_model("claude", model)
        .and_then(|row| row["label"].as_str().map(str::to_string))
        .or_else(|| detected.map(str::to_string))
        .filter(|label| !label.trim().is_empty())
}

/// The strongest Opus the catalog offers (its first Opus row outside Legacy models): its value,
/// label, and the effort to run it at (the session's own when Opus offers it).
fn strongest_opus(effort: &str) -> Option<(String, String, String)> {
    let agent = crate::agent_model_catalog::catalog_agent("claude")?;
    let row = agent["models"].as_array()?.iter().find(|row| {
        row["group"].as_str() != Some("legacy")
            && row["value"]
                .as_str()
                .is_some_and(|value| value.to_ascii_lowercase().contains("opus"))
    })?;
    let efforts = row["efforts"].as_array().or(agent["efforts"].as_array());
    let offered = |level: &str| {
        efforts.is_some_and(|efforts| efforts.iter().any(|e| e.as_str() == Some(level)))
    };
    let effort = if !effort.is_empty() && offered(effort) {
        effort.to_string()
    } else {
        row["defaultEffort"]
            .as_str()
            .or(agent["defaultEffort"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    Some((
        row["value"].as_str()?.to_string(),
        row["label"].as_str()?.to_string(),
        effort,
    ))
}

/// A model and account a Fable session at its limit can continue on.
#[derive(Clone, Debug)]
pub(crate) struct Alternative {
    pub account_id: String,
    pub account_name: String,
    pub model: String,
    pub model_label: String,
    pub effort: String,
}

impl Alternative {
    /// The facts the limit card turns into its "Continue with … on …" button.
    pub(super) fn to_json(&self) -> Value {
        json!({
            "accountId": self.account_id,
            "account": self.account_name,
            "model": self.model,
            "modelLabel": self.model_label,
            "effort": self.effort,
        })
    }
}

/// Where a Fable session that no account has Fable room for can continue on Opus: the account it
/// is on when that has room for Opus, else the first account with Auto-switch on that has, in the
/// session's Try first order. `None` for any other model, or when no account has room.
pub(super) fn alternative(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: Option<&str>,
    model: &str,
    effort: &str,
    priority: Priority,
    session: &Value,
) -> Option<Alternative> {
    if provider != Provider::Claude || !is_fable(model) {
        return None;
    }
    let (value, label, effort) = strongest_opus(effort)?;
    let room = |id: &str| {
        snapshot.accounts.iter().any(|found| {
            endpoint::account_id(found) == id && super::recovery::trusted_room(found, &value)
        })
    };
    let here = current
        .and_then(|id| registry.accounts.iter().find(|a| a.id == id))
        .filter(|a| a.provider == provider && !a.user_disabled && a.login_expired_at.is_none())
        .filter(|a| room(&a.id));
    // An account this session automatically left counts once a reading taken since says so,
    // the rule `switch_target` keeps.
    let left = super::recovery::left_account(session);
    let account = here.or_else(|| {
        super::recovery::ranked(registry, snapshot, provider, current, &value, priority)
            .into_iter()
            .find(|account| match left {
                Some((from, at)) if from == account.id => snapshot
                    .accounts
                    .iter()
                    .find(|found| endpoint::account_id(found) == from)
                    .and_then(super::recovery::reading_time)
                    .is_some_and(|read| read > at),
                _ => true,
            })
    })?;
    Some(Alternative {
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        model: value,
        model_label: label,
        effort,
    })
}

/// Moves the session to `alternative`'s model, and to its account when that is another one, then
/// continues the task the way an automatic switch does. The model change is a session-only pick
/// (`/model` … `s`), queued first so the continuation's "." waits behind it (continuation.rs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn continue_with(
    state: &AppState,
    repo: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    project: &Value,
    session: &Value,
    alternative: &Alternative,
    from_label: Option<&str>,
    source: SwitchSource,
) -> Result<(), DomainStateError> {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return Err(DomainStateError::bad_request(
            "A project and session are required.",
        ));
    };
    let provider = launch_provider(project, session)?;
    let mut params = Map::new();
    params.insert("projectId".into(), json!(pid));
    params.insert("sessionId".into(), json!(sid));
    params.insert("model".into(), json!(alternative.model));
    params.insert("effort".into(), json!(alternative.effort));
    params.insert(
        "scope".into(),
        json!(crate::session_chat_model_selection::SCOPE_SESSION),
    );
    if source == SwitchSource::Automatic {
        crate::session_chat_model_selection::enqueue_unless_pending(state, &params)?;
    } else {
        crate::session_chat_model_selection::enqueue(state, &params)?;
    }
    let current = super::session_identity::display_account_id(
        registry,
        provider,
        session,
        &state.paths.home_dir,
    )
    .map(str::to_string);
    let mark = |progress: &mut Map<String, Value>| {
        progress.insert("model".into(), json!(alternative.model_label));
        if let Some(from) = from_label {
            progress.insert("fromModel".into(), json!(from));
        }
    };
    if current.as_deref() == Some(alternative.account_id.as_str()) {
        // Same account: the agent's limit dialog stands in front of its input box, so the
        // session restarts in place (the way recovery continues on the same account) and the
        // queued model is applied before the continuation goes out.
        let mut runtime = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let now = chrono::Utc::now();
        let mut progress = Map::new();
        progress.insert("id".into(), json!(uuid::Uuid::new_v4().to_string()));
        progress.insert("provider".into(), json!(provider));
        progress.insert(
            "source".into(),
            json!(if source == SwitchSource::Automatic {
                "automatic"
            } else {
                "manual"
            }),
        );
        progress.insert("phase".into(), json!("resuming"));
        progress.insert("accountReady".into(), json!(true));
        progress.insert("fromAccountId".into(), json!(alternative.account_id));
        progress.insert("toAccountId".into(), json!(alternative.account_id));
        progress.insert("updatedAt".into(), json!(now.to_rfc3339()));
        mark(&mut progress);
        runtime.insert("accountSwitch".into(), Value::Object(progress));
        let updated = endpoint::update_session_changes(repo, session, &runtime, Map::new())?;
        super::switch_progress::publish(state, &updated);
        crate::session_chat_notice::suppress_account_usage_notice(pid, sid, now);
        if updated["lifecycleState"].as_str() == Some("running") {
            endpoint::cycle(state, repo, &updated, "/api/sleepSession")?;
        }
        endpoint::cycle(state, repo, &updated, "/api/wakeSession")?;
        super::continuation::start(state, repo, &updated, source)?;
        return Ok(());
    }
    endpoint::select(
        state,
        repo,
        registry,
        snapshot,
        project,
        session,
        Some(&alternative.account_id),
        source,
    )?;
    if let Some(row) = repo.get_session(pid, sid)? {
        let mut runtime = row["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(progress) = runtime
            .get_mut("accountSwitch")
            .and_then(Value::as_object_mut)
        {
            mark(progress);
            let updated = endpoint::update_session_changes(repo, &row, &runtime, Map::new())?;
            super::switch_progress::publish(state, &updated);
        }
    }
    Ok(())
}

fn launch_provider(project: &Value, session: &Value) -> Result<Provider, DomainStateError> {
    super::launch::provider(project, session)
        .filter(|provider| *provider == Provider::Claude)
        .ok_or_else(|| DomainStateError::bad_request("Only Claude sessions can continue on Opus."))
}

/// `agentAccounts` `continueWithModel` and the limit card's "Continue with … on …" button: the
/// account and model the card offered, chosen by the user.
pub(super) fn chosen(
    state: &AppState,
    repo: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    project: &Value,
    session: &Value,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let account_id = text("accountId")
        .ok_or_else(|| DomainStateError::bad_request("Choose an account to continue on."))?;
    let model = text("model")
        .ok_or_else(|| DomainStateError::bad_request("Choose a model to continue on."))?;
    let provider = launch_provider(project, session)?;
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == account_id && a.provider == provider)
        .ok_or_else(|| {
            DomainStateError::bad_request("Choose a registered account for this provider.")
        })?;
    let alternative = Alternative {
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        model: model.to_string(),
        model_label: model_label(model, text("modelLabel")).unwrap_or_else(|| model.to_string()),
        effort: text("effort").unwrap_or_default().to_string(),
    };
    continue_with(
        state,
        repo,
        registry,
        snapshot,
        project,
        session,
        &alternative,
        text("fromModel"),
        SwitchSource::Forced,
    )
}
