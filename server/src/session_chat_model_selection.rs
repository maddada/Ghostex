//! CDXC:SessionChat 2026-09-05 DECISION:
//! User: model selection is always available, including during a turn; if delivery cannot succeed, queue it for the next opportunity.
//! User: try the change immediately, even while working; only retain it when the terminal cannot accept it. This supersedes waiting for an idle turn boundary.
//! One durable desired selection per session coalesces repeated choices, survives disconnects and restarts, and remains pending until the terminal confirms it.
//! CDXC:SessionChat 2026-09-08 DECISION:
//! User: effort, Plan mode, Fast mode and Claude permissions are always changeable from chat too; attempt immediately and retain the desired state for retry if delivery cannot succeed.

use crate::{domain::DomainStateError, server::AppState, storage::open_gxserver_database};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
};

static SELECTION_REQUESTED: tokio::sync::Notify = tokio::sync::Notify::const_new();

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode: Option<String>,
    /// Codex's permission mode, one of `CODEX_PERMISSION_MODES`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

impl SelectionOptions {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.fast_mode.is_none() && self.permissions.is_none()
    }
}

pub(crate) fn read_options(
    provider: &str,
    params: &Map<String, Value>,
) -> Result<SelectionOptions, DomainStateError> {
    let options: SelectionOptions =
        serde_json::from_value(params.get("options").cloned().unwrap_or_else(|| json!({})))
            .map_err(|_| DomainStateError {
                code: "invalidParams",
                message: "Invalid chat option selection.".into(),
            })?;
    let valid_mode = options.mode.as_deref().is_none_or(|mode| match provider {
        "opencode" => !mode.trim().is_empty() && mode.len() <= 160,
        "codex" => matches!(mode, "plan" | "default"),
        "claude" => matches!(mode, "bypass" | "auto" | "manual" | "accept-edits" | "plan"),
        "antigravity" => matches!(mode, "default" | "accept-edits" | "auto" | "plan"),
        _ => false,
    });
    let valid_permissions = options.permissions.as_deref().is_none_or(|value| {
        provider == "codex"
            && crate::session_chat_options::codex_permissions_command(value).is_some()
    });
    if (options.fast_mode.is_some() && !matches!(provider, "codex" | "claude"))
        || !valid_mode
        || !valid_permissions
        || !options
            .fast_mode
            .as_deref()
            .is_none_or(|value| matches!(value, "on" | "off"))
    {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "That option is not available for this agent.".into(),
        });
    }
    Ok(options)
}

pub(crate) async fn selection_requested() {
    SELECTION_REQUESTED.notified().await;
}

pub(crate) const SCOPE_SESSION: &str = "session";
pub(crate) const SCOPE_DEFAULT: &str = "default";

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: a chat model pick can apply to this session alone instead of changing the agent's saved default (2026-09-18), and Codex uses its own session choice too (2026-09-27).
/// Claude's `/model` list answers `s` with "for this session only"; Codex 0.157 added `s session` to its reasoning lists, answered with "for this conversation". This supersedes Claude-only session picks, from when confirming Codex's picker always rewrote `model` and `model_reasoning_effort` in `~/.codex/config.toml`; an older Codex refuses the session pick instead of saving the default.
/// An absent scope stays `default`, so a client that predates the field keeps its old behaviour.
/// SEE-ALSO: server/src/session_chat_codex_picker.rs drives both scopes, packages/shared/session-chat-agents.ts carries the wire type.
pub(crate) fn read_scope(
    provider: &str,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(SCOPE_DEFAULT);
    if provider == "opencode" && matches!(scope, SCOPE_DEFAULT | SCOPE_SESSION) {
        return Ok(scope.to_string());
    }
    if scope == SCOPE_DEFAULT {
        return Ok(SCOPE_DEFAULT.to_string());
    }
    if scope != SCOPE_SESSION {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "A model selection scope is either session or default.".into(),
        });
    }
    if !matches!(provider, "claude" | "codex") {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "This agent's model picker always saves the choice as its default.".into(),
        });
    }
    Ok(SCOPE_SESSION.to_string())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingModelSelection {
    pub id: String,
    pub model: String,
    pub effort: String,
    pub options: SelectionOptions,
    /// Always serialized so a client can tell a scope-aware daemon from one that dropped the field.
    pub scope: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip)]
    pub retry_at: i64,
}

pub type ModelSelectionSender = Arc<
    dyn Fn(String, String, PendingModelSelection) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

fn storage_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

pub fn read_pending(
    db: &Connection,
    project: &str,
    session: &str,
) -> Option<PendingModelSelection> {
    db.query_row(
        "SELECT selectionId, model, effort, state, errorMessage, retryAt, options, scope FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2",
        params![project, session],
        |row| Ok(PendingModelSelection { id: row.get(0)?, model: row.get(1)?, effort: row.get(2)?, state: row.get(3)?, error_message: row.get(4)?, retry_at: row.get(5)?, options: serde_json::from_str(&row.get::<_, String>(6)?).map_err(|error| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error)))?, scope: row.get(7)? }),
    ).optional().ok().flatten()
}

/// Whether a model or option change is waiting, applying or failed for this session; a new chat
/// message then waits behind it.
pub(crate) fn has_pending_selection(state: &AppState, project: &str, session: &str) -> bool {
    open_gxserver_database(&state.paths)
        .is_ok_and(|db| read_pending(&db, project, session).is_some())
}

/// The published catalog can grow without rebuilding gxserver; the terminal is the authority on availability.
pub(crate) fn validate_selection(
    provider: &str,
    model: &str,
    effort: &str,
) -> Result<(), DomainStateError> {
    let token = |value: &str| {
        !value.is_empty()
            && value.len() <= 160
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._[]():/@+".contains(&byte))
    };
    if !matches!(
        provider,
        "codex"
            | "claude"
            | "cursor"
            | "grok"
            | "antigravity"
            | "opencode"
            | "hermes"
            | "pi"
            | "omp"
            | "empryo"
            | "dsh"
    ) || !token(model)
        || (!effort.is_empty() && !token(effort))
    {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "A valid model and effort are required for this agent.".into(),
        });
    }
    Ok(())
}

/// Drops the session's queued, applying or failed model pick when a draft switches agent: it was
/// made for the agent being replaced (CDXC:SessionChat 2026-10-24 at `foreign_model_refusal`), and
/// the switch carries a pick for the new agent on its launch line.
pub(crate) fn discard_for_agent_switch(db: &Connection, project: &str, session: &str) {
    let _ = db.execute(
        "DELETE FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2",
        params![project, session],
    );
}

/// The agents that run only their own vendor's models, with the name chat uses for them.
const FIRST_PARTY_AGENTS: [(&str, &str); 2] = [("claude", "Claude Code"), ("codex", "Codex")];

/// CDXC:SessionChat 2026-10-24 WHY:
/// A model is only sent to the agent it belongs to. A Codex pick queued while a new chat was still a Codex draft stayed queued when the draft switched to Claude Code, and `/model gpt-6.1-sol` was typed into Claude three times ("Model 'gpt-6.1-sol' not found", GitHub #290). The switch now drops the old agent's pick (agents/drafts.rs), and a pick the catalog lists for the other first-party agent but not for this one is refused before anything is typed. Agents that run several vendors' models (Cursor, Antigravity) are never refused here.
/// Returns the message to refuse with.
pub(crate) fn foreign_model_refusal(provider: &str, model: &str) -> Option<String> {
    foreign_model_owner(provider, model, |agent, model| {
        crate::agent_model_catalog::catalog_model(agent, model).is_some()
    })
}

fn foreign_model_owner(
    provider: &str,
    model: &str,
    listed: impl Fn(&str, &str) -> bool,
) -> Option<String> {
    let (_, agent_name) = FIRST_PARTY_AGENTS
        .iter()
        .find(|(agent, _)| *agent == provider)?;
    if model.is_empty() || listed(provider, model) {
        return None;
    }
    let (_, owner_name) = FIRST_PARTY_AGENTS
        .iter()
        .find(|(agent, _)| *agent != provider && listed(agent, model))?;
    Some(format!(
        "{model} is a {owner_name} model, so it was not sent to {agent_name}. Pick a {agent_name} model, or pick {model} again to move this chat to {owner_name}."
    ))
}

pub(crate) fn enqueue(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    enqueue_selection(state, params, false)
}

/// Queues a change Ghostex makes on its own, unless a pick is already waiting or applying: the
/// user's own pick always wins, and a pick the user makes afterwards replaces this one.
pub(crate) fn enqueue_unless_pending(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    enqueue_selection(state, params, true)
}

fn enqueue_selection(
    state: &AppState,
    params: &Map<String, Value>,
    unless_pending: bool,
) -> Result<Value, DomainStateError> {
    let project = params
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if project.is_empty() || session_id.is_empty() {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "A project and session are required.".into(),
        });
    }
    let db = open_gxserver_database(&state.paths).map_err(storage_error)?;
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let session = repository
        .get_session(project, session_id)?
        .ok_or_else(|| DomainStateError {
            code: "notFound",
            message: "The session no longer exists.".into(),
        })?;
    let provider =
        crate::session_chat_follower::session_chat_agent_for_session(&session).unwrap_or_default();
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let effort = params
        .get("effort")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let incoming_options = read_options(&provider, params)?;
    let scope = read_scope(&provider, params)?;
    if !model.is_empty() || incoming_options.is_empty() {
        validate_selection(&provider, model, effort)?;
    } else if !matches!(
        provider.as_str(),
        "codex" | "claude" | "opencode" | "antigravity"
    ) || !effort.is_empty()
    {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "An effort change requires its model.".into(),
        });
    }
    // Merge under the write lock so independent controls cannot overwrite each other's pending choice.
    let transaction =
        rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
    // CDXC:SessionChat 2026-09-29 WHY:
    // A failed change never reached the agent and never will, so a new pick replaces it instead of
    // merging into it: inheriting a Fast mode the account cannot turn on made every later Mode or
    // model pick fail on that toggle again, and the pick itself was never applied.
    let previous =
        read_pending(&transaction, project, session_id).filter(|pending| pending.state != "failed");
    if unless_pending && previous.is_some() {
        return Ok(json!({ "ok": true, "queued": false, "skipped": "pendingSelection" }));
    }
    let model = if model.is_empty() {
        previous
            .as_ref()
            .map_or("", |pending| pending.model.as_str())
    } else {
        model
    };
    let effort = if params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty()
    {
        previous
            .as_ref()
            .map_or("", |pending| pending.effort.as_str())
    } else {
        effort
    };
    let mut options = previous
        .as_ref()
        .map(|pending| pending.options.clone())
        .unwrap_or_default();
    if incoming_options.mode.is_some() {
        options.mode = incoming_options.mode;
    }
    if incoming_options.permissions.is_some() {
        options.permissions = incoming_options.permissions;
    }
    if incoming_options.fast_mode.is_some() {
        options.fast_mode = incoming_options.fast_mode;
    }
    // CDXC:SessionChat 2026-09-19 WHY:
    // Only a request that names a model carries a scope of its own; an absent one means `default`
    // even while a session-only pick is pending, so an older client keeps its old behaviour. An
    // options-only change keeps the pending model choice's scope so it cannot turn a session-only
    // pick into a default one, and with no pending choice it is `default`: a session-only row without
    // a model would be refused by the driver and retried every five seconds forever.
    let replaces_model = !params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty();
    let scope = if replaces_model {
        scope
    } else {
        previous
            .as_ref()
            .map_or(SCOPE_DEFAULT, |pending| pending.scope.as_str())
            .to_string()
    };
    let id = uuid::Uuid::new_v4().to_string();
    transaction.execute(
        "INSERT INTO session_chat_model_selections (projectId, sessionId, selectionId, model, effort, state, retryAt, options, scope) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0, ?6, ?7) ON CONFLICT(projectId, sessionId) DO UPDATE SET selectionId = excluded.selectionId, model = excluded.model, effort = excluded.effort, options = excluded.options, scope = excluded.scope, state = 'queued', errorMessage = NULL, retryAt = 0",
        params![project, session_id, id, model, effort, serde_json::to_string(&options).map_err(storage_error)?, scope],
    ).map_err(storage_error)?;
    transaction.commit().map_err(storage_error)?;
    // A new pick is a new attempt; a failure it meets marks the session again.
    crate::session_chat_model_selection_alert::clear_model_selection_failure(
        state, project, session_id,
    );
    let pending = read_pending(&db, project, session_id);
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project, session_id,
    );
    SELECTION_REQUESTED.notify_one();
    Ok(
        json!({ "ok": true, "model": model, "effort": effort, "scope": scope, "queued": true, "pendingModelSelection": pending }),
    )
}

/// CDXC:SessionChat 2026-10-17 WHY:
/// A pick whose command was typed but never confirmed on screen (a timeout) is retried only this many times before it counts as failed. One footer the reader could not parse (omp 18.11's compact model segment) retyped `/switch` every ~12s for over 13 minutes; only closing the session ended it. A busy agent (a dialog in front of its input box) is not counted and keeps retrying.
const MAX_UNCONFIRMED_ATTEMPTS: u32 = 3;

/// CDXC:SessionChat 2026-10-24 WHY:
/// A pick that keeps finding no input box at all (a blank screen, a CLI that never came up, a shell prompt) types nothing, but it retried every ~6 seconds for minutes with nothing in the chat to say why (GitHub #290, a Codex that never drew its input box). After this long without the agent's input box it fails with the reason, which the chat and the sidebar's red dot show; picking the model again retries it. A dialog standing in front of the input box is not counted: answering it lets the pick through.
const MISSING_INPUT_BOX_LIMIT_MS: i64 = 120_000;

/// When each pick first found no input box, until it finds one.
fn missing_input_box_since() -> &'static Mutex<HashMap<String, i64>> {
    static SINCE: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    SINCE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Notes this failure and answers whether the pick has now gone `MISSING_INPUT_BOX_LIMIT_MS`
/// without seeing the agent's input box.
fn missing_input_box_exhausted(selection_id: &str, error: &DomainStateError, now_ms: i64) -> bool {
    let Ok(mut since) = missing_input_box_since().lock() else {
        return false;
    };
    if error.code != crate::session_chat_codex_picker::INPUT_BOX_MISSING_CODE {
        since.remove(selection_id);
        return false;
    }
    let first = *since.entry(selection_id.to_string()).or_insert(now_ms);
    now_ms - first >= MISSING_INPUT_BOX_LIMIT_MS
}

/// The names each pick has tried after Claude refused one, so a refused name is never typed again.
fn tried_model_forms() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static TRIED: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    TRIED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// CDXC:SessionChat 2026-11-13 DECISION:
/// The coordinator, for the user (G7zwb typed `/model claude-opus` nine times): when Claude
/// refuses a model name, Ghostex tries the next valid form of it once (Claude's alias, then the full
/// id), and when that is refused too it stops, leaves the session on its model, and the failure
/// shows in the chat. The forms of `claude-opus` or `opus[1m]` are `opus`, then the full id of the
/// catalog's current Opus (`claude-opus-5-5`); a full id with a version has no other form.
fn next_model_form(selection_id: &str, model: &str) -> Option<String> {
    let mut tried = tried_model_forms().lock().ok()?;
    let names = tried.entry(selection_id.to_string()).or_default();
    if !names.iter().any(|name| name == model) {
        names.push(model.to_string());
    }
    let next = claude_model_forms(model)
        .into_iter()
        .find(|form| !names.contains(form))?;
    names.push(next.clone());
    Some(next)
}

/// Claude's other names for `model`, in the order to try them: its alias, then its full id.
fn claude_model_forms(model: &str) -> Vec<String> {
    let base = model.strip_suffix("[1m]").unwrap_or(model);
    let family = base.strip_prefix("claude-").unwrap_or(base);
    if family.is_empty() || !family.chars().all(|c| c.is_ascii_alphabetic()) {
        return Vec::new();
    }
    let mut forms = vec![family.to_string()];
    // The full id of the catalog's current row of that family ("Opus 5.5" => claude-opus-5-5).
    if let Some(label) = crate::agent_model_catalog::catalog_agent("claude")
        .and_then(|agent| agent.get("models").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter(|row| row.get("group").is_none())
        .filter_map(|row| row.get("label").and_then(Value::as_str).map(str::to_string))
        .find(|label| {
            label
                .to_ascii_lowercase()
                .strip_prefix(family)
                .is_some_and(|rest| rest.starts_with(' '))
        })
    {
        forms.push(format!(
            "claude-{}",
            label
                .to_ascii_lowercase()
                .replace(' ', "-")
                .replace('.', "-")
        ));
    }
    forms.retain(|form| form != model);
    forms
}

fn unconfirmed_attempts() -> &'static Mutex<HashMap<String, u32>> {
    static ATTEMPTS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Counts an unconfirmed attempt and answers whether the pick has now used them all.
fn unconfirmed_attempts_exhausted(selection_id: &str, error: &DomainStateError) -> bool {
    let Ok(mut attempts) = unconfirmed_attempts().lock() else {
        return false;
    };
    if error.code != "timeout" {
        return false;
    }
    let count = attempts.entry(selection_id.to_string()).or_insert(0);
    *count += 1;
    *count >= MAX_UNCONFIRMED_ATTEMPTS
}

/// Whether a change ended only because the agent's own mode cycle leaves the mode out. A change
/// that failed any other way, or was given up on after retries, is not this case.
fn mode_was_refused(error: &DomainStateError) -> bool {
    error.code == crate::session_chat_codex_picker::UNSUPPORTED_SELECTION_CODE
        && crate::session_chat_mode_availability::is_refusal(&error.message)
}

pub(crate) fn sender(state: &Arc<AppState>) -> ModelSelectionSender {
    let state = state.clone();
    Arc::new(move |project, session, pending| {
        let state = state.clone();
        Box::pin(async move {
            let claimed = open_gxserver_database(&state.paths).ok().and_then(|db| db.execute(
                "UPDATE session_chat_model_selections SET state = 'applying', errorMessage = NULL WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3",
                params![project, session, pending.id],
            ).ok()).is_some_and(|count| count > 0);
            if !claimed {
                return;
            }
            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                &state, &project, &session,
            );
            let params = json!({ "projectId": project, "sessionId": session, "model": pending.model, "effort": pending.effort, "options": pending.options, "scope": pending.scope });
            let result = crate::session_chat_codex_picker::select_session_chat_model(
                &state,
                params.as_object().unwrap(),
            )
            .await;
            let now_ms = chrono::Utc::now().timestamp_millis();
            // Claude refused the name: its other form once, then a final failure (never the same
            // name again).
            let result = match result {
                Err(error) if error.code == crate::session_chat_codex_picker::MODEL_NOT_FOUND_CODE => {
                    match next_model_form(&pending.id, &pending.model) {
                        Some(next) => {
                            if let Ok(db) = open_gxserver_database(&state.paths) {
                                let _ = db.execute(
                                    "UPDATE session_chat_model_selections SET model = ?4, state = 'queued', errorMessage = ?5, retryAt = 0 WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3",
                                    params![project, session, pending.id, next, error.message],
                                );
                            }
                            SELECTION_REQUESTED.notify_one();
                            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                                &state, &project, &session,
                            );
                            return;
                        }
                        None => Err(crate::session_chat_codex_picker::unsupported_selection(
                            format!(
                                "{} The session stays on the model it was on. Pick another model, or switch account.",
                                error.message
                            ),
                        )),
                    }
                }
                other => other,
            };
            if result.is_ok()
                || result.as_ref().is_err_and(|error| {
                    error.code == crate::session_chat_codex_picker::UNSUPPORTED_SELECTION_CODE
                })
            {
                if let Ok(mut tried) = tried_model_forms().lock() {
                    tried.remove(&pending.id);
                }
            }
            let result = result.map_err(|error| {
                if unconfirmed_attempts_exhausted(&pending.id, &error) {
                    crate::session_chat_codex_picker::unsupported_selection(format!(
                        "The agent never confirmed this change after {MAX_UNCONFIRMED_ATTEMPTS} tries, so Ghostex stopped retrying it. {}",
                        error.message
                    ))
                } else if missing_input_box_exhausted(&pending.id, &error, now_ms) {
                    crate::session_chat_codex_picker::unsupported_selection(format!(
                        "The agent has not shown its input box for {} minutes, so Ghostex stopped trying to change the model. Open the terminal to see what it is showing, then pick the model again.",
                        MISSING_INPUT_BOX_LIMIT_MS / 60_000
                    ))
                } else {
                    error
                }
            });
            let final_failure = result.as_ref().is_err_and(|error| {
                error.code == crate::session_chat_codex_picker::UNSUPPORTED_SELECTION_CODE
            });
            if result.is_ok() || final_failure {
                if let Ok(mut attempts) = unconfirmed_attempts().lock() {
                    attempts.remove(&pending.id);
                }
                if let Ok(mut since) = missing_input_box_since().lock() {
                    since.remove(&pending.id);
                }
            }
            let refused_mode = result.as_ref().err().is_some_and(mode_was_refused);
            if let Ok(db) = open_gxserver_database(&state.paths) {
                match &result {
                    Err(_) if refused_mode => {
                        let _ = db.execute(
                            "DELETE FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3",
                            params![project, session, pending.id],
                        );
                    }
                    Ok(_) => {
                        let _ = db.execute("DELETE FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3", params![project, session, pending.id]);
                    }
                    // CDXC:SessionChat 2026-09-18 WHY:
                    // A selection the terminal can never accept stops here instead of retrying:
                    // one model the CLI does not list for an account retyped `/model` into the
                    // user's pane 181 times. The row stays to carry its reason to the chat, and
                    // `failed` keeps it out of delivery, wake-keeping and the desired model.
                    Err(error) if final_failure => {
                        let _ = db.execute("UPDATE session_chat_model_selections SET state = 'failed', errorMessage = ?4, retryAt = 0 WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3", params![project, session, pending.id, error.message]);
                    }
                    Err(error) => {
                        let _ = db.execute("UPDATE session_chat_model_selections SET state = 'queued', errorMessage = ?4, retryAt = ?5 WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3", params![project, session, pending.id, error.message, chrono::Utc::now().timestamp_millis() + 5_000]);
                    }
                }
            }
            match &result {
                Ok(_) => crate::session_chat_model_selection_alert::clear_model_selection_failure(
                    &state, &project, &session,
                ),
                // The refusal is remembered, so the mode menu greys it and says why; nothing is left
                // to raise or hold (the 2026-10-31 DECISION on `note_model_selection_failure`).
                Err(error) if refused_mode => {
                    crate::session_chat_mode_availability::remember_refusal(
                        &state,
                        &project,
                        &session,
                        &error.message,
                    );
                    crate::session_chat_send_diagnostics::record_send_recovery(
                        &state,
                        "sessionChatModeRefused",
                        &project,
                        &session,
                        &error.message,
                        &[],
                    );
                    crate::session_chat_model_selection_alert::clear_model_selection_failure(
                        &state, &project, &session,
                    );
                }
                Err(error) => {
                    crate::session_chat_model_selection_alert::note_model_selection_failure(
                        &state,
                        &project,
                        &session,
                        &pending,
                        error,
                        final_failure,
                    )
                    .await
                }
            }
            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                &state, &project, &session,
            );
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn final_failure(message: String) -> DomainStateError {
        crate::session_chat_codex_picker::unsupported_selection(message)
    }

    #[test]
    fn a_refused_mode_is_told_apart_from_every_other_failed_change() {
        let refused = crate::session_chat_mode_availability::refusal_message(
            "bypass",
            &["auto".to_string(), "manual".to_string()],
        );
        assert!(mode_was_refused(&final_failure(refused.clone())));
        // A model the agent does not list keeps its dot and its hold.
        assert!(!mode_was_refused(&final_failure(
            "Claude's model list does not offer opus in this session, so it cannot be applied."
                .to_string()
        )));
        // Given up on after retries: the agent never said the mode was left out.
        assert!(!mode_was_refused(&final_failure(format!(
            "The agent never confirmed this change after 3 tries, so Ghostex stopped retrying it. {refused}"
        ))));
        // The same words on a retryable error are not a final answer.
        assert!(!mode_was_refused(&DomainStateError {
            code: "agentBusy",
            message: refused,
        }));
    }

    /// The bundled catalog's first-party lineups, without the live catalog.
    fn listed(agent: &str, model: &str) -> bool {
        match agent {
            "claude" => ["fable", "opus[1m]", "sonnet", "haiku"].contains(&model),
            "codex" => ["gpt-6.1-sol", "gpt-6-luna", "gpt-6-sol"].contains(&model),
            _ => false,
        }
    }

    #[test]
    fn a_codex_model_is_refused_for_claude_code() {
        let refusal = foreign_model_owner("claude", "gpt-6.1-sol", listed).unwrap();
        assert!(
            refusal.starts_with("gpt-6.1-sol is a Codex model, so it was not sent to Claude Code.")
        );
    }

    #[test]
    fn a_claude_model_is_refused_for_codex() {
        let refusal = foreign_model_owner("codex", "opus[1m]", listed).unwrap();
        assert!(
            refusal.starts_with("opus[1m] is a Claude Code model, so it was not sent to Codex.")
        );
    }

    #[test]
    fn an_agents_own_or_unlisted_model_goes_through() {
        assert_eq!(foreign_model_owner("claude", "sonnet", listed), None);
        assert_eq!(foreign_model_owner("codex", "gpt-6-luna", listed), None);
        // A full id the catalog does not list is the terminal's to judge.
        assert_eq!(
            foreign_model_owner("claude", "claude-haiku-4-5-20251001", listed),
            None
        );
        assert_eq!(foreign_model_owner("claude", "", listed), None);
    }

    #[test]
    fn agents_that_run_several_vendors_models_are_never_refused() {
        assert_eq!(foreign_model_owner("cursor", "gpt-6.1-sol", listed), None);
        assert_eq!(foreign_model_owner("antigravity", "sonnet", listed), None);
    }

    #[test]
    fn the_bundled_catalog_owns_the_reported_model() {
        assert!(foreign_model_refusal("claude", "gpt-6.1-sol").is_some());
        assert_eq!(foreign_model_refusal("codex", "gpt-6.1-sol"), None);
    }

    fn missing() -> DomainStateError {
        DomainStateError {
            code: crate::session_chat_codex_picker::INPUT_BOX_MISSING_CODE,
            message: "Codex is not showing its input box, so the model was not changed.".into(),
        }
    }

    #[test]
    fn a_pick_without_an_input_box_gives_up_after_the_limit() {
        let id = "missing-input-box-limit";
        assert!(!missing_input_box_exhausted(id, &missing(), 1_000));
        assert!(!missing_input_box_exhausted(
            id,
            &missing(),
            1_000 + MISSING_INPUT_BOX_LIMIT_MS - 1
        ));
        assert!(missing_input_box_exhausted(
            id,
            &missing(),
            1_000 + MISSING_INPUT_BOX_LIMIT_MS
        ));
    }

    #[test]
    fn a_dialog_in_front_of_the_input_box_restarts_the_count() {
        let id = "missing-input-box-dialog";
        let busy = DomainStateError {
            code: "agentBusy",
            message: "Codex is not showing its input box, so the model was not changed.".into(),
        };
        assert!(!missing_input_box_exhausted(id, &missing(), 0));
        assert!(!missing_input_box_exhausted(
            id,
            &busy,
            MISSING_INPUT_BOX_LIMIT_MS / 2
        ));
        assert!(!missing_input_box_exhausted(
            id,
            &missing(),
            MISSING_INPUT_BOX_LIMIT_MS + 1
        ));
    }
}
