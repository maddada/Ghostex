/*
CDXC:SessionChatPromptQueue 2026-08-21:
gxserver owns a per-session queue of prompts the user wrote but does not want
delivered yet, plus the unsent composer draft synced between devices. The wire
contract is `packages/shared/session-chat-queue.ts` and this file is its Rust mirror:
field names, omission semantics, and reorder rules must match it verbatim.

Ownership split:
  - THIS file owns storage, the seven endpoints, and the frame/read carriage.
  - `session_chat_queue_runtime.rs` owns the scheduler that releases one row per
    idle window. It reuses `deliver_session_chat_queued_prompt` here so both the
    user's "Send now" and the scheduler travel the same claim → send → settle
    path and cannot double-claim a row.
  - `server.rs` stays dispatch only: it supplies a `SessionChatQueueSender`
    (the internal /api/sendSessionChatMessage path, so the per-session send
    mutex is inherited) and a publisher that broadcasts the state frame.

NAMING COLLISION, READ THIS TWICE: `SessionChatMessage.queued` is a DIFFERENT
thing — it means the agent CLI's OWN internal queue (Claude Code's
`queue-operation` rows) is holding a prompt the user already sent with Enter.
Nothing here ever touches that flag.
*/

use std::{future::Future, pin::Pin, sync::Arc};

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    domain::{DomainRepository, DomainStateError},
    paths::GxserverPaths,
    storage::open_gxserver_database,
};

/// A queue this long is a runaway client, not a user. Bounded so one session
/// can never grow the state database without limit.
pub const SESSION_CHAT_QUEUE_MAX_PROMPTS: usize = 200;

pub const SESSION_CHAT_QUEUE_STATE_QUEUED: &str = "queued";
pub const SESSION_CHAT_QUEUE_STATE_SENDING: &str = "sending";
pub const SESSION_CHAT_QUEUE_STATE_FAILED: &str = "failed";

/// Reason stamped on a row that was mid-delivery when the daemon went down.
/// Never silently re-sent: the agent may already have received it.
pub const SESSION_CHAT_QUEUE_RESTART_REASON: &str =
    "gxserver restarted while this prompt was being delivered.";

/// Mirrors `SessionChatQueuedPrompt` in `packages/shared/session-chat-queue.ts`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChatQueuedPrompt {
    pub id: String,
    pub text: String,
    /// `queued` | `sending` | `failed`.
    pub state: String,
    /// Set only when `state == "failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Mirrors `SessionChatDraft` in `packages/shared/session-chat-queue.ts`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChatDraft {
    pub content: String,
    pub updated_at: String,
    pub origin_client_id: String,
}

/*
CDXC:SessionChatQueueCarriage 2026-08-21:
`queue` and `draft` ride readSessionChat results and the snapshot / replaced /
state frames, never `appended`. Their omission semantics differ:

  queue — PRESENT (even empty) is the daemon capability probe, so this daemon
    always emits it. A client that never sees it hides every queue control.
  draft — OMITTED means "unchanged / none on the server", NOT cleared. A draft
    is cleared by writing an explicit empty `content`, which stores a row with
    an empty string and is therefore still emitted.
*/
#[derive(Clone, Debug, Default)]
pub struct SessionChatQueueSnapshot {
    pub queue: Vec<SessionChatQueuedPrompt>,
    pub draft: Option<SessionChatDraft>,
}

impl SessionChatQueueSnapshot {
    /// Writes `queue` (always) and `draft` (only when stored) onto a frame or
    /// read result.
    pub fn insert_into(&self, target: &mut Map<String, Value>) {
        target.insert(
            "queue".to_string(),
            serde_json::to_value(&self.queue).unwrap_or_else(|_| Value::Array(Vec::new())),
        );
        if let Some(draft) = self.draft.as_ref() {
            if let Ok(value) = serde_json::to_value(draft) {
                target.insert("draft".to_string(), value);
            }
        }
    }

    /*
    The long-poll fingerprint contribution. Ghostex mobile has no /api/events
    socket: it synthesizes frames from long-polled readSessionChat results, so
    a queue or draft change that does not move the fingerprint is a change the
    phone NEVER sees. Fold every field a client renders — id, order, state,
    text and error all reach the row strip.
    */
    pub fn revision(&self) -> String {
        let mut revision = String::with_capacity(32 + self.queue.len() * 48);
        for prompt in &self.queue {
            revision.push_str(&prompt.id);
            revision.push(':');
            revision.push_str(&prompt.state);
            revision.push(':');
            revision.push_str(&prompt.updated_at);
            revision.push(':');
            revision.push_str(prompt.error_message.as_deref().unwrap_or(""));
            revision.push('|');
        }
        revision.push('#');
        if let Some(draft) = self.draft.as_ref() {
            revision.push_str(&draft.updated_at);
            revision.push(':');
            revision.push_str(&draft.origin_client_id);
        }
        revision
    }

    /*
    CDXC:SessionChatPromptQueue 2026-08-21-b:
    The badge count is EVERY row, `failed` included. Excluding failed rows made
    the one state that needs the user act invisible everywhere outside the chat
    view: a queue stalled behind a failed head published no count at all, so a
    dead queue looked exactly like no queue. `failed_count` rides alongside so a
    client can colour the same badge red without a second field per row, and so
    anything that means "work is still coming" (Auto Sleep) can subtract it
    instead of guessing.
    */
    pub fn queued_count(&self) -> usize {
        self.queue.len()
    }

    pub fn failed_count(&self) -> usize {
        self.queue
            .iter()
            .filter(|prompt| prompt.state == SESSION_CHAT_QUEUE_STATE_FAILED)
            .count()
    }

    /// The row the scheduler would deliver next, or `None` when the queue is
    /// empty or blocked behind a `failed`/`sending` head.
    pub fn deliverable_head(&self) -> Option<&SessionChatQueuedPrompt> {
        self.queue
            .first()
            .filter(|prompt| prompt.state == SESSION_CHAT_QUEUE_STATE_QUEUED)
    }
}

/*
Delivery injection. server.rs owns the only implementation: it routes into the
same internals `/api/sendSessionChatMessage` uses, so a queued prompt inherits
the per-session send mutex and cannot interleave with a Delayed Send or a
message the user typed. The queue module (and the scheduler) therefore never
learns about AppState, zmx names, or the send watchdog.
*/
pub type SessionChatQueueSender =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync>;

/// Builds a sender for one `(projectId, sessionId)`. The scheduler holds the
/// factory because it delivers for whichever session becomes ready.
pub type SessionChatQueueSenderFactory =
    Arc<dyn Fn(&str, &str) -> SessionChatQueueSender + Send + Sync>;

/// Broadcasts a `sessionChatState` frame carrying the session's current queue
/// and draft. server.rs owns the only implementation.
pub type SessionChatQueuePublisher = Arc<dyn Fn() + Send + Sync>;

pub type SessionChatQueuePublisherFactory =
    Arc<dyn Fn(&str, &str) -> SessionChatQueuePublisher + Send + Sync>;

/// What one endpoint call produced. `broadcast` is false only for pure reads,
/// so every mutation reaches the session's other clients.
pub struct SessionChatQueueEndpointResult {
    pub value: Value,
    pub project_id: String,
    pub session_id: String,
    pub broadcast: bool,
}

/// Outcome of one delivery attempt, shared by "Send now" and the scheduler.
pub struct SessionChatQueueDelivery {
    /// True ⇒ the prompt reached the agent and its row is gone. False ⇒ the
    /// send failed and the row is now `failed` with `error_message`.
    pub sent: bool,
    pub error_message: Option<String>,
    pub snapshot: SessionChatQueueSnapshot,
}

pub fn is_session_chat_queue_endpoint(endpoint_path: &str) -> bool {
    matches!(
        endpoint_path,
        "/api/readSessionChatQueue"
            | "/api/queueSessionChatPrompt"
            | "/api/updateSessionChatQueuedPrompt"
            | "/api/removeSessionChatQueuedPrompt"
            | "/api/reorderSessionChatQueue"
            | "/api/setSessionChatDraft"
    )
}

/// Every queue endpoint except `/api/sendSessionChatQueuedPrompt`, which is
/// async because it delivers (see `deliver_session_chat_queued_prompt`).
pub fn handle_session_chat_queue_endpoint(
    paths: &GxserverPaths,
    server_id: &str,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Result<SessionChatQueueEndpointResult, DomainStateError> {
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    let (project_id, session_id) = require_target(&db, server_id, params)?;
    let (value, broadcast) = match endpoint_path {
        "/api/readSessionChatQueue" => {
            let snapshot = read_snapshot(&db, &project_id, &session_id)?;
            (snapshot_value(&snapshot), false)
        }
        "/api/queueSessionChatPrompt" => {
            let text = required_text(params, "text")?;
            let prompt = append_prompt(&db, &project_id, &session_id, &text)?;
            let snapshot = read_snapshot(&db, &project_id, &session_id)?;
            let mut value = snapshot_value(&snapshot);
            insert_prompt(&mut value, &prompt);
            (value, true)
        }
        "/api/updateSessionChatQueuedPrompt" => {
            let prompt_id = required_text(params, "promptId")?;
            let text = optional_text(params, "text");
            let retry = params.get("retry").and_then(Value::as_bool) == Some(true);
            update_prompt(&db, &project_id, &session_id, &prompt_id, text, retry)?;
            let snapshot = read_snapshot(&db, &project_id, &session_id)?;
            (snapshot_value(&snapshot), true)
        }
        "/api/removeSessionChatQueuedPrompt" => {
            let prompt_id = required_text(params, "promptId")?;
            let prompt = remove_prompt(&db, &project_id, &session_id, &prompt_id)?;
            let snapshot = read_snapshot(&db, &project_id, &session_id)?;
            let mut value = snapshot_value(&snapshot);
            insert_prompt(&mut value, &prompt);
            (value, true)
        }
        "/api/reorderSessionChatQueue" => {
            let prompt_ids = params
                .get("promptIds")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "reorderSessionChatQueue requires promptIds as an array of row ids.",
                    )
                })?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>();
            reorder_queue(&db, &project_id, &session_id, &prompt_ids)?;
            let snapshot = read_snapshot(&db, &project_id, &session_id)?;
            (snapshot_value(&snapshot), true)
        }
        "/api/setSessionChatDraft" => {
            // An empty `content` is how a draft is CLEARED, so it is valid
            // input and must not be rejected as a missing parameter.
            let content = params
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    DomainStateError::bad_request("setSessionChatDraft requires content.")
                })?
                .to_string();
            let client_id = required_text(params, "clientId")?;
            let draft = set_draft(&db, &project_id, &session_id, &content, &client_id)?;
            (
                json!({
                    "draft": serde_json::to_value(&draft).unwrap_or(Value::Null),
                }),
                true,
            )
        }
        _ => {
            return Err(DomainStateError::not_found(format!(
                "{endpoint_path} is not a gxserver session-chat queue endpoint."
            )))
        }
    };
    Ok(SessionChatQueueEndpointResult {
        value,
        project_id,
        session_id,
        broadcast,
    })
}

/*
The one delivery path. Both `/api/sendSessionChatQueuedPrompt` ("Send now",
which delivers regardless of agent state exactly like pressing Enter) and the
scheduler call this, so a row can only ever be claimed once: the claim is a
guarded UPDATE from `queued`/`failed` to `sending`, and a row already `sending`
is refused rather than sent twice.
*/
pub async fn deliver_session_chat_queued_prompt(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    send: &SessionChatQueueSender,
) -> Result<SessionChatQueueDelivery, DomainStateError> {
    let text = {
        let db = open_gxserver_database(paths).map_err(internal_error)?;
        require_session(&db, server_id, project_id, session_id)?;
        claim_prompt(&db, project_id, session_id, prompt_id)?
    };
    let outcome = send(text).await;
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    let error_message = match outcome {
        Ok(()) => {
            db.execute(
                r#"
                DELETE FROM session_chat_queued_prompts
                WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
                "#,
                params![prompt_id, project_id, session_id],
            )
            .map_err(sql_error)?;
            None
        }
        Err(message) => {
            let message = bounded_reason(&message);
            fail_prompt(&db, project_id, session_id, prompt_id, &message)?;
            Some(message)
        }
    };
    Ok(SessionChatQueueDelivery {
        sent: error_message.is_none(),
        error_message,
        snapshot: read_snapshot(&db, project_id, session_id)?,
    })
}

/// Marks the head row `failed` without attempting a send: used when the
/// session is in a blocking terminal state (login expired, trust prompt, agent
/// exited), where delivering would silently lose the text.
pub fn fail_session_chat_queued_prompt(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    reason: &str,
) -> Result<SessionChatQueueSnapshot, DomainStateError> {
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    fail_prompt(
        &db,
        project_id,
        session_id,
        prompt_id,
        &bounded_reason(reason),
    )?;
    read_snapshot(&db, project_id, session_id)
}

/// Read helper for the frame/read carriage and the scheduler. A daemon that
/// cannot read its own state database still reports a present (empty) queue:
/// `queue` present is the capability probe, and this daemon has the capability.
pub fn read_session_chat_queue_snapshot(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> SessionChatQueueSnapshot {
    let Ok(db) = open_gxserver_database(paths) else {
        return SessionChatQueueSnapshot::default();
    };
    read_snapshot(&db, project_id, session_id).unwrap_or_default()
}

/// Same read against a connection the caller already holds (the long-poll
/// fingerprint runs every 500ms per poller and must not open a second one).
pub fn read_session_chat_queue_snapshot_with(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> SessionChatQueueSnapshot {
    read_snapshot(db, project_id, session_id).unwrap_or_default()
}

/// Every session that currently holds at least one non-failed row, so the
/// scheduler never walks transcripts for sessions with an empty queue and the
/// Auto Sleep decline is one query.
pub fn list_sessions_with_pending_queue(
    paths: &GxserverPaths,
) -> Result<Vec<(String, String)>, DomainStateError> {
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    let mut statement = db
        .prepare(
            r#"
            SELECT DISTINCT projectId, sessionId
            FROM session_chat_queued_prompts
            WHERE state <> 'failed'
            ORDER BY projectId, sessionId
            "#,
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

/// True when the session holds a row that has not failed. The Auto Sleep
/// decline reads this: an automatic sleep must not retire a session whose
/// queue still has work, while an explicit user Sleep is untouched.
pub fn session_has_pending_session_chat_queue(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> bool {
    db.query_row(
        r#"
        SELECT COUNT(*) FROM session_chat_queued_prompts
        WHERE projectId = ?1 AND sessionId = ?2 AND state <> 'failed'
        "#,
        params![project_id, session_id],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/*
Restart recovery. A row left in `sending` is ambiguous — the bytes may have
reached the agent — so it becomes `failed` with an explicit reason and waits
for the user to retry or delete it. Re-sending it automatically is exactly the
mistake `delayed_sends::recover_after_restart` avoids.
*/
pub fn recover_session_chat_queue_after_restart(
    paths: &GxserverPaths,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    db.execute(
        r#"
        UPDATE session_chat_queued_prompts
        SET state = 'failed', errorMessage = ?1, updatedAt = ?2
        WHERE state = 'sending'
        "#,
        params![SESSION_CHAT_QUEUE_RESTART_REASON, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

fn read_snapshot(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<SessionChatQueueSnapshot, DomainStateError> {
    let mut statement = db
        .prepare(
            r#"
            SELECT promptId, text, state, errorMessage, createdAt, updatedAt
            FROM session_chat_queued_prompts
            WHERE projectId = ?1 AND sessionId = ?2
            ORDER BY position, createdAt, promptId
            "#,
        )
        .map_err(sql_error)?;
    let queue = statement
        .query_map(params![project_id, session_id], |row| {
            Ok(SessionChatQueuedPrompt {
                id: row.get(0)?,
                text: row.get(1)?,
                state: row.get(2)?,
                error_message: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let draft = db
        .query_row(
            r#"
            SELECT content, originClientId, updatedAt
            FROM session_chat_drafts
            WHERE projectId = ?1 AND sessionId = ?2
            "#,
            params![project_id, session_id],
            |row| {
                Ok(SessionChatDraft {
                    content: row.get(0)?,
                    origin_client_id: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)?;
    Ok(SessionChatQueueSnapshot { queue, draft })
}

fn append_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<SessionChatQueuedPrompt, DomainStateError> {
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM session_chat_queued_prompts WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if count as usize >= SESSION_CHAT_QUEUE_MAX_PROMPTS {
        return Err(DomainStateError::bad_request(format!(
            "This session already holds {SESSION_CHAT_QUEUE_MAX_PROMPTS} queued prompts."
        )));
    }
    let next_position: i64 = db
        .query_row(
            r#"
            SELECT COALESCE(MAX(position), -1) + 1
            FROM session_chat_queued_prompts
            WHERE projectId = ?1 AND sessionId = ?2
            "#,
            params![project_id, session_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let prompt = SessionChatQueuedPrompt {
        id: create_prompt_id(),
        text: text.to_string(),
        state: SESSION_CHAT_QUEUE_STATE_QUEUED.to_string(),
        error_message: None,
        created_at: now_iso(),
        updated_at: now_iso(),
    };
    db.execute(
        r#"
        INSERT INTO session_chat_queued_prompts (
          promptId, projectId, sessionId, position, text,
          state, errorMessage, createdAt, updatedAt
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'queued', NULL, ?6, ?6)
        "#,
        params![
            prompt.id,
            project_id,
            session_id,
            next_position,
            prompt.text,
            prompt.created_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(prompt)
}

fn update_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    text: Option<String>,
    retry: bool,
) -> Result<(), DomainStateError> {
    let state = read_prompt_state(db, project_id, session_id, prompt_id)?;
    if state == SESSION_CHAT_QUEUE_STATE_SENDING {
        return Err(DomainStateError::bad_request(
            "This prompt is being delivered right now.",
        ));
    }
    if text.is_none() && !retry {
        return Err(DomainStateError::bad_request(
            "updateSessionChatQueuedPrompt requires text or retry.",
        ));
    }
    if let Some(text) = text.as_deref() {
        validate_text(text)?;
    }
    // `retry` is ignored for rows that are not failed, per the wire contract.
    let clear_failure = retry && state == SESSION_CHAT_QUEUE_STATE_FAILED;
    db.execute(
        r#"
        UPDATE session_chat_queued_prompts
        SET text = COALESCE(?4, text),
            state = CASE WHEN ?5 THEN 'queued' ELSE state END,
            errorMessage = CASE WHEN ?5 THEN NULL ELSE errorMessage END,
            updatedAt = ?6
        WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
        "#,
        params![
            prompt_id,
            project_id,
            session_id,
            text,
            clear_failure,
            now_iso(),
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn remove_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> Result<SessionChatQueuedPrompt, DomainStateError> {
    let prompt = read_prompt(db, project_id, session_id, prompt_id)?;
    if prompt.state == SESSION_CHAT_QUEUE_STATE_SENDING {
        return Err(DomainStateError::bad_request(
            "This prompt is being delivered right now.",
        ));
    }
    db.execute(
        r#"
        DELETE FROM session_chat_queued_prompts
        WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
        "#,
        params![prompt_id, project_id, session_id],
    )
    .map_err(sql_error)?;
    Ok(prompt)
}

/*
Reorder contract: ids the server does not know are IGNORED, and rows the caller
omitted keep their relative order AFTER the listed ones. That is what stops a
stale client from dropping a row queued from another device mid-drag — the
missing row is appended rather than deleted.
*/
fn reorder_queue(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_ids: &[String],
) -> Result<(), DomainStateError> {
    let existing = read_snapshot(db, project_id, session_id)?.queue;
    let mut ordered: Vec<String> = Vec::with_capacity(existing.len());
    for prompt_id in prompt_ids {
        if ordered.iter().any(|id| id == prompt_id) {
            continue;
        }
        if existing.iter().any(|prompt| &prompt.id == prompt_id) {
            ordered.push(prompt_id.clone());
        }
    }
    for prompt in &existing {
        if !ordered.iter().any(|id| id == &prompt.id) {
            ordered.push(prompt.id.clone());
        }
    }
    let timestamp = now_iso();
    for (position, prompt_id) in ordered.iter().enumerate() {
        db.execute(
            r#"
            UPDATE session_chat_queued_prompts
            SET position = ?4, updatedAt = ?5
            WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
            "#,
            params![
                prompt_id,
                project_id,
                session_id,
                position as i64,
                timestamp,
            ],
        )
        .map_err(sql_error)?;
    }
    Ok(())
}

fn set_draft(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    content: &str,
    client_id: &str,
) -> Result<SessionChatDraft, DomainStateError> {
    if content.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "setSessionChatDraft content exceeds the {}-byte limit.",
            crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES
        )));
    }
    let draft = SessionChatDraft {
        content: content.to_string(),
        origin_client_id: client_id.to_string(),
        updated_at: now_iso(),
    };
    db.execute(
        r#"
        INSERT INTO session_chat_drafts (projectId, sessionId, content, originClientId, updatedAt)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(projectId, sessionId) DO UPDATE SET
          content = excluded.content,
          originClientId = excluded.originClientId,
          updatedAt = excluded.updatedAt
        "#,
        params![
            project_id,
            session_id,
            draft.content,
            draft.origin_client_id,
            draft.updated_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(draft)
}

/*
CDXC:DraftSessions 2026-08-28:
The synced composer draft on its own, without the queue rows `read_snapshot`
carries. Draft sessions read it for the sidebar display title, published on
every presentation delta, which must never pay for the queue. A blank draft
answers `Ok(None)`, exactly like a missing row — a draft cleared to empty text
and a draft that never existed are the same state everywhere in the feature.

A FAILED read is deliberately NOT folded into `None`: "nobody typed anything"
is a claim about the user's unsent text, and a transient SQLite error is not
evidence for it. Callers that are only decorating a projection may discard the
error.
*/
pub fn read_session_chat_draft_content(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, DomainStateError> {
    let content = db
        .query_row(
            "SELECT content FROM session_chat_drafts WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(content.filter(|content| !content.trim().is_empty()))
}

/// Every non-blank synced draft, keyed by `(projectId, sessionId)`. One grouped
/// read of a table that holds at most one row per session with unsent text —
/// never one query per session, because presentation snapshots publish many
/// times a second on a busy sidebar.
pub fn read_non_blank_session_chat_draft_contents(
    db: &Connection,
) -> std::collections::HashMap<(String, String), String> {
    let Ok(mut statement) = db.prepare(
        "SELECT projectId, sessionId, content FROM session_chat_drafts WHERE TRIM(content) <> ''",
    ) else {
        return std::collections::HashMap::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
            row.get::<_, String>(2)?,
        ))
    }) else {
        return std::collections::HashMap::new();
    };
    rows.filter_map(Result::ok)
        .filter(|(_, content)| !content.trim().is_empty())
        .collect()
}

/*
CDXC:DraftCrashSafety 2026-08-28:
The `/api/listSessionChatDrafts` read behind the client-side draft-cache
reconcile. The composer's per-keystroke localStorage cache is not durable — a
kill that skips a clean Chromium shutdown drops uncommitted batches — so this
table is the copy that survives, and clients heal their cache from it at boot.
One bounded list (at most one row per session with unsent text), stamped so
the client can refuse anything older than what it still holds.
*/
pub fn list_session_chat_drafts_value(db: &Connection) -> Result<Value, DomainStateError> {
    let mut statement = db
        .prepare(
            r#"
            SELECT projectId, sessionId, content, updatedAt
            FROM session_chat_drafts
            WHERE TRIM(content) <> ''
            ORDER BY updatedAt DESC
            "#,
        )
        .map_err(sql_error)?;
    let drafts = statement
        .query_map([], |row| {
            Ok(json!({
                "projectId": row.get::<_, String>(0)?,
                "sessionId": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "updatedAt": row.get::<_, String>(3)?,
            }))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(json!({ "drafts": drafts }))
}

/// Text an armed Delayed Send must deliver instead of a bare Enter: the
/// session's synced chat composer draft, when one is non-empty. Chat clients
/// hand the terminal composer's text off into this draft, so an Enter fired
/// into the pty would land on an empty input line and silently drop the
/// message the user staged.
pub fn armed_delayed_send_draft_text(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, DomainStateError> {
    Ok(read_snapshot(db, project_id, session_id)?
        .draft
        .map(|draft| draft.content)
        .filter(|content| !content.trim().is_empty()))
}

/// Clears the synced composer draft after a Delayed Send delivered it. The
/// server origin id makes every client apply the empty draft instead of
/// treating the frame as an echo of its own edit.
pub fn clear_session_chat_draft_after_delivery(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    set_draft(db, project_id, session_id, "", "gxserver-delayed-send").map(|_| ())
}

/// Guarded claim: only a `queued` or `failed` row can move to `sending`, so two
/// scheduler ticks (or a tick racing a "Send now") cannot both deliver it.
fn claim_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> Result<String, DomainStateError> {
    let prompt = read_prompt(db, project_id, session_id, prompt_id)?;
    if prompt.state == SESSION_CHAT_QUEUE_STATE_SENDING {
        return Err(DomainStateError::bad_request(
            "This prompt is being delivered right now.",
        ));
    }
    let claimed = db
        .execute(
            r#"
            UPDATE session_chat_queued_prompts
            SET state = 'sending', errorMessage = NULL, updatedAt = ?4
            WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
              AND state IN ('queued', 'failed')
            "#,
            params![prompt_id, project_id, session_id, now_iso()],
        )
        .map_err(sql_error)?
        > 0;
    if !claimed {
        return Err(DomainStateError::bad_request(
            "This prompt is being delivered right now.",
        ));
    }
    Ok(prompt.text)
}

fn fail_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    reason: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE session_chat_queued_prompts
        SET state = 'failed', errorMessage = ?4, updatedAt = ?5
        WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
        "#,
        params![prompt_id, project_id, session_id, reason, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn read_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> Result<SessionChatQueuedPrompt, DomainStateError> {
    db.query_row(
        r#"
        SELECT promptId, text, state, errorMessage, createdAt, updatedAt
        FROM session_chat_queued_prompts
        WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
        "#,
        params![prompt_id, project_id, session_id],
        |row| {
            Ok(SessionChatQueuedPrompt {
                id: row.get(0)?,
                text: row.get(1)?,
                state: row.get(2)?,
                error_message: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(sql_error)?
    .ok_or_else(|| DomainStateError::not_found("That queued prompt no longer exists."))
}

fn read_prompt_state(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> Result<String, DomainStateError> {
    Ok(read_prompt(db, project_id, session_id, prompt_id)?.state)
}

// ---------------------------------------------------------------------------
// Params + helpers
// ---------------------------------------------------------------------------

fn snapshot_value(snapshot: &SessionChatQueueSnapshot) -> Value {
    let mut value = Map::new();
    snapshot.insert_into(&mut value);
    Value::Object(value)
}

fn insert_prompt(value: &mut Value, prompt: &SessionChatQueuedPrompt) {
    if let (Some(object), Ok(prompt)) = (value.as_object_mut(), serde_json::to_value(prompt)) {
        object.insert("prompt".to_string(), prompt);
    }
}

fn require_target(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<(String, String), DomainStateError> {
    let project_id = required_text(params, "projectId")?;
    let session_id = required_text(params, "sessionId")?;
    require_session(db, server_id, &project_id, &session_id)?;
    Ok((project_id, session_id))
}

fn require_session(
    db: &Connection,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::not_found("The session no longer exists."))?;
    Ok(())
}

fn required_text(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    let value = params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))?;
    if key == "text" {
        validate_text(&value)?;
    }
    Ok(value)
}

fn optional_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A row that could never be delivered is worse than a rejected queue click:
/// hold queued text to the same limit the send path enforces.
fn validate_text(text: &str) -> Result<(), DomainStateError> {
    if text.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "A queued prompt exceeds the {}-byte zmx send limit.",
            crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES
        )));
    }
    Ok(())
}

fn bounded_reason(reason: &str) -> String {
    let reason = reason.trim();
    if reason.is_empty() {
        return "The prompt could not be delivered.".to_string();
    }
    reason.chars().take(300).collect()
}

fn create_prompt_id() -> String {
    format!("Q{}", Uuid::new_v4().simple())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn internal_error(error: anyhow::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite session-chat queue state error: {error}"),
    }
}
