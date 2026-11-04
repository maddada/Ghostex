//! A chat message that could not be sent for good: the card in its thread, the red dot on its
//! sidebar row and the row in the Notifications panel. All three come from one record kept with the
//! session, so they stand and clear together, and across a gxserver restart.
//!
//! The message itself is a `failed` row of the session's queue. A send from the chat box that
//! fails for good becomes one (session_chat_send/send_http.rs), so its text stays in the thread
//! with the reason beside it instead of going back to the chat box unexplained.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::Notify;

use crate::domain::{DomainRepository, DomainStateError};
use crate::notification_feed::store::{
    bounded_notification_body, insert_notification_feed_row, mark_session_send_failure_read,
    notification_agent_icon, NewNotificationFeedRow, NOTIFICATION_FEED_KIND_SEND_FAILED,
};
use crate::server::AppState;
use crate::session_chat_compacting::SessionChatCompactingPublisher;
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource, SESSION_CHAT_NOTICE_DELIVERY_FAILED,
};
use crate::storage::open_gxserver_database;

/// Runtime-settings key holding the record, a JSON string read back by [`session_send_failure`].
pub(crate) const SEND_FAILURE_KEY: &str = "sessionChatSendFailure";
/// Runtime-settings key set while a queued row waits on a session nobody will wake by itself.
pub(crate) const HELD_ON_CLOSED_SESSION_KEY: &str = "sessionChatHeldOnClosedSession";
/// What the dot's tooltip says about a row held on a closed session.
pub(crate) const HELD_ON_CLOSED_SESSION_REASON: &str =
    "A message is waiting for this session to be opened again.";

/// A held row shows its reason after this long (`WAIT_REASON_SHOWN_AFTER_MS`); the dot follows.
const HELD_ON_CLOSED_DOT_AFTER: Duration = Duration::from_secs(60);
/// How often a failed row is compared with the state its card describes.
const RECONCILE_EVERY: Duration = Duration::from_secs(2);
/// How many (row, reason) pairs are remembered; past this a very old row could notify again.
const NOTIFIED_REMEMBERED: usize = 500;
/// How often the transcript is asked whether a failed message arrived after all.
const TRANSCRIPT_CHECK_EVERY: Duration = Duration::from_secs(15);

pub(crate) const ACTION_RETRY: &str = "retrySend";
pub(crate) const ACTION_START_FRESH: &str = "startFreshSend";
pub(crate) const ACTION_DISMISS: &str = "dismissSendFailure";
const ACTION_FIX: &str = crate::session_chat_notice::SESSION_CHAT_NOTICE_ACTION_FIX_SEND;

/// The way out the card offers, decided from what the agent is doing when the send fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SendFailureNext {
    /// The agent is running: send it again as things stand, or restart the agent first.
    RetryOrFix,
    /// The agent is gone and its conversation can be resumed: a retry would do the same restart.
    Fix,
    /// The agent is gone with no conversation to resume: only a new conversation can take it.
    StartFresh,
}

/// The record behind the card, the dot and the notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendFailure {
    pub(crate) prompt_id: String,
    pub(crate) reason: String,
    pub(crate) at: String,
    pub(crate) next: SendFailureNext,
}

/// CDXC:SessionChat 2026-10-31 DECISION:
/// GitHub #374, user: a failed send always shows a card in the thread with the real reason, offering Retry and Open terminal and keeping Fix it where it is the right next step, and the spinner stops. Sidebar: "show only what needs the user", a dot for failed sends and for rows held on a closed session, never for a card the transcript has not confirmed. "No separate errors list": "Do notification for it with red dot there", so a failed send also goes to the Notifications panel, once per failure.
/// WHY: the card used to be a 10-minute, in-memory watchdog verdict: gone when the user looked after ten minutes or gxserver restarted, cleared by the next state frame of a follower that had not seen it, and published only for two of the ways a send can end (never for an input box that would not clear, a queued row or an agent the watchdog does not cover). A record kept with the session has none of those holes. Retry types the same message again without touching the agent; Fix it restarts the agent on its conversation and then sends it; so they are offered together only while the agent runs, and a dead agent gets only the restart, since its Retry would do the same restart.
/// SEE-ALSO: session_chat_queue.rs (`fail_prompt` reports every row that fails), session_chat_send/send_http.rs, session_chat_notice/watchdog_store.rs (`resolve_session_chat_terminal_notice`), packages/gx-core/src/sidebar_view/rows.rs, apps/desktop/src/notification_feed/mod.rs, packages/gx-chat-core/src/questions/document.rs.
pub(crate) fn send_failed_notice(failure: &SendFailure) -> SessionChatTerminalNotice {
    let guide = match failure.next {
        SendFailureNext::RetryOrFix => "Retry, or Fix it to restart the agent and send it.",
        SendFailureNext::Fix => "Fix it restarts the agent and sends it.",
        SendFailureNext::StartFresh => "Start fresh starts the agent over and sends it.",
    };
    let terminal = SessionChatTerminalNoticeAction::switch_to_terminal("Terminal view");
    let actions = match failure.next {
        SendFailureNext::RetryOrFix => vec![
            SessionChatTerminalNoticeAction::retry_send(),
            terminal,
            SessionChatTerminalNoticeAction::fix_send(),
        ],
        SendFailureNext::Fix => vec![SessionChatTerminalNoticeAction::fix_send(), terminal],
        SendFailureNext::StartFresh => {
            vec![
                SessionChatTerminalNoticeAction::start_fresh_send(),
                terminal,
            ]
        }
    };
    let mut notice = SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_DELIVERY_FAILED,
        SessionChatTerminalNoticeSeverity::Error,
        SessionChatTerminalNoticeSource::Watchdog,
        "Message not sent",
    )
    .with_detail(format!("{} {guide}", failure.reason))
    .with_actions(actions);
    notice.detected_at = failure.at.clone();
    notice.send_failure_prompt_id = Some(failure.prompt_id.clone());
    notice
}

/// What a refused send's step failed at, in the words the card and the log use.
pub(crate) fn send_failure_reason(step: &str, heal_gave_up: Option<&str>) -> String {
    let step = step.trim();
    let step = step
        .strip_suffix(" Your chat draft has been kept.")
        .unwrap_or(step);
    // The clear's own wording names no cause, and "the agent has no conversation to resume" was
    // only in the log (GitHub #374, example 4).
    let step = if step == "The terminal draft could not be cleared and verified." {
        "The agent's input box had text in it that Ghostex couldn't clear."
    } else {
        step
    };
    match heal_gave_up.and_then(heal_reason_sentence) {
        Some(why) => format!("{step} {why}"),
        None => step.to_string(),
    }
}

/// Why the send recovery could not fix it, as a sentence. `None` when the step already says all
/// of it: the turn that sat on the message is the same fact as "the agent is in the middle of a turn".
fn heal_reason_sentence(reason: &str) -> Option<String> {
    let sentence = match reason {
        "the agent is in the middle of a turn" => return None,
        "the agent has no conversation to resume" => {
            "This session has no saved conversation to resume, so Ghostex could not restart the agent."
        }
        "the agent was already restarted automatically a few minutes ago" => {
            "Ghostex already restarted the agent a few minutes ago, and it is still not taking messages."
        }
        "the agent coordinates threads, and a restart would end its background work" => {
            "Ghostex did not restart the agent because it coordinates threads, and a restart would end its background work."
        }
        "the agent has background tasks a restart would end" => {
            "Ghostex did not restart the agent because a restart would end its background tasks."
        }
        "the session could not be read"
        | "the session could not be put to sleep"
        | "the session could not be started again"
        | "the restart record could not be read" => {
            "Ghostex could not restart the agent."
        }
        "The agent took the message into its input box but kept it there; it was not restarted." => {
            return None
        }
        other => return Some(other.to_string()),
    };
    Some(sentence.to_string())
}

/// A reason is shown in the chat, the sidebar and the Notifications panel, so it never carries a
/// credential or a piece of terminal output: control bytes and caret pictures of them go, words
/// that hold a secret go, and it is cut to a sentence or two. The log keeps the raw text.
pub(crate) fn displayable_reason(reason: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut hide_next = false;
    for word in reason.split_whitespace() {
        let word = strip_terminal_codes(word);
        if word.is_empty() {
            continue;
        }
        let hide = hide_next || looks_like_secret(&word);
        hide_next = names_a_secret(&word);
        words.push(if hide { "[hidden]".to_string() } else { word });
    }
    let text = words.join(" ");
    if text.chars().count() <= REASON_MAX_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(REASON_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

const REASON_MAX_CHARS: usize = 300;

/// A word that is a key by its shape: a provider's prefix, `name=value` on a secret's name, or one
/// long unbroken run of letters and digits. A false hit hides one word of a sentence.
fn looks_like_secret(word: &str) -> bool {
    let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
    const PREFIXES: [&str; 8] = [
        "sk-",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "AKIA",
    ];
    if PREFIXES.iter().any(|prefix| trimmed.starts_with(prefix)) || trimmed.starts_with("eyJ") {
        return true;
    }
    if let Some((name, value)) = trimmed.split_once('=') {
        if names_a_secret(name) && !value.is_empty() {
            return true;
        }
    }
    trimmed.chars().count() >= 32
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '/'))
        && !trimmed.contains('/')
}

/// A word that names a secret, so the word after it (`password: hunter2`) is the secret.
fn names_a_secret(word: &str) -> bool {
    let lower = word
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "password"
            | "passwd"
            | "token"
            | "secret"
            | "apikey"
            | "api_key"
            | "api-key"
            | "bearer"
            | "authorization"
    ) && (word.ends_with(':') || word.ends_with('=') || lower == "bearer")
}

/// Drops control characters, their `^[[201~` caret pictures and raw escape sequences from a word.
fn strip_terminal_codes(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars().peekable();
    while let Some(c) = chars.next() {
        let escape = c == '\u{1b}' || (c == '^' && chars.peek() == Some(&'[')) || (c == '␛');
        if escape {
            if c == '^' {
                chars.next();
            }
            if chars.peek() == Some(&'[') {
                chars.next();
                // A CSI sequence ends at its first final byte.
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        if c == '^' && chars.peek().is_some_and(|next| next.is_ascii_uppercase()) {
            chars.next();
            continue;
        }
        if !c.is_control() {
            out.push(c);
        }
    }
    out
}

/// The record a session carries, while it carries one.
pub(crate) fn session_send_failure(session: &Value) -> Option<SendFailure> {
    session
        .pointer(&format!("/runtimeSettings/{SEND_FAILURE_KEY}"))
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
}

/// What the sidebar's red dot says: the failed send, or the row held on a closed session.
pub(crate) fn session_send_failure_dot(session: &Value) -> Option<String> {
    session_send_failure(session)
        .map(|failure| failure.reason)
        .or_else(|| {
            session
                .pointer(&format!("/runtimeSettings/{HELD_ON_CLOSED_SESSION_KEY}"))
                .and_then(Value::as_str)
                .filter(|reason| !reason.is_empty())
                .map(str::to_string)
        })
}

#[derive(Clone)]
struct FailedRow {
    project_id: String,
    session_id: String,
    prompt_id: String,
    reason: String,
}

#[derive(Default)]
struct Registry {
    /// Rows that failed since the last pass (`fail_prompt` has no `AppState`).
    newly_failed: Vec<FailedRow>,
    /// The record of every session with a card, by session.
    cards: HashMap<String, SendFailure>,
    /// Rows on a closed session, with when each was first seen held there.
    held_on_closed: HashMap<(String, String), (String, Instant)>,
    /// (row, reason) pairs already in the Notifications panel: a retry that fails the same way is
    /// not news. A different reason is.
    notified: HashSet<(String, String)>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

fn woken() -> &'static Notify {
    static WOKEN: OnceLock<Notify> = OnceLock::new();
    WOKEN.get_or_init(Notify::new)
}

fn card_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

/// The card a session shows, built fresh so its words follow the record. Reading never touches
/// the database: this runs for every frame.
pub(crate) fn send_failure_card(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    registry()
        .lock()
        .ok()?
        .cards
        .get(&card_key(project_id, session_id))
        .map(send_failed_notice)
}

/// Called by the queue for every row that fails; the pass below turns it into the card.
pub(crate) fn report_failed_row(project_id: &str, session_id: &str, prompt_id: &str, reason: &str) {
    if let Ok(mut registry) = registry().lock() {
        registry.newly_failed.push(FailedRow {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            prompt_id: prompt_id.to_string(),
            reason: reason.to_string(),
        });
    }
    woken().notify_one();
}

/// The scheduler saw this session's head row wait because the session is closed.
pub(crate) fn note_held_on_closed_session(project_id: &str, session_id: &str, prompt_id: &str) {
    if let Ok(mut registry) = registry().lock() {
        registry
            .held_on_closed
            .entry((project_id.to_string(), session_id.to_string()))
            .and_modify(|held| {
                if held.0 != prompt_id {
                    *held = (prompt_id.to_string(), Instant::now());
                }
            })
            .or_insert_with(|| (prompt_id.to_string(), Instant::now()));
    }
}

/// Every session's card, dot and notification, kept in step with its queue.
pub(crate) fn spawn_send_failure_task(
    state: std::sync::Arc<AppState>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        restore_cards(&state);
        let mut last_transcript_check = Instant::now();
        loop {
            tokio::select! {
                _ = shutdown.recv() => return,
                _ = woken().notified() => {}
                _ = tokio::time::sleep(RECONCILE_EVERY) => {}
            }
            let failed = registry()
                .lock()
                .map(|mut registry| std::mem::take(&mut registry.newly_failed))
                .unwrap_or_default();
            for row in failed {
                record_failed_row(&state, row).await;
            }
            reconcile(&state);
            settle_held_on_closed(&state);
            if last_transcript_check.elapsed() >= TRANSCRIPT_CHECK_EVERY {
                last_transcript_check = Instant::now();
                retire_rows_the_transcript_recorded(&state).await;
            }
        }
    });
}

/// Puts the cards back after a gxserver restart: the failure stays true whatever the daemon did.
fn restore_cards(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let path = format!("$.{SEND_FAILURE_KEY}");
    let Ok(mut statement) = db.prepare(
        "SELECT projectId, sessionId, json_extract(runtimeSettingsJson, ?1) FROM sessions WHERE json_extract(runtimeSettingsJson, ?1) IS NOT NULL",
    ) else {
        return;
    };
    let rows = statement.query_map([path], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    });
    let Ok(rows) = rows else { return };
    let Ok(mut registry) = registry().lock() else {
        return;
    };
    for (project_id, session_id, raw) in rows.flatten() {
        if let Ok(failure) = serde_json::from_str::<SendFailure>(&raw) {
            registry
                .notified
                .insert((failure.prompt_id.clone(), failure.reason.clone()));
            registry
                .cards
                .insert(card_key(&project_id, &session_id), failure);
        }
    }
}

async fn record_failed_row(state: &AppState, row: FailedRow) {
    let session = {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(Some(session)) = repository.get_session(&row.project_id, &row.session_id) else {
            return;
        };
        let queued = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
            &db,
            &row.project_id,
            &row.session_id,
        );
        let Some(prompt) = queued
            .queue
            .iter()
            .find(|prompt| prompt.id == row.prompt_id)
        else {
            return;
        };
        // An agent's message that could not be delivered goes back to the agent that sent it
        // (session_chat_queue_undelivered.rs); its card on this chat is enough for the user.
        if prompt.state != "failed"
            || crate::session_chat_queue_undelivered::agent_message_reply_to(&prompt.text).is_some()
        {
            return;
        }
        session
    };
    let zmx_name = session
        .get("zmxName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let running = crate::session_chat_send::session_agent_process_running(
        &zmx_name,
        &crate::resume_lookup::home_dir(),
    )
    .await;
    let next = if running {
        SendFailureNext::RetryOrFix
    } else if crate::session_chat_queue_runtime::can_start_on_its_conversation(state, &session)
        .await
    {
        SendFailureNext::Fix
    } else {
        SendFailureNext::StartFresh
    };
    let failure = SendFailure {
        prompt_id: row.prompt_id.clone(),
        reason: displayable_reason(&row.reason),
        at: crate::domain::now_iso(),
        next,
    };
    let stalled = failure
        .reason
        .starts_with(crate::session_chat_send::SESSION_CHAT_BUSY_AGENT_STALLED);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionChatSendFailedForGood",
        &row.project_id,
        &row.session_id,
        &row.reason,
        &[],
    );
    store_failure(state, &row.project_id, &row.session_id, &failure);
    if stalled || !running {
        settle_stale_working(state, &row.project_id, &row.session_id, &session);
    }
    notify_once(state, &row.project_id, &row.session_id, &session, &failure);
    publish(state, &row.project_id, &row.session_id);
}

fn store_failure(state: &AppState, project_id: &str, session_id: &str, failure: &SendFailure) {
    let Ok(raw) = serde_json::to_string(failure) else {
        return;
    };
    if let Ok(mut registry) = registry().lock() {
        registry
            .cards
            .insert(card_key(project_id, session_id), failure.clone());
    }
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        SEND_FAILURE_KEY,
        Some(&raw),
    );
}

/// The spinner of a turn whose hooks went quiet for good. No hook reports an agent that hung or
/// died mid-turn, so this is the Escape rule chat Stop uses (`record_session_chat_interrupt`): the
/// next hook the agent fires says it is working again, so a turn that was alive after all comes
/// back on its own. Only for a stalled or dead agent: a send that failed beside a working agent
/// says nothing about that turn.
fn settle_stale_working(state: &AppState, project_id: &str, session_id: &str, session: &Value) {
    if !crate::session_chat_follower::session_chat_hook_working(session) {
        return;
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("event".to_string(), json!("escape"));
    let _ = crate::server::dispatch_agent_http_blocking(
        state,
        "/api/updateAgentActivity".to_string(),
        uuid::Uuid::new_v4().to_string(),
        params,
    );
}

fn notify_once(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
    failure: &SendFailure,
) {
    let fresh = registry().lock().is_ok_and(|mut registry| {
        if registry.notified.len() >= NOTIFIED_REMEMBERED {
            registry.notified.clear();
        }
        registry
            .notified
            .insert((failure.prompt_id.clone(), failure.reason.clone()))
    });
    if !fresh {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let subtitle = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let title = session
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Session")
        .to_string();
    let inserted = insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            kind: NOTIFICATION_FEED_KIND_SEND_FAILED,
            title,
            subtitle,
            body: bounded_notification_body(&format!("Message not sent. {}", failure.reason)),
            agent_name: notification_agent_icon(session),
            attention_event_id: None,
        },
    );
    if inserted.is_ok() {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
}

fn publish(state: &AppState, project_id: &str, session_id: &str) {
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
    crate::session_chat_options::session_chat_terminal_notice_publisher(
        state, project_id, session_id,
    )();
}

/// The user dealt with it, or it stopped being true: the card, the dot and the bell's row go.
pub(crate) fn clear_send_failure(state: &AppState, project_id: &str, session_id: &str) {
    let had = registry()
        .lock()
        .ok()
        .and_then(|mut registry| registry.cards.remove(&card_key(project_id, session_id)));
    if had.is_none() {
        return;
    }
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        SEND_FAILURE_KEY,
        None,
    );
    if let Ok(db) = open_gxserver_database(&state.paths) {
        if mark_session_send_failure_read(&db, session_id).is_ok_and(|changed| changed > 0) {
            crate::notification_feed::broadcast_notification_feed_changed(state);
        }
    }
    publish(state, project_id, session_id);
}

/// A row that stopped being failed (Retry, Remove, sent from another client) takes its card along.
fn reconcile(state: &AppState) {
    let cards: Vec<(String, SendFailure)> = registry()
        .lock()
        .map(|registry| {
            registry
                .cards
                .iter()
                .map(|(key, failure)| (key.clone(), failure.clone()))
                .collect()
        })
        .unwrap_or_default();
    if cards.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    for (key, failure) in cards {
        let Some((project_id, session_id)) = key.split_once('|') else {
            continue;
        };
        let snapshot = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
            &db, project_id, session_id,
        );
        let still_failed = snapshot
            .queue
            .iter()
            .any(|prompt| prompt.id == failure.prompt_id && prompt.state == "failed");
        if !still_failed {
            clear_send_failure(state, project_id, session_id);
        }
    }
}

fn settle_held_on_closed(state: &AppState) {
    let held: Vec<((String, String), String, bool)> = registry()
        .lock()
        .map(|registry| {
            registry
                .held_on_closed
                .iter()
                .map(|(key, (prompt_id, since))| {
                    (
                        key.clone(),
                        prompt_id.clone(),
                        since.elapsed() >= HELD_ON_CLOSED_DOT_AFTER,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if held.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    for ((project_id, session_id), prompt_id, due) in held {
        let lifecycle = repository
            .get_session(&project_id, &session_id)
            .ok()
            .flatten()
            .and_then(|session| {
                session
                    .get("lifecycleState")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        let snapshot = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
            &db,
            &project_id,
            &session_id,
        );
        let waiting = lifecycle.as_deref().is_some_and(|state| state != "running")
            && snapshot
                .queue
                .iter()
                .any(|prompt| prompt.id == prompt_id && prompt.state == "queued");
        let publisher = SessionChatCompactingPublisher::new(state);
        if !waiting {
            if let Ok(mut registry) = registry().lock() {
                registry
                    .held_on_closed
                    .remove(&(project_id.clone(), session_id.clone()));
            }
            publisher.publish_marker(&project_id, &session_id, HELD_ON_CLOSED_SESSION_KEY, None);
        } else if due {
            publisher.publish_marker(
                &project_id,
                &session_id,
                HELD_ON_CLOSED_SESSION_KEY,
                Some(HELD_ON_CLOSED_SESSION_REASON),
            );
        }
    }
}

/// CDXC:SessionChat 2026-10-31 WHY:
/// A turn that sat on a message for five minutes can still take it: the paste waits in the terminal, and the agent may submit it when it catches up. The transcript is the only proof either way, so a failed row it records is removed, which ends the card, the dot and the notification. Something that recovers by itself is not the user's to clear.
async fn retire_rows_the_transcript_recorded(state: &AppState) {
    let cards: Vec<(String, SendFailure)> = registry()
        .lock()
        .map(|registry| {
            registry
                .cards
                .iter()
                .map(|(key, failure)| (key.clone(), failure.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (key, failure) in cards {
        let Some((project_id, session_id)) = key.split_once('|') else {
            continue;
        };
        let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
        let check_state = state.clone();
        let check_failure = failure.clone();
        let (check_project, check_session) = (project_id.clone(), session_id.clone());
        let recorded = tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&check_state.paths).ok()?;
            let snapshot = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
                &db,
                &check_project,
                &check_session,
            );
            let prompt = snapshot
                .queue
                .iter()
                .find(|prompt| prompt.id == check_failure.prompt_id)?;
            let since = chrono::DateTime::parse_from_rfc3339(&prompt.created_at)
                .ok()?
                .timestamp_millis();
            let needles = crate::coordinators::delivery_needles(&prompt.text);
            let session = DomainRepository::new(&db, check_state.metadata.server_id.as_str())
                .get_session(&check_project, &check_session)
                .ok()??;
            crate::coordinators::transcript_records_message(&session, &needles, since, &mut None)
        })
        .await
        .ok()
        .flatten()
            == Some(true);
        if !recorded {
            continue;
        }
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("promptId".to_string(), json!(failure.prompt_id));
        let _ = crate::session_chat_queue::handle_session_chat_queue_endpoint(
            &state.paths,
            state.metadata.server_id.as_str(),
            "/api/removeSessionChatQueuedPrompt",
            &params,
        );
        crate::session_chat_send_diagnostics::record_send_recovery(
            state,
            "sessionChatSendFailureRecorded",
            &project_id,
            &session_id,
            "The agent's transcript recorded a message that had failed; its card was retired.",
            &[],
        );
        clear_send_failure(state, &project_id, &session_id);
    }
}

/// A button on the card (`answerSessionChatPrompt`'s `restartAgent` lane, which every client
/// already sends and which gxserver tells apart by the action id). `None` for any other action.
pub(crate) async fn answer_card_action(
    state: &AppState,
    params: &Map<String, Value>,
) -> Option<Result<(), DomainStateError>> {
    let action = params.get("actionId").and_then(Value::as_str)?;
    if ![ACTION_RETRY, ACTION_FIX, ACTION_START_FRESH, ACTION_DISMISS].contains(&action) {
        return None;
    }
    let target = match crate::session_chat_send::resolve_session_chat_send_target(
        state,
        params,
        "sendFailure",
    ) {
        Ok(target) => target,
        Err(error) => return Some(Err(error)),
    };
    let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
    let failure = registry().lock().ok().and_then(|registry| {
        registry
            .cards
            .get(&card_key(&project_id, &session_id))
            .cloned()
    });
    // A card another client already answered has nothing left to do.
    let Some(failure) = failure else {
        return Some(Ok(()));
    };
    if action == ACTION_DISMISS {
        clear_send_failure(state, &project_id, &session_id);
        return Some(Ok(()));
    }
    if action == ACTION_START_FRESH {
        forget_conversation(state, &project_id, &session_id);
    }
    if action != ACTION_RETRY {
        if let Err(reason) =
            crate::session_chat_queue_runtime::cycle_session(state, &project_id, &session_id).await
        {
            return Some(Err(DomainStateError {
                code: "invalidState",
                message: format!("Ghostex could not restart this session: {reason}."),
            }));
        }
    }
    Some(retry_row(
        state,
        &project_id,
        &session_id,
        &failure.prompt_id,
    ))
}

fn retry_row(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("promptId".to_string(), json!(prompt_id));
    params.insert("retry".to_string(), json!(true));
    crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/updateSessionChatQueuedPrompt",
        &params,
    )?;
    clear_send_failure(state, project_id, session_id);
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
    crate::session_chat_send_wake::wake_for_retried_message(
        state, project_id, session_id, prompt_id,
    );
    crate::session_chat_queue_runtime::kick_session_chat_queue();
    Ok(())
}

/// Start fresh: a wake with no saved identity launches the agent on a new conversation.
fn forget_conversation(state: &AppState, project_id: &str, session_id: &str) {
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let _ = db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.agentSessionId', '$.agentSessionPath') WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![project_id, session_id],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(next: SendFailureNext) -> SendFailure {
        SendFailure {
            prompt_id: "Q1".to_string(),
            reason: "The agent stayed in the middle of a turn for five minutes without taking new input, so the message was not sent.".to_string(),
            at: "2026-10-31T00:00:00.000Z".to_string(),
            next,
        }
    }

    fn action_ids(next: SendFailureNext) -> Vec<String> {
        send_failed_notice(&failure(next))
            .actions
            .iter()
            .map(|action| action.id.clone())
            .collect()
    }

    #[test]
    fn a_running_agent_is_offered_retry_terminal_and_fix() {
        assert_eq!(
            action_ids(SendFailureNext::RetryOrFix),
            ["retrySend", "switchToTerminal", "fixSend"]
        );
    }

    #[test]
    fn a_dead_agent_gets_no_retry_since_it_would_do_the_same_restart() {
        assert_eq!(
            action_ids(SendFailureNext::Fix),
            ["fixSend", "switchToTerminal"]
        );
        assert_eq!(
            action_ids(SendFailureNext::StartFresh),
            ["startFreshSend", "switchToTerminal"]
        );
    }

    #[test]
    fn the_card_carries_the_logged_reason_and_keeps_its_time() {
        let notice = send_failed_notice(&failure(SendFailureNext::RetryOrFix));
        assert_eq!(notice.title, "Message not sent");
        assert!(notice
            .detail
            .as_deref()
            .unwrap()
            .starts_with("The agent stayed in the middle of a turn for five minutes"));
        assert_eq!(notice.detected_at, "2026-10-31T00:00:00.000Z");
        assert!(notice.to_value().pointer("/sendFailure/promptId").is_some());
    }

    #[test]
    fn example_four_names_the_real_cause() {
        let reason = send_failure_reason(
            "The terminal draft could not be cleared and verified. Your chat draft has been kept.",
            Some("the agent has no conversation to resume"),
        );
        assert_eq!(
            reason,
            "The agent's input box had text in it that Ghostex couldn't clear. This session has no saved conversation to resume, so Ghostex could not restart the agent."
        );
    }

    #[test]
    fn a_stall_stays_in_the_logged_words() {
        let stalled = crate::session_chat_send::SESSION_CHAT_BUSY_AGENT_STALLED;
        assert_eq!(
            send_failure_reason(stalled, Some("the agent is in the middle of a turn")),
            stalled
        );
    }

    #[test]
    fn a_reason_never_shows_a_credential_or_terminal_codes() {
        let shown = displayable_reason(
            "The terminal shows: Reply to: S90:P3lv0:G1c0r^[[201~ / \u{1b}[31mred\u{1b}[0m key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789 end",
        );
        assert!(!shown.contains("[201~"), "{shown}");
        assert!(!shown.contains('\u{1b}'), "{shown}");
        assert!(!shown.contains("sk-ant"), "{shown}");
        assert!(shown.contains("[hidden]"), "{shown}");
        assert!(shown.contains("red"), "{shown}");
    }

    #[test]
    fn a_long_reason_is_cut_to_a_sentence_or_two() {
        let shown = displayable_reason(&"word ".repeat(400));
        assert!(shown.chars().count() <= REASON_MAX_CHARS);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn the_record_survives_its_round_trip_through_the_session() {
        let raw = serde_json::to_string(&failure(SendFailureNext::Fix)).unwrap();
        let session = json!({ "runtimeSettings": { SEND_FAILURE_KEY: raw } });
        assert_eq!(
            session_send_failure(&session),
            Some(failure(SendFailureNext::Fix))
        );
        assert_eq!(
            session_send_failure_dot(&session).as_deref(),
            Some(failure(SendFailureNext::Fix).reason.as_str())
        );
    }

    #[test]
    fn a_row_held_on_a_closed_session_gets_the_dot_and_no_card() {
        let session = json!({ "runtimeSettings": { HELD_ON_CLOSED_SESSION_KEY: HELD_ON_CLOSED_SESSION_REASON } });
        assert_eq!(session_send_failure(&session), None);
        assert_eq!(
            session_send_failure_dot(&session).as_deref(),
            Some(HELD_ON_CLOSED_SESSION_REASON)
        );
    }
}
