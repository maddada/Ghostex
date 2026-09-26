//! What the queue scheduler does when a queued prompt does not go through:
//! hold and retry while the agent's input box is briefly missing, and tell the
//! sending agent when a message from another agent is marked failed.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use crate::{
    domain::DomainRepository, ids::create_global_session_ref, paths::GxserverPaths,
    storage::open_gxserver_database,
};

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: a queued message must not fail because the agent's input box was off screen for a moment (a repaint, a redraw, a CLI still booting). The scheduler puts the row back and retries every few seconds; it is marked failed only when the input box stays missing for two minutes.
/// SEE-ALSO: session_chat_queue_runtime.rs already holds without an attempt when the cached screen shows no input box; this covers the fresh screen the send itself reads.
pub(crate) const SESSION_CHAT_QUEUE_COMPOSER_HOLD_MS: i64 = 120_000;
const SESSION_CHAT_QUEUE_COMPOSER_RETRY_MS: i64 = 5_000;

struct ComposerHold {
    prompt_id: String,
    since: DateTime<Utc>,
    retry_at: DateTime<Utc>,
}

/// Per-session hold clocks, keyed like the scheduler's gates. In memory only:
/// a gxserver restart simply starts the two-minute budget again.
#[derive(Clone, Default)]
pub(crate) struct SessionChatQueueComposerHolds {
    holds: Arc<Mutex<HashMap<String, ComposerHold>>>,
}

impl SessionChatQueueComposerHolds {
    /// The head row was put back a moment ago; wait out the retry spacing.
    pub(crate) fn waiting(&self, key: &str, prompt_id: &str, now: DateTime<Utc>) -> bool {
        self.holds.lock().is_ok_and(|holds| {
            holds
                .get(key)
                .is_some_and(|hold| hold.prompt_id == prompt_id && now < hold.retry_at)
        })
    }

    /// Whether a missing input box may still put this row back instead of failing it.
    pub(crate) fn may_hold(&self, key: &str, prompt_id: &str, now: DateTime<Utc>) -> bool {
        self.holds.lock().is_ok_and(|holds| match holds.get(key) {
            Some(hold) if hold.prompt_id == prompt_id => {
                now.signed_duration_since(hold.since).num_milliseconds()
                    < SESSION_CHAT_QUEUE_COMPOSER_HOLD_MS
            }
            _ => true,
        })
    }

    pub(crate) fn record(&self, key: &str, prompt_id: &str, now: DateTime<Utc>) {
        let Ok(mut holds) = self.holds.lock() else {
            return;
        };
        let since = holds
            .get(key)
            .filter(|hold| hold.prompt_id == prompt_id)
            .map_or(now, |hold| hold.since);
        holds.insert(
            key.to_string(),
            ComposerHold {
                prompt_id: prompt_id.to_string(),
                since,
                retry_at: now + Duration::milliseconds(SESSION_CHAT_QUEUE_COMPOSER_RETRY_MS),
            },
        );
    }

    pub(crate) fn clear(&self, key: &str) {
        if let Ok(mut holds) = self.holds.lock() {
            holds.remove(key);
        }
    }

    pub(crate) fn retain(&self, live: impl Fn(&str) -> bool) {
        if let Ok(mut holds) = self.holds.lock() {
            holds.retain(|key, _| live(key));
        }
    }
}

/// The `Reply to:` global ref of a `ghostex agents send` header, if `text` carries one.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs writes the header; packages/gx-chat-core/src/transcript/agent_message.rs parses it for display.
fn agent_message_reply_to(text: &str) -> Option<(&str, &str)> {
    let mut lines = text.lines();
    let opener = lines.next()?.trim();
    if opener != "Message from another agent" && opener != "MESSAGE FROM" {
        return None;
    }
    let mut reply_to = None;
    for line in lines.by_ref() {
        let Some((name, value)) = line.split_once(": ") else {
            break;
        };
        if name == "Reply to" {
            reply_to = Some(value.trim());
        }
    }
    let body_start = text.find("\n\n").map_or(text.len(), |index| index + 2);
    Some((reply_to?, text[body_start..].trim()))
}

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: when a queued message from another agent is marked failed, tell the agent that sent it. The notice goes into the sender's own queue as a plain Ghostex note, so it reaches the sender at its next stop and can never trigger a notice of its own.
/// Only unattended failures notify; a failed "Send now" is shown to the person who pressed it.
pub(crate) fn notify_sender_of_undelivered_agent_message(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    prompt_text: &str,
    reason: &str,
) -> Option<(String, String)> {
    let (reply_to, body) = agent_message_reply_to(prompt_text)?;
    let mut parts = reply_to.split(':');
    let (_, sender_project, sender_session) = (parts.next()?, parts.next()?, parts.next()?);
    if create_global_session_ref(server_id, sender_project, sender_session) != reply_to
        || (sender_project == project_id && sender_session == session_id)
    {
        return None;
    }
    let db = open_gxserver_database(paths).ok()?;
    let repository = DomainRepository::new(&db, server_id);
    repository
        .get_session(sender_project, sender_session)
        .ok()??;
    let recipient = repository.get_session(project_id, session_id).ok()??;
    let title = recipient
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("the other session");
    let recipient_ref = create_global_session_ref(server_id, project_id, session_id);
    let excerpt: String = body.chars().take(400).collect();
    let ellipsis = if excerpt.len() < body.len() {
        "…"
    } else {
        ""
    };
    let notice = format!(
        "Ghostex could not deliver your queued message to \"{title}\" ({recipient_ref}).\nReason: {reason}\nThe message is still in that session's queue, marked Not delivered, until someone retries or deletes it there. Read that session's chat before sending it again.\n\nYour message began:\n{excerpt}{ellipsis}"
    );
    crate::session_chat_queue::enqueue_session_chat_prompt(
        &db,
        sender_project,
        sender_session,
        &notice,
    )
    .ok()?;
    Some((sender_project.to_string(), sender_session.to_string()))
}
