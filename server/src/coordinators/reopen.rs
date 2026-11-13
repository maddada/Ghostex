//! A message to a done thread reopens it, so the turn it starts is reported to its coordinator.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! Approved plan (coordinator report delivery): a message sent to a done thread in its chat reopens it when it comes from the thread's own coordinator or from the user's own doors (the desktop app, a paired phone, and the token file, which the user's terminal and the phone's SSH sends use). Every other sender (another agent's session, another computer, a Collab guest, an extension, gxserver's own sends) leaves it done. The token file cannot be told apart from an agent running outside any Ghostex session (security/capability.rs `LocalAgent`), so such an agent reopens a done thread too; that limitation is accepted.
//!
//! CDXC:Coordinators 2026-11-10 WHY:
//! The reopen used to live only in `ghostex agents send` (a second call after the send), so a message typed in the thread's chat, sent by `send-session-chat-message`, or whose second call failed woke the thread but left it done, and its report was skipped until `coordinator reopen`. gxserver's `/api/sendSessionChatMessage` route now owns it, inside the send's ledger attempt, so a replayed send answers with the same outcome. Only chat messages count: `send-text`, `send-key` and typing in the terminal are keystrokes, not messages, and never undo a Mark Done; queued prompts and threads on another computer are not covered. Every path compares the thread's latest Mark Done (`lastResolvedAt`, migration 0075) with the original send's time, so a Mark Done given after the message was sent always wins over a late completion, a replay or the CLI's confirmation. A reopened thread gets the delivery watch, which is what reports a reply too fast for a supervisor tick to see.
//! SEE-ALSO: server/src/session_chat_send/send_reopen.rs (the route's attempt), server/src/coordinators/endpoint.rs (`reopenOnly`, the CLI's link call and confirmation), server/src/ghostex_cli/agents/delivery.rs.

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, Transaction, TransactionBehavior};

use super::endpoint::{held_reply, set_parked};
use super::records::{
    read_thread, set_thread_pending_message, set_thread_resolved, SessionKey, ThreadRecord,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::security::Principal;

/// A done thread a send may reopen, worked out before the message is typed.
#[derive(Clone, Debug)]
pub struct ReopenPlan {
    pub thread_key: SessionKey,
    /// When the original send began (ms); a Mark Done after it wins.
    pub send_ms: i64,
    /// The reply the thread finished while it was done, read before the message started a turn.
    pub held_report: Option<String>,
}

/// Whether `caller` may reopen `thread` by messaging it.
pub fn caller_may_reopen(caller: &Principal, thread: &ThreadRecord) -> bool {
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => {
            *project_id == thread.coordinator_project_id
                && *session_id == thread.coordinator_session_id
        }
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => true,
        Principal::Peer { .. }
        | Principal::Guest { .. }
        | Principal::Extension { .. }
        | Principal::Anonymous => false,
    }
}

fn iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

/// Resolved, and marked done no later than the send began.
fn reopenable(thread: &ThreadRecord, send_ms: i64) -> bool {
    thread.is_resolved()
        && thread
            .close_stamp()
            .and_then(iso_ms)
            .is_some_and(|closed| closed <= send_ms)
}

/// BLOCKING. The plan for a send to `thread_key` that began at `send_ms`, when the target is a
/// done thread `may_reopen` accepts and no Mark Done came after the send began.
pub fn prepare_reopen(
    db: &Connection,
    repository: &DomainRepository<'_>,
    thread_key: &SessionKey,
    send_ms: i64,
    may_reopen: impl Fn(&ThreadRecord) -> bool,
) -> Result<Option<ReopenPlan>, DomainStateError> {
    let Some(thread) = read_thread(db, &thread_key.0, &thread_key.1)? else {
        return Ok(None);
    };
    if !reopenable(&thread, send_ms) || !may_reopen(&thread) {
        return Ok(None);
    }
    Ok(Some(ReopenPlan {
        thread_key: thread_key.clone(),
        send_ms,
        held_report: held_reply(repository, &thread),
    }))
}

/// BLOCKING. Reopens the planned thread unless it was reopened, or marked done again, since the
/// plan: checked again in one writer transaction, so a Mark Done landing meanwhile wins. With
/// `watch` (`(message, its sendRequestId, sent_at_ms)`), the message gets the delivery watch, the
/// one that reports a reply too fast for a supervisor tick to see. Returns whether it reopened the
/// thread.
pub fn apply_reopen(
    db: &Connection,
    repository: &DomainRepository<'_>,
    plan: &ReopenPlan,
    watch: Option<(&str, Option<&str>, i64)>,
) -> Result<bool, DomainStateError> {
    let own_transaction = db.is_autocommit();
    let transaction = if own_transaction {
        Some(
            Transaction::new_unchecked(db, TransactionBehavior::Immediate)
                .map_err(super::records::sql_error)?,
        )
    } else {
        None
    };
    let (project_id, session_id) = (&plan.thread_key.0, &plan.thread_key.1);
    let reopened = match read_thread(db, project_id, session_id)? {
        Some(thread) if reopenable(&thread, plan.send_ms) => {
            set_thread_resolved(db, project_id, session_id, false)?;
            set_parked(repository, &plan.thread_key, false)?;
            if let Some((message, send_id, sent_ms)) = watch {
                let mut excerpt = super::delivery::pending_message_excerpt(message);
                // A message with no text (images only) is watched too, found in the transcript
                // by time (CDXC:Coordinators 2026-11-13 in delivery.rs).
                if super::delivery::delivery_needles(&excerpt).is_empty() {
                    excerpt = super::delivery::IMAGES_ONLY_PENDING_MESSAGE.to_string();
                }
                let sent_at = chrono::DateTime::from_timestamp_millis(sent_ms)
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339_opts(SecondsFormat::Millis, true);
                set_thread_pending_message(
                    db, project_id, session_id, &excerpt, &sent_at, send_id,
                )?;
            }
            true
        }
        _ => false,
    };
    if let Some(transaction) = transaction {
        transaction.commit().map_err(super::records::sql_error)?;
    }
    Ok(reopened)
}
