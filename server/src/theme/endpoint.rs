//! `/api/theme`: the look settings and their change history, for `ghostex theme` and the "Theme
//! changed" card's state and Undo.
//!
//! Ops (`op`): `read` (every look setting, grouped, and what the current look makes hard to
//! read), `set` (`changes: {key: value}`, saved as one change), `undo` and `redo` (`change`, else
//! the newest one that can be), `history` (the kept changes, newest first) and `change` (one,
//! for its card).
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3113 B, Q3115, Q3118, agent theming page): agents restyle Ghostex through a `ghostex theme` command, first with the look settings Settings already has (stage A), then theme roles. Any agent may change the look; the chat of the agent that changed it shows a "Theme changed" card naming the agent, listing what changed, with Undo. A change saves through the app's own settings save, as `ghostex settings set` does, so it shows at once.
//! SEE-ALSO: server/src/ghostex_cli/theme_cli.rs (the command), packages/gx-chat-core/src/transcript/theme_cards.rs (the card), skills/ghostex-help/SKILL.md (the theme chapter agents read).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use ghostex_gx_protocol::chat_card::THEME_CHANGED;
use serde_json::{json, Map, Value};

use super::journal::{self, Item, ThemeChange, Who};
use super::look;
use super::readability;
use crate::coordinators::now_iso;
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::ghostex_cli::settings as catalog;
use crate::protocol::{rpc_error, rpc_success};
use crate::security::Principal;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

pub(crate) const THEME_ENDPOINT: &str = "/api/theme";

/// What the app's settings save records as the writer.
const SETTINGS_SOURCE: &str = "cli:theme";
const SAVE_TIMEOUT_MS: u64 = 15_000;
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);
const CONFIRM_POLL: Duration = Duration::from_millis(100);

fn error(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> &'a str {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

/// The settings as the app and gxserver read them.
fn read_settings(state: &AppState) -> Result<Map<String, Value>, DomainStateError> {
    let path = state
        .paths
        .app_config_dir
        .join("native-sidebar-settings.json");
    let read = ghostex_paths::settings_file::read(&path).map_err(|failure| {
        error(
            "internalError",
            format!("Could not read {}: {failure}", path.display()),
        )
    })?;
    let mut object = read.object.unwrap_or_default();
    ghostex_settings_catalog::migrate_experimental_features_switch(&mut object);
    ghostex_settings_catalog::migrate_retired_chat_looks(&mut object);
    ghostex_settings_catalog::migrate_marked_handovers(&mut object);
    Ok(object)
}

fn state_dir(state: &AppState) -> &Path {
    &state.paths.app_state_dir
}

fn you() -> Who {
    Who {
        kind: "you".to_string(),
        name: "You".to_string(),
        ..Who::default()
    }
}

/// Who is asking: the agent of the session the command runs in, else you (your own computer,
/// phone or terminal).
fn who(state: &AppState, caller: &Principal, params: &Map<String, Value>) -> Who {
    let session = match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => Some((project_id.clone(), session_id.clone())),
        Principal::Owner { .. } | Principal::Device { .. } => None,
        // The CLI says which session it runs in (`callerSession`, ghostex_cli/rpc.rs).
        _ => {
            let parts: Vec<&str> = text(params, "callerSession").split(':').collect();
            (parts.len() == 3 && parts.iter().all(|part| !part.is_empty()))
                .then(|| (parts[1].to_string(), parts[2].to_string()))
        }
    };
    let Some((project_id, session_id)) = session else {
        return you();
    };
    let agent_id = open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| {
            DomainRepository::new(&db, &state.metadata.server_id)
                .get_session(&project_id, &session_id)
                .ok()
                .flatten()
        })
        .and_then(|session| {
            session
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let name =
        ghostex_gx_chat_core::transcript::agent_message_card::agent_display_name(&agent_id, "");
    Who {
        kind: "agent".to_string(),
        name: if name.trim().is_empty() {
            "An agent".to_string()
        } else {
            name
        },
        agent_id,
        session: format!("{}:{project_id}:{session_id}", state.metadata.server_id),
    }
}

/// Why `change` can't be undone (`undo`) or redone now; `None` when it can.
fn problem(
    change: &ThemeChange,
    settings: &Map<String, Value>,
    undo: bool,
) -> Option<(&'static str, String)> {
    match (undo, change.undone_at.is_some()) {
        (true, true) => {
            return Some((
                "alreadyUndone",
                "That change was already undone.".to_string(),
            ))
        }
        (false, false) => {
            return Some(("conflict", "That change is not undone.".to_string()));
        }
        _ => {}
    }
    let changed: Vec<&str> = change
        .items
        .iter()
        .filter(|item| {
            let expected = if undo { &item.after } else { &item.before };
            !catalog::values_equal(&look::current(settings, &item.key), expected)
        })
        .map(|item| item.title.as_str())
        .collect();
    if changed.is_empty() {
        return None;
    }
    let names = changed
        .iter()
        .map(|title| format!("\"{title}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Some((
        "conflict",
        if undo {
            format!("{names} changed again since, so Undo would lose that change.")
        } else {
            format!("{names} changed since it was undone, so redo would lose that change.")
        },
    ))
}

/// A change as `/api/theme` answers it.
fn summary(change: &ThemeChange, settings: &Map<String, Value>) -> Value {
    let undo_problem = problem(change, settings, true).map(|(_, message)| message);
    let redo_problem = problem(change, settings, false).map(|(_, message)| message);
    json!({
        "changeId": change.change_id,
        "at": change.at,
        "by": change.by,
        "items": change.items,
        "warnings": change.warnings,
        "undoneAt": change.undone_at,
        "undoneBy": change.undone_by,
        "undoable": undo_problem.is_none(),
        "undoProblem": if change.undone_at.is_some() { None } else { undo_problem },
        "redoable": change.undone_at.is_some() && redo_problem.is_none(),
    })
}

/// The "Theme changed" card the command prints for its chat
/// (packages/gx-protocol/src/chat_card.rs).
fn card(change: &ThemeChange) -> Value {
    json!({
        "kind": THEME_CHANGED,
        "changeId": change.change_id,
        "agentId": change.by.agent_id,
        "by": change.by.name,
        "items": change.items.iter().map(|item| json!({
            "title": item.title,
            "before": item.before_shown,
            "after": item.after_shown,
        })).collect::<Vec<_>>(),
        "warnings": change.warnings,
    })
}

/// Saves `patch` through the app (or gxserver on a computer without it) and waits until the
/// settings file holds it.
async fn save(state: &AppState, patch: Map<String, Value>) -> Result<(), DomainStateError> {
    let refused = crate::settings_agent_writable::refused_keys(&patch);
    if !refused.is_empty() {
        return Err(error(
            "forbidden",
            crate::settings_agent_writable::refusal_message(&refused),
        ));
    }
    let mut payload = Map::new();
    payload.insert("patch".into(), Value::Object(patch.clone()));
    payload.insert("source".into(), json!(SETTINGS_SOURCE));
    crate::server::save_settings_patch(state, payload, SAVE_TIMEOUT_MS)
        .await
        .map_err(|failure| {
            if failure.code == "dependencyUnavailable" {
                error(
                    "dependencyUnavailable",
                    "Ghostex desktop app is not running. Open Ghostex and retry.",
                )
            } else {
                error("internalError", failure.message)
            }
        })?;
    let started = Instant::now();
    while started.elapsed() < CONFIRM_TIMEOUT {
        let saved = read_settings(state)?;
        if patch
            .iter()
            .all(|(key, value)| catalog::values_equal(&look::current(&saved, key), value))
        {
            return Ok(());
        }
        tokio::time::sleep(CONFIRM_POLL).await;
    }
    Err(error(
        "internalError",
        "Ghostex accepted the change but the saved settings did not show it within 3 seconds. Check Settings > Interface.",
    ))
}

/// The raw value a `changes` entry carries, as `ghostex settings set` would read it.
fn raw_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => catalog::format_value(other),
    }
}

async fn set(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let changes = params
        .get("changes")
        .and_then(Value::as_object)
        .filter(|changes| !changes.is_empty())
        .ok_or_else(|| {
            error(
                "badRequest",
                "Give the settings to change: `ghostex theme set <key>=<value> ...`.",
            )
        })?;
    let settings = read_settings(state)?;
    let mut items = Vec::new();
    let mut problems = Vec::new();
    for (key, raw) in changes {
        if let Some(targets) = look::role_targets(key) {
            let paths = match targets {
                Ok(paths) => paths,
                Err(message) => {
                    problems.push(message);
                    continue;
                }
            };
            let after = match look::role_value(&raw_value(raw)) {
                Ok(value) => value,
                Err(message) => {
                    problems.push(message);
                    continue;
                }
            };
            for path in paths {
                let before = look::current(&settings, &path);
                if before == after {
                    continue;
                }
                items.push(Item {
                    title: look::role_title(&path),
                    before_shown: look::role_shown(&before),
                    after_shown: look::role_shown(&after),
                    key: path,
                    before,
                    after: after.clone(),
                });
            }
            continue;
        }
        let entry = match look::look_entry(key, &settings) {
            Ok(entry) => entry,
            Err(message) => {
                problems.push(message);
                continue;
            }
        };
        let after = match catalog::parse_value(entry, &raw_value(raw)) {
            Ok(value) => value,
            Err(failure) => {
                problems.push(failure.to_string());
                continue;
            }
        };
        let before = look::current(&settings, &entry.key);
        if catalog::values_equal(&before, &after) {
            continue;
        }
        items.push(Item {
            key: entry.key.clone(),
            title: entry.title.clone(),
            before_shown: look::shown(entry, &before),
            after_shown: look::shown(entry, &after),
            before,
            after,
        });
    }
    if !problems.is_empty() {
        return Err(error("badRequest", problems.join("\n")));
    }
    if items.is_empty() {
        return Ok(json!({ "unchanged": true }));
    }
    let patch = look::patch_for(
        &settings,
        items.iter().map(|item| (item.key.as_str(), &item.after)),
    );
    save(state, patch).await?;
    let saved = read_settings(state)?;
    let keys: Vec<String> = items.iter().map(|item| item.key.clone()).collect();
    let change = ThemeChange {
        change_id: format!("theme-{}", uuid::Uuid::new_v4().simple()),
        at: now_iso(),
        by: who(state, caller, params),
        items,
        warnings: readability::warnings_from(&settings, &saved, &keys)
            .into_iter()
            .map(|warning| warning.text)
            .collect(),
        undone_at: None,
        undone_by: None,
    };
    journal::record(state_dir(state), change.clone())
        .map_err(|message| error("internalError", message))?;
    Ok(json!({ "change": summary(&change, &saved), "chatCard": card(&change) }))
}

/// `undo` (`undo: true`) or `redo` of `change`, else of the newest change that can be.
async fn undo_or_redo(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
    undo: bool,
) -> Result<Value, DomainStateError> {
    let settings = read_settings(state)?;
    let wanted = text(params, "change");
    let change = if wanted.is_empty() {
        journal::list(state_dir(state))
            .into_iter()
            .find(|change| change.undone_at.is_some() != undo)
            .ok_or_else(|| {
                error(
                    "notFound",
                    if undo {
                        "There is no theme change to undo."
                    } else {
                        "There is no undone theme change to redo."
                    },
                )
            })?
    } else {
        journal::find(state_dir(state), wanted).ok_or_else(|| {
            error(
                "notFound",
                format!("There is no theme change {wanted}. `ghostex theme history` lists them."),
            )
        })?
    };
    if let Some((code, message)) = problem(&change, &settings, undo) {
        return Err(error(code, message));
    }
    let patch = look::patch_for(
        &settings,
        change.items.iter().map(|item| {
            (
                item.key.as_str(),
                if undo { &item.before } else { &item.after },
            )
        }),
    );
    save(state, patch).await?;
    journal::mark(
        state_dir(state),
        &change.change_id,
        undo.then(|| (now_iso(), who(state, caller, params))),
    )
    .map_err(|message| error("internalError", message))?;
    let saved = read_settings(state)?;
    let keys: Vec<String> = change.items.iter().map(|item| item.key.clone()).collect();
    let change = journal::find(state_dir(state), &change.change_id).unwrap_or(change);
    Ok(json!({
        "change": summary(&change, &saved),
        "warnings": readability::warnings_from(&settings, &saved, &keys)
            .into_iter()
            .map(|warning| warning.text)
            .collect::<Vec<_>>(),
    }))
}

fn read(state: &AppState) -> Result<Value, DomainStateError> {
    let settings = read_settings(state)?;
    Ok(json!({
        "groups": look::grouped(&settings),
        "warnings": readability::warnings(&settings)
            .into_iter()
            .map(|warning| warning.text)
            .collect::<Vec<_>>(),
        "kept": journal::KEPT_CHANGES,
    }))
}

fn history(state: &AppState) -> Result<Value, DomainStateError> {
    let settings = read_settings(state)?;
    Ok(json!({
        "changes": journal::list(state_dir(state))
            .iter()
            .map(|change| summary(change, &settings))
            .collect::<Vec<_>>(),
    }))
}

fn one_change(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let settings = read_settings(state)?;
    let wanted = text(params, "change");
    let change = journal::find(state_dir(state), wanted).ok_or_else(|| {
        error(
            "notFound",
            "This theme change is no longer in the history, which keeps the last 20.",
        )
    })?;
    Ok(json!({ "change": summary(&change, &settings) }))
}

pub(crate) async fn handle_theme_http(
    state: Arc<AppState>,
    caller: Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(failure) => return domain_error_response(endpoint_path, request_id, failure),
    };
    let answer = match text(&params, "op") {
        "" | "read" => read(&state),
        "history" => history(&state),
        "change" => one_change(&state, &params),
        "set" => set(&state, &caller, &params).await,
        "undo" => undo_or_redo(&state, &caller, &params, true).await,
        "redo" => undo_or_redo(&state, &caller, &params, false).await,
        other => Err(error(
            "badRequest",
            format!("Unknown theme op \"{other}\"."),
        )),
    };
    match answer {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(failure) if failure.code == "dependencyUnavailable" => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error(failure.code, failure.message, Some(request_id)),
        ),
        Err(failure) => domain_error_response(endpoint_path, request_id, failure),
    }
}
