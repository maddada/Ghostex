//! The Mode menu's "Claude in Chrome" switch: one Claude Code chat connects or drops Claude in
//! Chrome, restarting Claude on its own conversation once it is idle.
//!
//! CDXC:SessionChat 2026-11-28 DECISION:
//! GitHub #790 and the user's answers on the Chrome switch page: Q5313 "B. Mode menu" (at the
//! bottom of the Mode pill menu, next to the approval levels), Q5314 "B. Claude in Chrome" with
//! the line "Browser tools for this chat", Q5315 "A. Restart when idle": an idle chat restarts at
//! once keeping its conversation, a busy one shows "Waiting for Claude to be idle" and restarts
//! once the reply ends. The switch starts on when the chat was created with `--chrome` or its
//! account's Claude Code has Chrome on by default (`claudeInChromeDefaultEnabled`), and it is
//! offered on Claude Code chats only, not in a box (which cannot reach this computer's Chrome).
//! WHY: Claude Code connects or drops Chrome only when it starts, so a change needs a restart, the
//! same limit and the same wait as the Mode menu's Full permissions switch
//! (session_chat_mode_restart.rs). The choice is kept on the chat (`CLAUDE_CHROME_KEY`), so every
//! later start, resume, fork and Native/Terminal switch of that one chat passes `--chrome` or
//! `--no-chrome` (acp/claude_chrome.rs). Only the user's own clients turn Chrome on, since it hands
//! Claude their signed-in browser; any client may turn it off or cancel a waiting restart.
//! SEE-ALSO: server/src/acp/claude_chrome.rs, packages/gx-chat-core/src/menus/chrome_switch.rs (the
//! row), apps/desktop/src/app/native_chat/option_menu/render.rs (the desktop's switch),
//! apps/mobile/app/src/chat/native/composer/ModelMenuSheet.tsx (the phone's Mode page).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::http::StatusCode;
use ghostex_gx_protocol::ClaudeChromeSwitch;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::acp::claude_chrome::{chat_starts_with_chrome, CLAUDE_CHROME_KEY};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::security::Principal;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_chat_compacting::SessionChatCompactingPublisher;
use crate::storage::open_gxserver_database;

/// Runtime-settings key holding a waiting restart, a JSON string (`Restart`).
const RESTART_KEY: &str = "sessionChatChromeRestart";
/// How often a waiting restart looks at the agent again.
const POLL: Duration = Duration::from_secs(2);

/// A restart that brings Chrome on or off, as the switch keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Restart {
    #[serde(default)]
    pending: bool,
    /// What the restart brings.
    #[serde(default)]
    on: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failed: Option<String>,
}

fn read_restart(session: &Value) -> Restart {
    session
        .pointer(&format!("/runtimeSettings/{RESTART_KEY}"))
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default()
}

fn runtime(session: &Value) -> Option<&Map<String, Value>> {
    session.get("runtimeSettings").and_then(Value::as_object)
}

/// Whether this chat has the switch: Claude Code, not in a box.
fn offered(session: &Value) -> bool {
    crate::agents::identity::normalize_agent_id(
        crate::session_chat_follower::session_chat_agent_for_session(session).as_deref(),
    )
    .as_deref()
        == Some("claude")
        && !crate::agentbox::is_agentbox_session(session)
}

/// The switch as the Mode menu shows it.
pub(crate) fn switch_state(session: &Value) -> ClaudeChromeSwitch {
    if !offered(session) {
        return ClaudeChromeSwitch::default();
    }
    let restart = read_restart(session);
    ClaudeChromeSwitch {
        offered: true,
        on: if restart.pending {
            restart.on
        } else {
            chat_starts_with_chrome(runtime(session))
        },
        pending: restart.pending,
        failed: restart.failed,
    }
}

/// The queue snapshot's reading of the switch, so it rides every frame (session_chat_queue.rs).
pub(crate) fn read_switch(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> ClaudeChromeSwitch {
    let json_column = |row: &rusqlite::Row<'_>, index: usize| -> Value {
        row.get::<_, String>(index)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(Value::Null)
    };
    db.query_row(
        "SELECT agentId, launchSettingsJson, runtimeSettingsJson FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id, session_id],
        |row| {
            Ok(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "agentId": row.get::<_, Option<String>>(0)?,
                "launchSettings": json_column(row, 1),
                "runtimeSettings": json_column(row, 2),
            }))
        },
    )
    .map(|session| switch_state(&session))
    .unwrap_or_default()
}

fn running_restarts() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(Mutex::default)
}

fn write_restart(state: &AppState, project_id: &str, session_id: &str, restart: &Restart) {
    let raw = (restart != &Restart::default())
        .then(|| serde_json::to_string(restart).ok())
        .flatten();
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        RESTART_KEY,
        raw.as_deref(),
    );
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
}

/// Keeps the chat's own choice, which every later launch of it reads (acp/claude_chrome.rs).
fn write_choice(state: &AppState, project_id: &str, session_id: &str, on: bool) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    db.execute(
        "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, ?3, json(?4)), updatedAt = ?5 WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![
            project_id,
            session_id,
            format!("$.{CLAUDE_CHROME_KEY}"),
            if on { "true" } else { "false" },
            crate::domain::now_iso(),
        ],
    )
    .is_ok_and(|changed| changed == 1)
}

fn read_session(state: &AppState, project_id: &str, session_id: &str) -> Option<Value> {
    let db = open_gxserver_database(&state.paths).ok()?;
    DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_session(project_id, session_id)
        .ok()
        .flatten()
}

/// `/api/setSessionClaudeChrome`: `{projectId, sessionId, on}` moves the switch. Back to what the
/// chat already runs with cancels a waiting restart.
pub(crate) async fn handle_set_session_claude_chrome_http(
    state: &AppState,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = async {
        let params = read_domain_rpc_params(body)?;
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        };
        let (project_id, session_id) = (text("projectId"), text("sessionId"));
        if project_id.is_empty() || session_id.is_empty() {
            return Err(DomainStateError::bad_request(
                "setSessionClaudeChrome requires projectId and sessionId.",
            ));
        }
        let Some(on) = params.get("on").and_then(Value::as_bool) else {
            return Err(DomainStateError::bad_request(
                "setSessionClaudeChrome requires on: true or false.",
            ));
        };
        let session = read_session(state, &project_id, &session_id)
            .ok_or_else(|| DomainStateError::not_found("Session not found."))?;
        if !offered(&session) {
            return Err(DomainStateError::bad_request(
                "Claude in Chrome is only for Claude Code chats that run on this computer.",
            ));
        }
        if on == chat_starts_with_chrome(runtime(&session)) {
            write_restart(state, &project_id, &session_id, &Restart::default());
            return Ok(json!({ "pending": false, "on": on }));
        }
        if on
            && !matches!(
                caller,
                Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
            )
        {
            return Err(DomainStateError::bad_request(
                "Only you can connect Chrome to a chat, from its Mode menu.",
            ));
        }
        write_restart(
            state,
            &project_id,
            &session_id,
            &Restart {
                pending: true,
                on,
                failed: None,
            },
        );
        spawn_restart(state, &project_id, &session_id);
        Ok(json!({ "pending": true, "on": on }))
    }
    .await;
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// gxserver start: the restarts that were waiting when it stopped wait again.
pub(crate) fn resume_waiting_restarts(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let Ok(mut statement) = db.prepare(
        "SELECT projectId, sessionId FROM sessions WHERE json_extract(runtimeSettingsJson, ?1) IS NOT NULL",
    ) else {
        return;
    };
    let waiting: Vec<(String, String)> = statement
        .query_map([format!("$.{RESTART_KEY}")], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();
    for (project_id, session_id) in waiting {
        if read_session(state, &project_id, &session_id)
            .is_some_and(|session| read_restart(&session).pending)
        {
            spawn_restart(state, &project_id, &session_id);
        }
    }
}

fn spawn_restart(state: &AppState, project_id: &str, session_id: &str) {
    let key = format!("{project_id}|{session_id}");
    if !running_restarts()
        .lock()
        .is_ok_and(|mut running| running.insert(key.clone()))
    {
        return;
    }
    let state = state.clone();
    let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
    tokio::spawn(async move {
        if let Err(why) = restart_when_idle(&state, &project_id, &session_id).await {
            write_restart(
                &state,
                &project_id,
                &session_id,
                &Restart {
                    pending: false,
                    on: false,
                    failed: Some(why),
                },
            );
        }
        if let Ok(mut running) = running_restarts().lock() {
            running.remove(&key);
        }
    });
}

/// Waits until Claude is idle and restarts it with the switch's choice. `Ok` also when the user
/// moved the switch back meanwhile. `Err` says why it did not happen, in the user's words.
async fn restart_when_idle(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    loop {
        let Some(session) = read_session(state, project_id, session_id) else {
            return Ok(());
        };
        let restart = read_restart(&session);
        if !restart.pending {
            return Ok(());
        }
        // A chat that isn't running starts with the choice the next time it wakes.
        if session["lifecycleState"].as_str() != Some("running") {
            write_choice(state, project_id, session_id, restart.on);
            write_restart(state, project_id, session_id, &Restart::default());
            return Ok(());
        }
        let target = crate::session_chat_send::resolve_session_chat_send_target(
            state,
            &params,
            "chromeRestart",
        )
        .map_err(|_| "the chat was closed".to_string())?;
        if !crate::session_chat_queue_runtime::can_start_on_its_conversation(state, &target.session)
            .await
        {
            return Err(
                "Claude has nothing to come back to yet. Send it a message first".to_string(),
            );
        }
        let busy = crate::session_chat_queue_runtime::agent_is_busy(&target.session).await
            || open_gxserver_database(&state.paths).is_ok_and(|db| {
                crate::session_chat_queue::session_has_pending_session_chat_queue(
                    &db, project_id, session_id,
                )
            });
        if !busy {
            match crate::session_chat_queue_runtime::work_a_restart_would_end(state, &target).await
            {
                Some(reason) if reason.contains("coordinates threads") => {
                    return Err(
                        "Claude coordinates threads, and a restart would end their work"
                            .to_string(),
                    );
                }
                Some(_) => {}
                None => return restart_now(state, project_id, session_id, restart.on).await,
            }
        }
        tokio::time::sleep(POLL).await;
    }
}

async fn restart_now(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    on: bool,
) -> Result<(), String> {
    let before = read_session(state, project_id, session_id)
        .and_then(|session| crate::acp::claude_chrome::claude_chrome_choice(runtime(&session)));
    write_choice(state, project_id, session_id, on);
    if let Err(failure) =
        crate::session_chat_queue_runtime::cycle_session_explained(state, project_id, session_id)
            .await
    {
        match before {
            Some(previous) => write_choice(state, project_id, session_id, previous),
            None => remove_choice(state, project_id, session_id),
        };
        return Err(format!("Ghostex couldn't restart Claude: {failure}"));
    }
    crate::session_chat_agent_restart::note_agent_restarted(state, project_id, session_id);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        if on {
            "sessionAgentRestartedWithChrome"
        } else {
            "sessionAgentRestartedWithoutChrome"
        },
        project_id,
        session_id,
        if on {
            "Restarted Claude on its own conversation with Claude in Chrome, as the user asked in the Mode menu."
        } else {
            "Restarted Claude on its own conversation without Claude in Chrome, as the user asked in the Mode menu."
        },
        &[],
    );
    write_restart(state, project_id, session_id, &Restart::default());
    Ok(())
}

fn remove_choice(state: &AppState, project_id: &str, session_id: &str) -> bool {
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        CLAUDE_CHROME_KEY,
        None,
    )
}
