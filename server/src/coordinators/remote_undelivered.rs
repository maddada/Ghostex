//! Messages a coordinator here sent to its thread on another computer that the other computer's
//! chat queue marked Not sent: the coordinator gets the same "did not reach it" report a thread on
//! this computer sends.
//!
//! CDXC:Coordinators 2026-11-08 WHY:
//! GitHub #502: `start-thread --computer` answered "pending: its screen shows Trust this folder?. Ghostex keeps watching and reports to you if it never arrives", but nothing watched a thread on another computer. The local watch (`watch_pending_message`, endpoint.rs) keeps the pending message on the local thread record, and the other computer's queue tells a sender whose message it marks Not sent only when the sender is one of its own sessions (`notify_sender_of_undelivered_agent_message`), so a coordinator on another computer was never told. The coordinator's computer already reads its remote threads every few seconds (server/coordinator_remote_runtime.rs), so it also reads an idle thread's queue there and reports each failed row its coordinator sent, once per row. Only rows the queue itself gave up on are reported: a held row (a trust question, a login, an agent still starting) is still going to be typed.
//! SEE-ALSO: server/src/session_chat_queue_undelivered.rs (the same notice for a sender on the thread's own computer), server/src/coordinators/brief.rs (`ThreadReport::Undelivered`).

use serde_json::Value;

/// How often an idle thread's queue on the other computer is read.
pub const REMOTE_UNDELIVERED_CHECK_EVERY_MS: i64 = 15_000;

/// A row the other computer's queue marked Not sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteUndelivered {
    pub row_id: String,
    pub excerpt: String,
    pub reason: String,
}

/// The failed rows of a `readSessionChatQueue` answer that `coordinator_ref` sent (their sender
/// block's `Reply to`), oldest first.
pub fn remote_undelivered_messages(queue: &Value, coordinator_ref: &str) -> Vec<RemoteUndelivered> {
    let text = |row: &Value, key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    queue
        .get("queue")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|row| row.get("state").and_then(Value::as_str) == Some("failed"))
        .filter_map(|row| {
            let message = text(row, "text");
            let (reply_to, body) =
                crate::session_chat_queue_undelivered::agent_message_reply_to(&message)?;
            (reply_to == coordinator_ref).then(|| RemoteUndelivered {
                row_id: text(row, "id"),
                excerpt: super::pending_message_excerpt(body),
                reason: text(row, "errorMessage"),
            })
        })
        .filter(|row| !row.row_id.is_empty())
        .collect()
}
