//! The Mode menu's "Restart in Bypass when idle" switch: a Claude started without Bypass is
//! restarted on its own conversation, in Bypass, once it is idle, and keeps starting in Bypass.
//!
//! CDXC:SessionChat 2026-11-14 DECISION:
//! User, of the greyed Bypass row's "This session wasn't started with the skip-permissions flag…" line: "the message we have now for bypass permissions sucks. and we need to have a toggle right there and the user can pick 'reload with bypass permissions when agent is idle' so we apply bypass permissions". Beside a greyed Bypass, the Mode menu has a switch; turned on, the agent restarts in Bypass as soon as it is idle (at once when it already is), resumes its own conversation, and every later start of that one chat starts in Bypass too (`START_IN_BYPASS_KEY`, read by agents/resume_plan.rs). Turning it off before then cancels it. Only that one chat changes: no setting, no other session. The restart never cuts into work: it waits while the agent works, compacts, asks something or runs background tasks, and while messages wait in the queue. Only the user's own clients turn it on (the desktop app, a paired phone, the CLI in their terminal); an agent's own token, another computer and a Collab guest are refused, while any client may cancel.
//! WHY: Accept all has no rule about who may turn it on (unlike Trust project folders automatically, packages/settings-catalog/src/folder_trust.rs), and the menu could already pick Bypass for a Claude that allows it, so this is the same choice made one restart later. Side chats and read-only coordinator threads start in their own restricted modes on every launch, so they are not offered it.
//! SEE-ALSO: server/src/session_chat_mode_availability.rs (the greyed row and this switch's state), packages/gx-chat-core/src/menus/option_menus.rs (the menu rows), apps/desktop/src/app/native_chat/option_menu/render.rs (the switch), server/src/agents/bypass_consent.rs (Claude's warning, accepted for a bypass launch).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::http::StatusCode;
use ghostex_gx_protocol::ModeRestart;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::security::Principal;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_chat_compacting::SessionChatCompactingPublisher;
use crate::storage::open_gxserver_database;

/// Runtime-settings key holding the switch's state, a JSON string (`ModeRestart`).
pub(crate) const RESTART_KEY: &str = "sessionChatBypassRestart";
/// Runtime-settings key that makes every launch of this chat's Claude start in Bypass.
pub(crate) const START_IN_BYPASS_KEY: &str = "sessionChatStartInBypass";
const START_IN_BYPASS_ON: &str = "on";
/// How often a waiting restart looks at the agent again.
const POLL: Duration = Duration::from_secs(2);

/// Whether this chat's Claude starts in Bypass on every launch, because the user restarted it in
/// Bypass from the Mode menu.
pub(crate) fn starts_in_bypass(runtime_settings: &Map<String, Value>) -> bool {
    runtime_settings
        .get(START_IN_BYPASS_KEY)
        .and_then(Value::as_str)
        == Some(START_IN_BYPASS_ON)
}

/// The switch's state, as the menu shows it.
pub(crate) fn read_restart(session: &Value) -> ModeRestart {
    session
        .pointer(&format!("/runtimeSettings/{RESTART_KEY}"))
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default()
}

/// Whether a restart can bring Bypass to this Claude: never a side chat or a read-only thread,
/// which start in their own restricted modes on every launch, nor a box session.
pub(crate) fn restart_offered(session: &Value) -> bool {
    let runtime = session.get("runtimeSettings").and_then(Value::as_object);
    crate::side_chats::parent_of(session).is_none()
        && !crate::coordinators::is_read_only_thread(runtime)
        && crate::agentbox::refuse_for_agentbox_session(session, "").is_ok()
}

fn running_restarts() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(Mutex::default)
}

fn write(state: &AppState, project_id: &str, session_id: &str, key: &str, value: Option<&str>) {
    SessionChatCompactingPublisher::new(state).publish_marker(project_id, session_id, key, value);
}

fn write_restart(state: &AppState, project_id: &str, session_id: &str, restart: &ModeRestart) {
    let raw = (restart != &ModeRestart::default())
        .then(|| serde_json::to_string(restart).ok())
        .flatten();
    write(state, project_id, session_id, RESTART_KEY, raw.as_deref());
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
}

fn read_session(state: &AppState, project_id: &str, session_id: &str) -> Option<Value> {
    let db = open_gxserver_database(&state.paths).ok()?;
    DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_session(project_id, session_id)
        .ok()
        .flatten()
}

/// `/api/restartSessionInMode`: `{projectId, sessionId, mode: "bypass", enabled}` turns the
/// switch on or off.
pub(crate) async fn handle_restart_session_in_mode_http(
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
                "restartSessionInMode requires projectId and sessionId.",
            ));
        }
        if text("mode") != "bypass" {
            return Err(DomainStateError::bad_request(
                "Only Bypass can be turned on with a restart.",
            ));
        }
        let enabled = params.get("enabled").and_then(Value::as_bool) == Some(true);
        let session = read_session(state, &project_id, &session_id)
            .ok_or_else(|| DomainStateError::not_found("Session not found."))?;
        if !enabled {
            write_restart(state, &project_id, &session_id, &ModeRestart::default());
            return Ok(json!({ "pending": false }));
        }
        if !matches!(
            caller,
            Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
        ) {
            return Err(DomainStateError::bad_request(
                "Only you can turn on Bypass for a chat, from its Mode menu.",
            ));
        }
        if !crate::session_chat_mode_availability::bypass_restart_offered(&session) {
            return Err(DomainStateError::bad_request(
                "This Claude can't be restarted in Bypass.",
            ));
        }
        write_restart(
            state,
            &project_id,
            &session_id,
            &ModeRestart {
                pending: true,
                failed: None,
            },
        );
        spawn_restart(state, &project_id, &session_id);
        Ok(json!({ "pending": true }))
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
                &ModeRestart {
                    pending: false,
                    failed: Some(why),
                },
            );
        }
        if let Ok(mut running) = running_restarts().lock() {
            running.remove(&key);
        }
    });
}

/// Waits until the agent is idle and restarts it in Bypass. `Ok` also when the user turned the
/// switch off meanwhile. `Err` says why it did not happen, in the user's words.
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
        if !read_restart(&session).pending {
            return Ok(());
        }
        // A chat that isn't running starts in Bypass the next time it wakes; nothing to restart.
        if session["lifecycleState"].as_str() != Some("running") {
            write(
                state,
                project_id,
                session_id,
                START_IN_BYPASS_KEY,
                Some(START_IN_BYPASS_ON),
            );
            write_restart(state, project_id, session_id, &ModeRestart::default());
            return Ok(());
        }
        let target = crate::session_chat_send::resolve_session_chat_send_target(
            state,
            &params,
            "bypassRestart",
        )
        .map_err(|_| "the chat was closed".to_string())?;
        if !crate::session_chat_queue_runtime::has_conversation_to_resume(&target.session).await {
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
                // A coordinator's threads keep it busy for good; background tasks end on their own.
                Some(reason) if reason.contains("coordinates threads") => {
                    return Err(
                        "Claude coordinates threads, and a restart would end their work"
                            .to_string(),
                    );
                }
                Some(_) => {}
                None => return restart(state, project_id, session_id).await,
            }
        }
        tokio::time::sleep(POLL).await;
    }
}

async fn restart(state: &AppState, project_id: &str, session_id: &str) -> Result<(), String> {
    write(
        state,
        project_id,
        session_id,
        START_IN_BYPASS_KEY,
        Some(START_IN_BYPASS_ON),
    );
    if let Err(failure) =
        crate::session_chat_queue_runtime::cycle_session_explained(state, project_id, session_id)
            .await
    {
        write(state, project_id, session_id, START_IN_BYPASS_KEY, None);
        return Err(format!("Ghostex couldn't restart Claude: {failure}"));
    }
    crate::session_chat_agent_restart::note_agent_restarted(state, project_id, session_id);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionAgentRestartedInBypass",
        project_id,
        session_id,
        "Restarted Claude on its own conversation in Bypass permissions, as the user asked in the Mode menu.",
        &[],
    );
    write_restart(state, project_id, session_id, &ModeRestart::default());
    Ok(())
}
