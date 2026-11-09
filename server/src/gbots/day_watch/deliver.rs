//! Where a day-watch run's result goes: Chief of Staff's chat, which gives it to the user, and the
//! notification and phone push when Chief of Staff's reply is ready.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q-E A): the daily brief lands in Chief of Staff's chat with a notification and a phone push.
//! CDXC:GBots 2026-11-09 WHY:
//! A run of a routine that delivers to the brief (`deliver: brief`) hands its whole final message to Chief of Staff, which adds what it knows and writes the brief as its reply; a run with nothing (`no_findings`) hands nothing. Chief of Staff's finished turns are quiet unless its handover needs the user (CDXC:Notifications 2026-11-08 in packages/gx-core/src/alert_rule.rs), so the reply that carries the brief or a heads-up counts as one that needs the user: the turn that answers a hand-off within three hours and opens with "Daily brief" or "Heads-up" alerts, the same way on the computer, the bell and the phone. With no Chief of Staff chat open, the run's own chat gets the notification.
//! SEE-ALSO: server/src/presentation/handover_asks.rs (asks `brief_turn_alerts`), server/src/gbots/routine_runs.rs (calls `deliver`).

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::super::record::read_bot;
use super::super::routine_runs::SettledRoutineRun;
use super::text::{chief_brief_message, chief_heads_up_message};
use crate::domain::DomainRepository;
use crate::server::{queue_session_chat_prompt, AppState};

/// How long after a hand-off Chief of Staff's reply still counts as the brief.
const REPLY_WINDOW: Duration = Duration::from_secs(3 * 60 * 60);
/// The most of a run's message handed on.
const HANDED_MAX_CHARS: usize = 8_000;
const MAX_CACHED_EVENTS: usize = 256;

#[derive(Default)]
struct Handoffs {
    /// Chief of Staff's session id → when a brief or heads-up was last handed to it.
    handed: HashMap<String, Instant>,
    /// `session \u{1f} event` → whether that finished turn is the brief's reply.
    answers: HashMap<String, bool>,
    order: VecDeque<String>,
}

fn handoffs() -> &'static Mutex<Handoffs> {
    static HANDOFFS: OnceLock<Mutex<Handoffs>> = OnceLock::new();
    HANDOFFS.get_or_init(Mutex::default)
}

/// Whether `message` opens with the brief's or a heads-up's first line.
fn opens_as_brief(message: &str) -> bool {
    let Some(first) = message.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return false;
    };
    let words: String = first
        .chars()
        .skip_while(|character| !character.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    words.starts_with("daily brief")
        || words.starts_with("heads-up")
        || words.starts_with("heads up")
}

/// Whether the finished turn behind attention event `event_id` of `session` is Chief of Staff's
/// reply to a brief or heads-up (module comment). Cheap for every other session: one map lookup.
pub(crate) fn brief_turn_alerts(session: &Value, event_id: &str) -> bool {
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let key = format!("{session_id}\u{1f}{event_id}");
    {
        let Ok(handoffs) = handoffs().lock() else {
            return false;
        };
        if let Some(answer) = handoffs.answers.get(&key) {
            return *answer;
        }
        let recent = handoffs
            .handed
            .get(session_id)
            .is_some_and(|at| at.elapsed() < REPLY_WINDOW);
        if !recent {
            return false;
        }
    }
    // Not written yet: asked again on the next projection.
    let Some(reply) = crate::notification_feed::body::newest_reply_markdown(session) else {
        return false;
    };
    let answer = opens_as_brief(&reply);
    if let Ok(mut handoffs) = handoffs().lock() {
        if answer {
            // One reply per hand-off.
            handoffs.handed.remove(session_id);
        }
        if handoffs.answers.insert(key.clone(), answer).is_none() {
            handoffs.order.push_back(key);
        }
        while handoffs.order.len() > MAX_CACHED_EVENTS {
            if let Some(stale) = handoffs.order.pop_front() {
                handoffs.answers.remove(&stale);
            }
        }
    }
    answer
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// What the run wrote before its `AUTOMATION_RESULT:` line.
fn before_result_line(message: &str) -> String {
    let mut kept = Vec::new();
    for line in message.lines() {
        if line.trim_start().starts_with("AUTOMATION_RESULT:") {
            break;
        }
        kept.push(line);
    }
    let text = kept.join("\n").trim().to_string();
    match text.char_indices().nth(HANDED_MAX_CHARS) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text,
    }
}

/// Chief of Staff's chat, while it is open (running or asleep).
fn chief_chat(state: &AppState, repository: &DomainRepository<'_>) -> Option<Value> {
    let (project, session) = crate::chief::chief_key(&state.paths)?;
    repository
        .get_session(&project, &session)
        .ok()
        .flatten()
        .filter(|session| {
            matches!(
                crate::presentation::session_attributes::effective_lifecycle_state(session)
                    .as_str(),
                "running" | "sleep" | "sleeping"
            )
        })
}

/// Hands a run that found something (or needs the user) to Chief of Staff. BLOCKING.
pub(crate) fn deliver(state: &AppState, db: &Connection, run: &SettledRoutineRun<'_>) {
    if !matches!(run.status, "findings" | "needs_attention") {
        return;
    }
    let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
    let run_chat = run.session.and_then(|session_id| {
        let project_id: String = db
            .query_row(
                "SELECT projectId FROM sessions WHERE sessionId = ?1",
                [session_id],
                |row| row.get(0),
            )
            .ok()?;
        repository
            .get_session(&project_id, session_id)
            .ok()
            .flatten()
    });
    let written = run_chat
        .as_ref()
        .and_then(crate::notification_feed::body::newest_reply_markdown)
        .map(|message| before_result_line(&message))
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| run.summary.trim().to_string());
    if written.is_empty() {
        return;
    }
    let bot_name = read_bot(db, run.bot_id)
        .ok()
        .flatten()
        .map(|bot| bot.name)
        .unwrap_or_else(|| super::text::BOT_NAME.to_string());
    let chat_ref = run_chat
        .as_ref()
        .map(|chat| {
            crate::ids::create_global_session_ref(
                &state.metadata.server_id,
                &text(chat, "projectId"),
                &text(chat, "sessionId"),
            )
        })
        .unwrap_or_else(|| "gone".to_string());
    let is_brief = super::store::read(db)
        .ok()
        .and_then(|record| record.brief_routine_id)
        .is_some_and(|id| id == run.automation_id);
    let message = if is_brief {
        let date = chrono::Local::now().format("%a %-d %b").to_string();
        chief_brief_message(&bot_name, &date, &written, &chat_ref)
    } else {
        chief_heads_up_message(&bot_name, &written, &chat_ref)
    };
    let Some(chief) = chief_chat(state, &repository) else {
        // No Chief of Staff to tell: the user hears on the run's chat.
        if let Some(chat) = &run_chat {
            let mut params = Map::new();
            params.insert("projectId".into(), json!(text(chat, "projectId")));
            params.insert("sessionId".into(), json!(text(chat, "sessionId")));
            params.insert(
                "title".into(),
                json!(if is_brief { "Daily brief" } else { "Heads-up" }),
            );
            params.insert("body".into(), json!(written));
            let _ = crate::notification_feed::create_notification_endpoint(
                state,
                &repository,
                db,
                &params,
            );
        }
        return;
    };
    let (chief_project, chief_session) = (text(&chief, "projectId"), text(&chief, "sessionId"));
    if queue_session_chat_prompt(state, &chief_project, &chief_session, &message, false).is_ok() {
        if let Ok(mut handoffs) = handoffs().lock() {
            handoffs
                .handed
                .insert(chief_session.clone(), Instant::now());
        }
        super::super::assign::wake_if_asleep(state, &chief);
    }
}
