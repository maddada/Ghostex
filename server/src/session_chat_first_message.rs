//! A new coordinator's first message (its skill line, then the user's first request), typed into
//! its agent's input box and left unsent until the user starts it: Start on the chat's card, Enter
//! in the chat box, Enter in the terminal, or any other message to the session.
//!
//! CDXC:Coordinators 2026-11-18 DECISION:
//! User: "we need to paste the message to send to the coordinator and not send it. so the user can pick any agent they want before we start the coordinator". This replaces the 13 Nov auto-send (Q2980) for New Coordinator on the desktop and the phone and for `ghostex coordinator create` (whose `--start` still sends at once); Autopilot's Chief, the voice agent and other callers that pass no `holdFirstMessage` still send it at once. From the design page the user picked: Q3950 A, the message in a card inside the chat box with Show all, Edit and Start; Q3951, a model or effort change from the chat while it waits takes the message out of the agent's box, runs the agent's picker and types it back (`lift_for_model_change`, `put_back`); Q3952, Enter with typed text starts the coordinator with the first message and queues the typed text behind it (`start_before_send`); Q3953, Edit moves the message into the chat box (`edit`). Enter in the terminal starts it too, and the card goes (`watch`).
//! SEE-ALSO: packages/gx-chat-core/src/composer/first_message.rs (the card and its actions), apps/desktop/src/app/native_chat/first_message_card.rs and apps/mobile/app/src/chat/native/composer/FirstMessageCard.tsx (the two cards), server/src/session_chat_model_selection.rs (`sender`), server/src/server/coordinator_runtime.rs (`start_orchestrator_role`).
//!
//! WHY: Start does not press Enter on the typed text. It takes the text out of the box and hands it
//! to the startup send every new coordinator used before (`queue_session_chat_prompt`), so the
//! message gets the chat's bubble, the wait for the input box and the delivery checks every send
//! has, also when it never reached the box (an agent whose box Ghostex cannot read).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;
use crate::session_chat_send::SessionChatSendTarget;
use crate::storage::open_gxserver_database;

/// Not in the agent's box yet: its input box is not up, or something else holds it.
pub(crate) const WAITING: &str = "waiting";
/// Typed into the agent's box, unsent.
pub(crate) const TYPED: &str = "typed";
/// Taken out of the box while a model or effort change runs; typed back after it.
pub(crate) const LIFTED: &str = "lifted";
/// Gone from the box without reaching the agent (cleared or replaced in the terminal). Start
/// still sends it; it is not typed again.
pub(crate) const LEFT: &str = "left";

/// How often the watch looks at the session: often while a new coordinator is fresh, then less.
const WATCH_FAST: Duration = Duration::from_millis(1_500);
const WATCH_SLOW: Duration = Duration::from_secs(10);
const WATCH_FAST_FOR: Duration = Duration::from_secs(10 * 60);

/// How long a message taken out for a slash command stays out before the watch types it back,
/// so the command is in the box (and its picker open) first.
const LIFTED_SETTLE: Duration = Duration::from_secs(4);

fn settled_since(updated_at: &str, settle: Duration) -> bool {
    chrono::DateTime::parse_from_rfc3339(updated_at).map_or(true, |at| {
        chrono::Utc::now()
            .signed_duration_since(at)
            .num_milliseconds()
            >= i64::try_from(settle.as_millis()).unwrap_or(i64::MAX)
    })
}

/// A waiting first message, as the chat reads it (`firstMessage` on the chat's queue snapshot).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstMessage {
    pub text: String,
    pub state: String,
    pub created_at: String,
    /// When the state last changed; not sent to the chat.
    #[serde(skip)]
    pub updated_at: String,
}

pub fn read(db: &Connection, project_id: &str, session_id: &str) -> Option<FirstMessage> {
    db.query_row(
        "SELECT text, state, createdAt, updatedAt FROM session_chat_first_messages WHERE projectId = ?1 AND sessionId = ?2",
        params![project_id, session_id],
        |row| {
            Ok(FirstMessage {
                text: row.get(0)?,
                state: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn read_now(state: &AppState, project_id: &str, session_id: &str) -> Option<FirstMessage> {
    open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| read(&db, project_id, session_id))
}

/// Moves the record from one of `from` to `to`; answers whether it moved.
fn set_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    from: &[&str],
    to: &str,
) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let current = read(&db, project_id, session_id).map(|message| message.state);
    if !current.is_some_and(|current| from.contains(&current.as_str())) {
        return false;
    }
    db.execute(
        "UPDATE session_chat_first_messages SET state = ?3, updatedAt = ?4 WHERE projectId = ?1 AND sessionId = ?2",
        params![project_id, session_id, to, now_iso()],
    )
    .is_ok_and(|count| count > 0)
}

/// Deletes the record and answers it.
fn take(state: &AppState, project_id: &str, session_id: &str) -> Option<FirstMessage> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let message = read(&db, project_id, session_id)?;
    db.execute(
        "DELETE FROM session_chat_first_messages WHERE projectId = ?1 AND sessionId = ?2",
        params![project_id, session_id],
    )
    .ok()?;
    Some(message)
}

fn storage_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn broadcast(state: &AppState, project_id: &str, session_id: &str) {
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
}

fn log(state: &AppState, event: &str, project_id: &str, session_id: &str, details: Value) {
    let mut details = details;
    details["projectId"] = json!(project_id);
    details["sessionId"] = json!(session_id);
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Info,
        event: event.into(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// One lock per session for everything that types into or out of its box on the first message's
/// behalf, so the watch never types while Start, Edit or a model change works on the box.
fn box_lock(project_id: &str, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Default::default);
    let key = format!("{project_id}|{session_id}");
    LOCKS
        .lock()
        .map(|mut locks| locks.entry(key).or_default().clone())
        .unwrap_or_default()
}

/// Whether a create asked to hold its coordinator's first message (`coordinator.holdFirstMessage`).
pub(crate) fn create_holds_first_message(params: &Map<String, Value>) -> bool {
    params
        .get("coordinator")
        .and_then(|coordinator| coordinator.get("holdFirstMessage"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Keeps `text` as the session's first message, to be typed into its agent's box once it is up.
pub(crate) fn hold(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| storage_error(error))?;
    let now = now_iso();
    db.execute(
        "INSERT OR REPLACE INTO session_chat_first_messages (projectId, sessionId, text, state, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![project_id, session_id, text, WAITING, now],
    )
    .map_err(|error| storage_error(error))?;
    drop(db);
    broadcast(state, project_id, session_id);
    spawn_watch(state, project_id, session_id);
    Ok(())
}

/// Watches every waiting first message again after gxserver starts. One a model change had taken
/// out of the box goes back in.
pub(crate) fn resume_all(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let _ = db.execute(
        "UPDATE session_chat_first_messages SET state = ?1 WHERE state = ?2",
        params![WAITING, LIFTED],
    );
    // CDXC:AgentSkills 2026-11-20: a message not in the agent's box gets the current coordinator
    // line now (the retired skill's line before, coordinators/role.rs
    // `with_current_coordinator_line`); a typed one keeps it until Start, which must find the box
    // holding exactly what was typed.
    if let Ok(mut held) = db.prepare(
        "SELECT projectId, sessionId, text FROM session_chat_first_messages WHERE state IN (?1, ?2)",
    ) {
        let rows: Vec<(String, String, String)> = held
            .query_map(params![WAITING, LEFT], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default();
        for (project_id, session_id, text) in rows {
            if let Some(current) = crate::coordinators::with_current_coordinator_line(&text) {
                let _ = db.execute(
                    "UPDATE session_chat_first_messages SET text = ?3, updatedAt = ?4 WHERE projectId = ?1 AND sessionId = ?2",
                    params![project_id, session_id, current, now_iso()],
                );
            }
        }
    }
    let Ok(mut statement) =
        db.prepare("SELECT projectId, sessionId FROM session_chat_first_messages")
    else {
        return;
    };
    let waiting: Vec<(String, String)> = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();
    for (project_id, session_id) in waiting {
        spawn_watch(state, &project_id, &session_id);
    }
}

fn spawn_watch(state: &AppState, project_id: &str, session_id: &str) {
    static WATCHING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
    let key = format!("{project_id}|{session_id}");
    if !WATCHING
        .lock()
        .is_ok_and(|mut watching| watching.insert(key.clone()))
    {
        return;
    }
    let state = state.clone();
    let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
    tokio::spawn(async move {
        watch(&state, &project_id, &session_id).await;
        if let Ok(mut watching) = WATCHING.lock() {
            watching.remove(&key);
        }
    });
}

/// Types the message into the box once the agent shows it, then waits for it to leave: Start (the
/// record is gone) or Enter in the terminal (the box no longer holds it and the agent works).
async fn watch(state: &AppState, project_id: &str, session_id: &str) {
    let started = Instant::now();
    loop {
        let pause = if started.elapsed() < WATCH_FAST_FOR {
            WATCH_FAST
        } else {
            WATCH_SLOW
        };
        tokio::time::sleep(pause).await;
        if read_now(state, project_id, session_id).is_none() {
            return;
        }
        let lock = box_lock(project_id, session_id);
        let _guard = lock.lock().await;
        let Some(message) = read_now(state, project_id, session_id) else {
            return;
        };
        let target = match target(state, project_id, session_id) {
            Ok(target) => target,
            Err(error) if error.code == "notFound" => {
                // The session was deleted.
                take(state, project_id, session_id);
                return;
            }
            Err(_) => continue,
        };
        match message.state.as_str() {
            WAITING => {
                if ready_to_type(state, &target).await
                    && type_into_box(state, &target, &message.text).await
                    && set_state(state, project_id, session_id, &[WAITING], TYPED)
                {
                    log(
                        state,
                        "coordinatorFirstMessageTyped",
                        project_id,
                        session_id,
                        json!({}),
                    );
                    broadcast(state, project_id, session_id);
                }
            }
            TYPED => match box_reading(&target, &message.text).await {
                BoxReading::HoldsIt | BoxReading::Unreadable => {}
                BoxReading::Other => {
                    if agent_took_it(&target) {
                        take(state, project_id, session_id);
                        log(
                            state,
                            "coordinatorFirstMessageSentFromTerminal",
                            project_id,
                            session_id,
                            json!({}),
                        );
                        broadcast(state, project_id, session_id);
                        return;
                    }
                    if set_state(state, project_id, session_id, &[TYPED], LEFT) {
                        broadcast(state, project_id, session_id);
                    }
                }
            },
            // Taken out for a slash command typed in the chat (`before_send`), or left out by a
            // model change that could not type it back: back in once the box is free again.
            LIFTED
                if settled_since(&message.updated_at, LIFTED_SETTLE)
                    && ready_to_type(state, &target).await =>
            {
                if type_into_box(state, &target, &message.text).await
                    && set_state(state, project_id, session_id, &[LIFTED], TYPED)
                {
                    broadcast(state, project_id, session_id);
                }
            }
            // Typed back by hand and sent from the terminal.
            LEFT if agent_took_it(&target) => {
                take(state, project_id, session_id);
                broadcast(state, project_id, session_id);
                return;
            }
            _ => {}
        }
    }
}

fn target(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<SessionChatSendTarget, DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    crate::session_chat_send::resolve_session_chat_send_target(state, &params, "firstMessage")
}

fn agent_of(target: &SessionChatSendTarget) -> Option<String> {
    crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(&target.session))
}

/// The agent took a message: it works on it (or already asks the user something about it).
fn agent_took_it(target: &SessionChatSendTarget) -> bool {
    let activity = crate::presentation::presentation_activity(&target.session, &now_iso());
    matches!(activity.as_str(), "working" | "attention")
}

/// Whether the message can be typed now: the agent runs and shows its input box, and nothing else
/// is on its way into the box (the queue, as a ZCode or OpenCode coordinator's `/model` line, or a
/// model change).
async fn ready_to_type(state: &AppState, target: &SessionChatSendTarget) -> bool {
    if target.session["lifecycleState"].as_str() != Some("running")
        || crate::agentbox::pending_session_agentbox(&target.session).is_some()
    {
        return false;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    if crate::session_chat_queue::session_has_pending_session_chat_queue(
        &db,
        &target.project_id,
        &target.session_id,
    ) || crate::session_chat_model_selection::has_pending_selection(
        state,
        &target.project_id,
        &target.session_id,
    ) {
        return false;
    }
    drop(db);
    // Pi and OMP take the text through their chat channel (`place_terminal_text`).
    if crate::pi_chat_actions::is_pi_family(&target.session)
        && crate::pi_chat_channel::live(&target.project_id, &target.session_id).is_some()
    {
        return true;
    }
    let Some(agent) = agent_of(target) else {
        return false;
    };
    let Some(screen) =
        crate::session_chat_send::capture_session_terminal_text_vt(&target.zmx_name).await
    else {
        return false;
    };
    let notice =
        crate::session_chat_notice::classify_session_chat_terminal_notice(Some(&agent), &screen);
    let ready = crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some(&agent),
        &screen,
        notice.as_ref(),
    );
    ready.state == crate::session_chat_composer::SessionChatComposerState::Ready
        && crate::session_chat_composer::session_chat_composer_input(&agent, &screen)
            .is_some_and(|input| input.is_empty())
}

async fn type_into_box(state: &AppState, target: &SessionChatSendTarget, text: &str) -> bool {
    match crate::session_chat_send::place_terminal_text(state, target, text).await {
        Ok(()) => true,
        Err(error) => {
            log(
                state,
                "coordinatorFirstMessageNotTyped",
                &target.project_id,
                &target.session_id,
                json!({ "reason": error.message }),
            );
            false
        }
    }
}

enum BoxReading {
    /// The box holds the message and nothing else.
    HoldsIt,
    /// The box is empty or holds other text.
    Other,
    /// No box could be read.
    Unreadable,
}

async fn box_reading(target: &SessionChatSendTarget, text: &str) -> BoxReading {
    let Some(agent) = agent_of(target) else {
        return BoxReading::Unreadable;
    };
    let Some(screen) =
        crate::session_chat_send::capture_session_terminal_text_vt(&target.zmx_name).await
    else {
        return BoxReading::Unreadable;
    };
    let Some(input) = crate::session_chat_composer::session_chat_composer_input(&agent, &screen)
    else {
        return BoxReading::Unreadable;
    };
    if input.text_unreadable() {
        return BoxReading::Unreadable;
    }
    if !input.is_empty() && box_holds_only(&input.text, text) {
        BoxReading::HoldsIt
    } else {
        BoxReading::Other
    }
}

/// Whether the input box text `shown` is the message `text` and nothing else: the whole text
/// (wrapping and spacing aside), the end of it in a box that scrolled, or the agent's folded paste
/// placeholder (`[Pasted text #1 +3 lines]`) alone.
pub(crate) fn box_holds_only(shown: &str, text: &str) -> bool {
    let normalize = crate::session_chat_send::normalize_session_chat_screen_text;
    let shown_normalized = normalize(shown);
    if shown_normalized.is_empty() {
        return false;
    }
    if shown_normalized == normalize(text) {
        return true;
    }
    let lone_placeholder = {
        let trimmed = shown.trim();
        trimmed.starts_with('[')
            && trimmed.ends_with(']')
            && trimmed.matches('[').count() == 1
            && trimmed.to_lowercase().starts_with("[paste")
    };
    lone_placeholder || crate::session_chat_send::input_box_shows_scrolled_message(shown, text)
}

/// Clears the box when it holds the message and nothing else; answers whether it now is free of
/// it. A box holding anything else is left alone, so text the user typed in the terminal is never
/// erased.
async fn lift_out(target: &SessionChatSendTarget, text: &str) -> bool {
    match box_reading(target, text).await {
        BoxReading::HoldsIt => {}
        BoxReading::Other => return true,
        BoxReading::Unreadable => return false,
    }
    let agent = agent_of(target);
    crate::session_chat_send::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "coordinator-first-message",
        crate::session_chat_send::build_session_chat_clear_input_steps(agent.as_deref(), text),
    )
    .await
    .is_ok()
        && matches!(box_reading(target, text).await, BoxReading::Other)
}

/// Start: sends the waiting first message. Answers whether there was one.
pub(crate) async fn start(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    let lock = box_lock(project_id, session_id);
    let _guard = lock.lock().await;
    let Some(message) = read_now(state, project_id, session_id) else {
        return Ok(false);
    };
    if message.state == TYPED {
        if let Ok(target) = target(state, project_id, session_id) {
            // Left in the box, the send would save it as the terminal's draft before typing it.
            lift_out(&target, &message.text).await;
        }
    }
    take(state, project_id, session_id);
    // Typed before the update, the box held the retired skill's line; the agent gets the current one.
    let text =
        crate::coordinators::with_current_coordinator_line(&message.text).unwrap_or(message.text);
    crate::server::queue_session_chat_prompt(state, project_id, session_id, &text, true)?;
    log(
        state,
        "coordinatorFirstMessageStarted",
        project_id,
        session_id,
        json!({}),
    );
    broadcast(state, project_id, session_id);
    Ok(true)
}

/// Before a message is sent to a session whose first message waits (Q3952): a message (Enter in
/// the chat box, a form's answers, `ghostex agents send`) starts the first message, so it goes
/// behind it; a slash command alone (`/model`, `/effort`) runs with the first message taken out of
/// the box, and the watch types it back once the command is done. Raw keys (Shift+Tab) pass.
pub(crate) async fn before_send(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    has_images: bool,
) {
    let Some(message) = read_now(state, project_id, session_id) else {
        return;
    };
    let trimmed = text.trim();
    if trimmed.is_empty() && !has_images {
        return;
    }
    let slash_command = !has_images && trimmed.starts_with('/') && !trimmed.contains('\n');
    if !slash_command {
        let _ = start(state, project_id, session_id).await;
        return;
    }
    if message.state != TYPED {
        return;
    }
    let lock = box_lock(project_id, session_id);
    let _guard = lock.lock().await;
    if let Ok(target) = target(state, project_id, session_id) {
        if lift_out(&target, &message.text).await
            && set_state(state, project_id, session_id, &[TYPED], LIFTED)
        {
            broadcast(state, project_id, session_id);
            spawn_watch(state, project_id, session_id);
        }
    }
}

/// Edit: takes the message out of the agent's box and answers it, for the chat box (Q3953).
pub(crate) async fn edit(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, DomainStateError> {
    let lock = box_lock(project_id, session_id);
    let _guard = lock.lock().await;
    let Some(message) = read_now(state, project_id, session_id) else {
        return Ok(None);
    };
    if message.state == TYPED {
        if let Ok(target) = target(state, project_id, session_id) {
            lift_out(&target, &message.text).await;
        }
    }
    take(state, project_id, session_id);
    log(
        state,
        "coordinatorFirstMessageEdited",
        project_id,
        session_id,
        json!({}),
    );
    broadcast(state, project_id, session_id);
    Ok(Some(message.text))
}

/// Held while a model or effort change runs on a session whose first message waits: nothing is
/// typed into its box meanwhile, and the message is back in it once the guard is handed to
/// [`put_back`].
pub(crate) struct ModelChangeGuard {
    _lock: tokio::sync::OwnedMutexGuard<()>,
    lifted: bool,
}

/// Takes the waiting first message out of the agent's box before a model or effort change, so the
/// agent's picker finds an empty box (Q3951). `None` when the session has no first message waiting.
/// A box holding other text too is left as it is, and the picker's own guard against replacing a
/// draft decides.
pub(crate) async fn lift_for_model_change(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<ModelChangeGuard> {
    read_now(state, project_id, session_id)?;
    let lock = box_lock(project_id, session_id).lock_owned().await;
    let message = read_now(state, project_id, session_id)?;
    let mut lifted = false;
    if message.state == TYPED {
        if let Ok(target) = target(state, project_id, session_id) {
            if lift_out(&target, &message.text).await
                && set_state(state, project_id, session_id, &[TYPED], LIFTED)
            {
                lifted = true;
                log(
                    state,
                    "coordinatorFirstMessageLifted",
                    project_id,
                    session_id,
                    json!({}),
                );
                broadcast(state, project_id, session_id);
            }
        }
    }
    Some(ModelChangeGuard {
        _lock: lock,
        lifted,
    })
}

/// Types the message back after a model or effort change. When it cannot go back yet (the agent
/// still shows its picker), the watch types it once the box is free again.
pub(crate) async fn put_back(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    guard: ModelChangeGuard,
) {
    if !guard.lifted {
        return;
    }
    let Some(message) = read_now(state, project_id, session_id) else {
        return;
    };
    let typed = match target(state, project_id, session_id) {
        Ok(target) => {
            let mut typed = false;
            // The picker may still be closing.
            for _ in 0..6 {
                if ready_to_type_after_change(&target).await {
                    typed = type_into_box(state, &target, &message.text).await;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            typed
        }
        Err(_) => false,
    };
    set_state(
        state,
        project_id,
        session_id,
        &[LIFTED],
        if typed { TYPED } else { WAITING },
    );
    log(
        state,
        "coordinatorFirstMessagePutBack",
        project_id,
        session_id,
        json!({ "typed": typed }),
    );
    broadcast(state, project_id, session_id);
    drop(guard);
    spawn_watch(state, project_id, session_id);
}

/// [`ready_to_type`] without the model change's own pending row, which is still stored while the
/// change finishes.
async fn ready_to_type_after_change(target: &SessionChatSendTarget) -> bool {
    let Some(agent) = agent_of(target) else {
        return false;
    };
    let Some(screen) =
        crate::session_chat_send::capture_session_terminal_text_vt(&target.zmx_name).await
    else {
        return false;
    };
    let ready = crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some(&agent),
        &screen,
        None,
    );
    ready.state == crate::session_chat_composer::SessionChatComposerState::Ready
        && crate::session_chat_composer::session_chat_composer_input(&agent, &screen)
            .is_some_and(|input| input.is_empty())
}

/// `/api/startSessionChatFirstMessage` and `/api/editSessionChatFirstMessage`.
pub(crate) async fn handle_first_message_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> crate::server::RoutedResponse {
    let params = match crate::domain::read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return crate::server::domain_error_response(endpoint_path, request_id, error)
        }
    };
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
        return crate::server::domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request("This needs projectId and sessionId."),
        );
    }
    let result = if endpoint_path.ends_with("editSessionChatFirstMessage") {
        edit(state, &project_id, &session_id)
            .await
            .map(|text| json!({ "edited": text.is_some(), "text": text.unwrap_or_default() }))
    } else {
        start(state, &project_id, &session_id)
            .await
            .map(|started| json!({ "started": started }))
    };
    match result {
        Ok(value) => crate::server::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, value),
        ),
        Err(error) => crate::server::domain_error_response(endpoint_path, request_id, error),
    }
}

#[cfg(test)]
mod tests {
    use super::box_holds_only;

    const MESSAGE: &str =
        "Read and adhere to this skill: /ghostex-coordinator\n\nTidy the release notes.";

    #[test]
    fn the_box_holds_the_message_however_it_wraps() {
        assert!(box_holds_only(
            "Read and adhere to this skill: /ghostex-coordinator\n\nTidy the\nrelease notes.",
            MESSAGE
        ));
        assert!(box_holds_only("[Pasted text #1 +3 lines]", MESSAGE));
    }

    #[test]
    fn other_text_in_the_box_is_never_the_message() {
        assert!(!box_holds_only("", MESSAGE));
        assert!(!box_holds_only("my own draft", MESSAGE));
        assert!(!box_holds_only(
            "Read and adhere to this skill: /ghostex-coordinator Tidy the release notes. and more",
            MESSAGE
        ));
        assert!(!box_holds_only(
            "[Pasted text #1 +3 lines] and mine",
            MESSAGE
        ));
    }
}
