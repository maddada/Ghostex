/*
CDXC:SessionChat 2026-08-21:
gxserver owns a per-session queue of prompts the user wrote but does not want
delivered yet, plus the unsent composer draft synced between devices. The wire
contract is `packages/shared/session-chat-queue.ts` and this file is its Rust mirror:
field names, omission semantics, and reorder rules must match it verbatim.

Ownership split:
  - THIS file owns storage, the seven endpoints, and the frame/read carriage.
  - `session_chat_queue_runtime/scheduler.rs` owns the scheduler that releases one row per
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
    pub startup_send: bool,
    /// The queue's own copies of the pictures the message carries (session_chat_queue_attachments.rs).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub image_paths: Vec<String>,
    /// `queued` | `sending` | `failed`.
    pub state: String,
    /// Set only when `state == "failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Why a waiting row has not gone out yet, in the user's words; only on a row that has waited
    /// about a minute (`set_queue_wait_reason`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The waiting reasons the scheduler shows, by prompt id. In memory only: a reason is a reading
/// of the live session, and the scheduler states it again within a minute of a restart.
static QUEUE_WAIT_REASONS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::LazyLock::new(Default::default);

/// Sets or clears the waiting reason shown on `prompt_id`; answers whether it changed, so the
/// caller republishes the queue only then.
pub(crate) fn set_queue_wait_reason(prompt_id: &str, reason: Option<&str>) -> bool {
    let Ok(mut reasons) = QUEUE_WAIT_REASONS.lock() else {
        return false;
    };
    match reason {
        Some(reason) if reasons.get(prompt_id).map(String::as_str) != Some(reason) => {
            reasons.insert(prompt_id.to_string(), reason.to_string());
            true
        }
        None => reasons.remove(prompt_id).is_some(),
        _ => false,
    }
}

/// Forgets the reasons of rows that are no longer waiting.
pub(crate) fn retain_queue_wait_reasons(keep: impl Fn(&str) -> bool) {
    if let Ok(mut reasons) = QUEUE_WAIT_REASONS.lock() {
        reasons.retain(|prompt_id, _| keep(prompt_id));
    }
}

fn queue_wait_reason(prompt_id: &str) -> Option<String> {
    QUEUE_WAIT_REASONS
        .lock()
        .ok()
        .and_then(|reasons| reasons.get(prompt_id).cloned())
}

/// Mirrors `SessionChatDraft` in `packages/shared/session-chat-queue.ts`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChatDraft {
    pub parked: bool,
    pub content: String,
    pub updated_at: String,
    pub origin_client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<crate::session_chat_draft_versions::DraftVersion>,
    pub consumed_drafts: Vec<crate::session_chat_draft_versions::DraftVersion>,
    pub delivered_drafts: Vec<crate::session_chat_delivered_drafts::DeliveredDraft>,
}

/*
CDXC:SessionChat 2026-08-21:
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
    pub account_switch: Option<Value>,
    /// The retry after a connection or server error (accounts/api_errors.rs `chat_state`).
    pub api_retry: Option<Value>,
    pub async_questions_since: Option<i64>,
    /// When the session was forked from another (`session_chat_open_questions::read_forked_at`).
    pub forked_at: Option<i64>,
    pub retired_async_question_ids: Vec<String>,
    /// The question forms and pages put off "For later" (`session_chat_open_questions::read_later_ids`).
    pub later_question_ids: Vec<String>,
    /// The question forms and pages the user removed without answering (`session_chat_open_questions::read_removed_ids`).
    pub removed_question_ids: Vec<String>,
    /// The session's resource index (`session_chat_resources::frame_index`), set only where a
    /// frame or read result is built (`with_resource_index`): the list is large and the snapshot
    /// is read in many places that never send it.
    pub resource_index: std::sync::Arc<Vec<Value>>,
    /// The index table's write counter when this was read, so the long poll's fingerprint moves
    /// when any index changes.
    pub resource_epoch: u64,
    pub pending_model_selection: Option<crate::session_chat_model_selection::PendingModelSelection>,
}

impl SessionChatQueueSnapshot {
    /// Writes `queue` (always) and `draft` (only when stored) onto a frame or
    /// read result.
    pub fn insert_into(&self, target: &mut Map<String, Value>) {
        // CDXC:SessionChat 2026-09-17 SEE-ALSO:
        // session_chat_async_questions.rs owns question expiry; carry its resume boundary on every snapshot and state frame, including a first subscription without a read request.
        target.insert(
            "retiredAsyncQuestionIds".to_string(),
            json!(self.retired_async_question_ids),
        );
        target.insert(
            "asyncQuestionsSince".to_string(),
            json!(self.async_questions_since),
        );
        target.insert("forkedAt".to_string(), json!(self.forked_at));
        target.insert(
            "laterQuestionIds".to_string(),
            json!(self.later_question_ids),
        );
        target.insert(
            "removedQuestionIds".to_string(),
            json!(self.removed_question_ids),
        );
        if !self.resource_index.is_empty() {
            target.insert("resourceIndex".to_string(), json!(*self.resource_index));
        }
        target.insert(
            "accountSwitch".to_string(),
            self.account_switch.clone().unwrap_or(Value::Null),
        );
        target.insert(
            "apiRetry".to_string(),
            self.api_retry.clone().unwrap_or(Value::Null),
        );
        target.insert(
            "pendingModelSelection".to_string(),
            serde_json::to_value(&self.pending_model_selection).unwrap_or(Value::Null),
        );
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
        let mut revision = serde_json::to_string(&self.pending_model_selection).unwrap_or_default();
        revision.push_str(&serde_json::to_string(&self.account_switch).unwrap_or_default());
        revision.push_str(&serde_json::to_string(&self.api_retry).unwrap_or_default());
        revision.push_str(&serde_json::to_string(&self.async_questions_since).unwrap_or_default());
        revision.push_str(&serde_json::to_string(&self.forked_at).unwrap_or_default());
        revision
            .push_str(&serde_json::to_string(&self.retired_async_question_ids).unwrap_or_default());
        revision.push_str(&serde_json::to_string(&self.later_question_ids).unwrap_or_default());
        revision.push_str(&serde_json::to_string(&self.removed_question_ids).unwrap_or_default());
        revision.push_str(&self.resource_epoch.to_string());
        for prompt in &self.queue {
            revision.push_str(&prompt.id);
            revision.push(':');
            revision.push_str(&prompt.state);
            revision.push_str(if prompt.startup_send {
                ":startup"
            } else {
                ":queue"
            });
            revision.push(':');
            revision.push_str(&prompt.updated_at);
            revision.push(':');
            revision.push_str(prompt.error_message.as_deref().unwrap_or(""));
            revision.push(':');
            revision.push_str(prompt.wait_reason.as_deref().unwrap_or(""));
            revision.push('|');
        }
        revision.push('#');
        if let Some(draft) = self.draft.as_ref() {
            revision.push_str(&draft.updated_at);
            revision.push(':');
            revision.push_str(&draft.origin_client_id);
            revision.push_str(&serde_json::to_string(&draft.version).unwrap_or_default());
            revision.push_str(&serde_json::to_string(&draft.consumed_drafts).unwrap_or_default());
            revision.push_str(&serde_json::to_string(&draft.delivered_drafts).unwrap_or_default());
        }
        revision
    }

    /*
    CDXC:SessionChat 2026-08-21-b:
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
    ///
    /// CDXC:SessionChat 2026-10-12 WHY: a startup send marked Not sent (a message the chat already shows as sent, which waited too long for its session to wake or whose session cannot be woken) is passed over instead of blocking the rows behind it; otherwise every later message to that session waited behind it until someone pressed Remove. Retry puts it back in its old place, ahead of them. A failed row queued for the next stop still blocks the queue.
    pub fn deliverable_head(&self) -> Option<&SessionChatQueuedPrompt> {
        self.queue
            .iter()
            .find(|prompt| {
                !(prompt.startup_send && prompt.state == SESSION_CHAT_QUEUE_STATE_FAILED)
            })
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
/// Takes the row's text and its pictures.
pub type SessionChatQueueSender = Arc<
    dyn Fn(
            String,
            Vec<String>,
        ) -> Pin<Box<dyn Future<Output = Result<(), DomainStateError>> + Send>>
        + Send
        + Sync,
>;

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
    /// send failed and the row is now `failed` with `error_message`, unless
    /// `held` put it back to `queued`.
    pub sent: bool,
    /// The input box was missing and the caller allowed a hold: the row is
    /// `queued` again, untouched, for the scheduler's next attempt.
    pub held: bool,
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
            | "/api/acknowledgeSessionChatDraftHandoff"
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
        "/api/acknowledgeSessionChatDraftHandoff" => {
            let id = required_text(params, "handoffId")?;
            crate::session_chat_draft_recovery::acknowledge(&db, &project_id, &session_id, &id)?;
            (json!({ "acknowledged": true }), false)
        }
        "/api/readSessionChatQueue" => {
            let snapshot = read_snapshot(&db, &project_id, &session_id)?;
            (snapshot_value(&snapshot), false)
        }
        "/api/queueSessionChatPrompt" => {
            let transaction =
                rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)
                    .map_err(sql_error)?;
            let images = crate::session_chat_queue_attachments::requested(params);
            // A message of pictures alone has no text.
            let text = if images.is_empty() {
                required_text(params, "text")?
            } else {
                optional_text(params, "text").unwrap_or_default()
            };
            validate_text(&text)?;
            let version = crate::session_chat_draft_versions::parse(params)?;
            if let Some(version) = version.as_ref() {
                crate::session_chat_draft_versions::require_saved(
                    &transaction,
                    &project_id,
                    &session_id,
                    &text,
                    version,
                )?;
            }
            let draft_before_queue = read_snapshot(&transaction, &project_id, &session_id)?.draft;
            let startup_send = params.get("startupSend").and_then(Value::as_bool) == Some(true);
            // The prompt a box session's launch already handed to its agent is answered, not typed again.
            if startup_send
                && crate::agentbox::claim_launch_prompt_echo(
                    &DomainRepository::new(&transaction, server_id),
                    &project_id,
                    &session_id,
                    &text,
                )?
            {
                let prompt = SessionChatQueuedPrompt {
                    id: create_prompt_id(),
                    text: text.clone(),
                    startup_send: true,
                    image_paths: Vec::new(),
                    state: SESSION_CHAT_QUEUE_STATE_QUEUED.to_string(),
                    error_message: None,
                    wait_reason: None,
                    created_at: now_iso(),
                    updated_at: now_iso(),
                };
                let snapshot = read_snapshot(&transaction, &project_id, &session_id)?;
                let mut value = snapshot_value(&snapshot);
                insert_prompt(&mut value, &prompt);
                transaction.commit().map_err(sql_error)?;
                return Ok(SessionChatQueueEndpointResult {
                    value,
                    broadcast: false,
                    project_id,
                    session_id,
                });
            }
            let prompt = append_prompt(
                Some(paths),
                &transaction,
                &project_id,
                &session_id,
                &text,
                startup_send,
                &images,
            )?;
            if let Some(version) = version.as_ref() {
                crate::session_chat_draft_versions::consume_in(
                    &transaction,
                    &project_id,
                    &session_id,
                    version,
                )?;
            } else {
                clear_session_chat_draft_after_send(
                    &transaction,
                    &project_id,
                    &session_id,
                    &text,
                    draft_before_queue.as_ref(),
                )?;
            }
            let snapshot = read_snapshot(&transaction, &project_id, &session_id)?;
            let mut value = snapshot_value(&snapshot);
            insert_prompt(&mut value, &prompt);
            transaction.commit().map_err(sql_error)?;
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
            crate::session_chat_queue_attachments::remove(paths, &prompt_id);
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
            let logger = crate::logging::GxserverLogger::new(paths.clone());
            let before = read_snapshot(&db, &project_id, &session_id)
                .ok()
                .and_then(|snapshot| snapshot.draft);
            let describe = |draft: &SessionChatDraft| {
                json!({
                    "value": crate::session_chat_draft_diagnostics::fingerprint(&draft.content),
                    "updatedAt": draft.updated_at, "originClientId": draft.origin_client_id,
                })
            };
            crate::session_chat_draft_diagnostics::log(
                &logger,
                "serverSaveBegin",
                &project_id,
                &session_id,
                json!({
                    "clientId": client_id, "incoming": crate::session_chat_draft_diagnostics::fingerprint(&content),
                    "previous": before.as_ref().map(&describe),
                }),
            );
            let version = crate::session_chat_draft_versions::parse(params)?;
            let result = match version.as_ref() {
                Some(version) => crate::session_chat_draft_versions::save(
                    &db,
                    &project_id,
                    &session_id,
                    &client_id,
                    &content,
                    version,
                ),
                None => set_draft(&db, &project_id, &session_id, &content, &client_id),
            };
            crate::session_chat_draft_diagnostics::log(
                &logger,
                "serverSaveSettled",
                &project_id,
                &session_id,
                json!({
                    "clientId": client_id, "accepted": result.is_ok(), "saved": result.as_ref().ok().map(describe),
                }),
            );
            let draft = result?;
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
            )));
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
    hold_when_composer_missing: bool,
) -> Result<SessionChatQueueDelivery, DomainStateError> {
    let (text, images) = {
        let db = open_gxserver_database(paths).map_err(internal_error)?;
        require_session(&db, server_id, project_id, session_id)?;
        claim_prompt(&db, project_id, session_id, prompt_id)?
    };
    let outcome = deliver_once(
        paths, server_id, project_id, session_id, prompt_id, text, images, send,
    )
    .await;
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    let mut held = false;
    let error_message = match outcome {
        // A delivery that restarted the agent made this row a startup send; it waits for the new
        // input box whichever caller delivered it.
        Err(error)
            if (hold_when_composer_missing && error.code == "composerNotReady")
                || error.code == crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING =>
        {
            db.execute(
                r#"
                UPDATE session_chat_queued_prompts
                SET state = 'queued', errorMessage = NULL
                WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3 AND state = 'sending'
                "#,
                params![prompt_id, project_id, session_id],
            )
            .map_err(sql_error)?;
            held = true;
            None
        }
        Ok(()) => {
            db.execute(
                r#"
                DELETE FROM session_chat_queued_prompts
                WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
                "#,
                params![prompt_id, project_id, session_id],
            )
            .map_err(sql_error)?;
            crate::session_chat_queue_attachments::mark_delivered(paths, prompt_id);
            crate::session_chat_queue_attachments::sweep(paths, &db);
            None
        }
        Err(error) => {
            let message = bounded_reason(&error.message);
            fail_prompt(&db, project_id, session_id, prompt_id, &message)?;
            Some(message)
        }
    };
    Ok(SessionChatQueueDelivery {
        sent: error_message.is_none() && !held,
        held,
        error_message,
        snapshot: read_snapshot(&db, project_id, session_id)?,
    })
}

/// CDXC:SessionChat 2026-10-05 WHY:
/// A row a gxserver restart caught mid-delivery comes back `failed` (SESSION_CHAT_QUEUE_RESTART_REASON), and retrying it used to type it again even when the agent already had it. Each row's delivery is a send in the send ledger (session_chat_send_requests.rs) under `queuedPrompt:<promptId>`: a retry of an interrupted delivery first reads the agent's transcript and, when the prompt is there, settles the row as sent without typing. A prompt the transcript does not show is typed again, because the user retried it after being told the delivery was cut off.
async fn deliver_once(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    text: String,
    images: Vec<String>,
    send: &SessionChatQueueSender,
) -> Result<(), DomainStateError> {
    use crate::session_chat_send_requests::{
        begin, begin_again, interrupted_send_arrived, mark_sent, Begin, SendRequestKey,
    };
    const ENDPOINT: &str = "queuedPromptDelivery";
    let key = SendRequestKey::new(project_id, session_id, &format!("queuedPrompt:{prompt_id}"));
    let needles = crate::coordinators::delivery_needles(&text);
    let mut begun = begin(paths, &key, ENDPOINT, &needles)?;
    if let Begin::Interrupted { since_ms, needles } = &begun {
        if interrupted_send_arrived(paths, server_id, &key, *since_ms, needles).await {
            mark_sent(paths, &key, &json!({ "recoveredAfterRestart": true }));
            return Ok(());
        }
        begun = begin_again(paths, &key, ENDPOINT, needles)?;
    }
    match begun {
        Begin::Send(attempt) => {
            let outcome = send(text, images).await;
            attempt.settle(match &outcome {
                Ok(()) => crate::session_chat_send_requests::Outcome::Sent(json!({})),
                Err(error) if crate::session_chat_send_requests::submitted_nothing(error.code) => {
                    crate::session_chat_send_requests::Outcome::Refused
                }
                Err(_) => crate::session_chat_send_requests::Outcome::Uncertain,
            });
            outcome
        }
        Begin::Sent(_) => Ok(()),
        Begin::InFlight(_) | Begin::Interrupted { .. } => Err(DomainStateError::bad_request(
            "This prompt is being delivered right now.",
        )),
    }
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

impl SessionChatQueueSnapshot {
    /// This snapshot with the session's resource index, for the frames and read results that
    /// carry it to a chat.
    pub fn with_resource_index(
        mut self,
        db: &Connection,
        project_id: &str,
        session_id: &str,
    ) -> Self {
        self.resource_index =
            crate::session_chat_resources::frame_index(db, project_id, session_id);
        self
    }
}

/// The snapshot a frame builder sends: [`read_session_chat_queue_snapshot`] plus the index.
pub fn read_session_chat_frame_snapshot(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> SessionChatQueueSnapshot {
    let Ok(db) = open_gxserver_database(paths) else {
        return SessionChatQueueSnapshot::default();
    };
    read_snapshot(&db, project_id, session_id)
        .unwrap_or_default()
        .with_resource_index(&db, project_id, session_id)
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
            UNION SELECT projectId, sessionId FROM session_chat_model_selections
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
    if crate::session_chat_model_selection::read_pending(db, project_id, session_id)
        .is_some_and(|selection| selection.state != "failed")
    {
        return true;
    }
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
    // Model selection is idempotent: the driver reads the current footer before changing anything.
    db.execute("UPDATE session_chat_model_selections SET state = 'queued', retryAt = 0 WHERE state = 'applying'", []).map_err(sql_error)?;
    crate::session_chat_queue_attachments::sweep(paths, &db);
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
            SELECT promptId, text, state, errorMessage, createdAt, updatedAt, startupSend,
              imagePaths
            FROM session_chat_queued_prompts
            WHERE projectId = ?1 AND sessionId = ?2
            ORDER BY position, createdAt, promptId
            "#,
        )
        .map_err(sql_error)?;
    let queue = statement
        .query_map(params![project_id, session_id], |row| {
            let id: String = row.get(0)?;
            Ok(SessionChatQueuedPrompt {
                wait_reason: queue_wait_reason(&id),
                id,
                text: row.get(1)?,
                state: row.get(2)?,
                error_message: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                startup_send: row.get(6)?,
                image_paths: crate::session_chat_queue_attachments::decode(row.get(7)?),
            })
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let draft = crate::session_chat_draft_versions::read(db, project_id, session_id)?;
    Ok(SessionChatQueueSnapshot {
        retired_async_question_ids: crate::session_chat_async_questions::read_retired_question_ids(
            db, project_id, session_id,
        ),
        async_questions_since: crate::session_chat_async_questions::read_async_questions_since(
            db, project_id, session_id,
        ),
        forked_at: crate::session_chat_open_questions::read_forked_at(db, project_id, session_id),
        later_question_ids: crate::session_chat_open_questions::read_later_ids(
            db, project_id, session_id,
        ),
        removed_question_ids: crate::session_chat_open_questions::read_removed_ids(
            db, project_id, session_id,
        ),
        resource_index: Default::default(),
        resource_epoch: crate::session_chat_resources::epoch(),
        account_switch: crate::accounts::switch_progress::read(db, project_id, session_id),
        api_retry: read_api_retry(db, project_id, session_id),
        queue,
        draft,
        pending_model_selection: crate::session_chat_model_selection::read_pending(
            db, project_id, session_id,
        ),
    })
}

fn read_api_retry(db: &Connection, project_id: &str, session_id: &str) -> Option<Value> {
    let (recovery, suppressed): (Option<String>, Option<i64>) = db
        .query_row(
            "SELECT json_extract(runtimeSettingsJson, '$.accountRecovery'), json_extract(runtimeSettingsJson, '$.accountRecoverySuppressed') FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
            |row| Ok((row.get(0).ok().flatten(), row.get(1).ok().flatten())),
        )
        .optional()
        .ok()
        .flatten()?;
    let runtime = json!({
        "accountRecovery": recovery
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .unwrap_or(Value::Null),
        "accountRecoverySuppressed": suppressed == Some(1),
    });
    crate::accounts::api_errors::chat_state(&runtime)
}

fn append_prompt(
    paths: Option<&GxserverPaths>,
    db: &Connection,
    project_id: &str,
    session_id: &str,
    text: &str,
    startup_send: bool,
    images: &[String],
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
    let id = create_prompt_id();
    let image_paths = match paths {
        Some(paths) => crate::session_chat_queue_attachments::store(paths, &id, images)?,
        None => Vec::new(),
    };
    let prompt = SessionChatQueuedPrompt {
        id,
        text: text.to_string(),
        startup_send,
        image_paths,
        state: SESSION_CHAT_QUEUE_STATE_QUEUED.to_string(),
        error_message: None,
        wait_reason: None,
        created_at: now_iso(),
        updated_at: now_iso(),
    };
    db.execute(
        r#"
        INSERT INTO session_chat_queued_prompts (
          promptId, projectId, sessionId, position, text,
          state, errorMessage, createdAt, updatedAt, startupSend, imagePaths
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'queued', NULL, ?6, ?6, ?7, ?8)
        "#,
        params![
            prompt.id,
            project_id,
            session_id,
            next_position,
            prompt.text,
            prompt.created_at,
            prompt.startup_send,
            crate::session_chat_queue_attachments::encode(&prompt.image_paths),
        ],
    )
    .map_err(|error| {
        if let Some(paths) = paths {
            crate::session_chat_queue_attachments::remove(paths, &prompt.id);
        }
        sql_error(error)
    })?;
    Ok(prompt)
}

/// Appends a plain queued prompt outside the endpoint, for gxserver's own notes.
pub(crate) fn enqueue_session_chat_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<SessionChatQueuedPrompt, DomainStateError> {
    append_prompt(None, db, project_id, session_id, text, false, &[])
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

/// Deletes the queued prompts of a session deleted for good, or of every session of a removed
/// project (`session_id` is `None`), and returns how many went.
///
/// CDXC:SessionChat 2026-10-25 WHY:
/// The queue has no foreign key onto `sessions` on purpose (migration 0021): its text must outlive a session row that is pruned or re-created. A session deleted for good (`/api/removeSession`, `/api/removeProject`, deleting a worktree) is neither, yet its rows stayed behind where no client or CLI verb could reach them, and a held one could still be tried and reported to its sender for a session that no longer existed (2026-10-25: an agent's message to a test Cursor session it then deleted). Only those deliberate deletes drop the rows; closing a session keeps them, since it can be brought back, and still marks held messages Not sent (`session_chat_send_wake.rs`).
pub(crate) fn forget_session_chat_queue(
    db: &Connection,
    project_id: &str,
    session_id: Option<&str>,
) -> Result<usize, DomainStateError> {
    match session_id {
        Some(session_id) => db.execute(
            "DELETE FROM session_chat_queued_prompts WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
        ),
        None => db.execute(
            "DELETE FROM session_chat_queued_prompts WHERE projectId = ?1",
            params![project_id],
        ),
    }
    .map_err(sql_error)
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
    let mut draft = SessionChatDraft {
        parked: false,
        content: content.to_string(),
        origin_client_id: client_id.to_string(),
        updated_at: now_iso(),
        version: None,
        consumed_drafts: Vec::new(),
        delivered_drafts: crate::session_chat_delivered_drafts::read(
            db,
            Some((project_id, session_id)),
        )?,
    };
    // A new write must have a distinct version even in the same millisecond:
    // a send acknowledgement may be comparing against the previous stamp.
    draft.updated_at = db
        .query_row(
            r#"
        INSERT INTO session_chat_drafts (projectId, sessionId, content, originClientId, updatedAt)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(projectId, sessionId) DO UPDATE SET
          content = excluded.content,
          originClientId = excluded.originClientId,
          draftId = NULL,
          revision = NULL,
          updatedAt = CASE
            WHEN excluded.updatedAt > session_chat_drafts.updatedAt THEN excluded.updatedAt
            ELSE strftime('%Y-%m-%dT%H:%M:%fZ', session_chat_drafts.updatedAt, '+0.001 seconds')
          END
        RETURNING updatedAt
        "#,
            params![
                project_id,
                session_id,
                draft.content,
                draft.origin_client_id,
                draft.updated_at,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    Ok(draft)
}

/*
CDXC:Drafts 2026-08-28:
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
/// CDXC:Drafts 2026-09-11 WHY:
/// Parked drafts retain recovery text after a terminal handoff, but the chat composer is empty; including them in either presentation read leaves a false sidebar draft dot.
pub fn read_session_chat_draft_content(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, DomainStateError> {
    let content = db
        .query_row(
            "SELECT content FROM session_chat_drafts WHERE projectId = ?1 AND sessionId = ?2 AND parked = 0",
            params![project_id, session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(content.filter(|content| !content.trim().is_empty()))
}

/// Every non-blank, unparked synced draft, keyed by `(projectId, sessionId)`. One grouped
/// read of a table that holds at most one row per session with unsent text —
/// never one query per session, because presentation snapshots publish many
/// times a second on a busy sidebar.
pub fn read_non_blank_session_chat_draft_contents(
    db: &Connection,
) -> std::collections::HashMap<(String, String), String> {
    let Ok(mut statement) = db.prepare(
        "SELECT projectId, sessionId, content FROM session_chat_drafts WHERE parked = 0 AND TRIM(content) <> ''",
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

/// Every session's unsent draft (at most one row per session with unsent text) and the recovery
/// checkpoints, for the history `/api/readAppUserData` returns.
pub fn list_session_chat_drafts_value(db: &Connection) -> Result<Value, DomainStateError> {
    let mut drafts = Vec::new();
    for (project, session, draft) in crate::session_chat_draft_versions::read_all(db)? {
        let mut value = serde_json::to_value(draft)
            .map_err(|error| DomainStateError::bad_request(error.to_string()))?;
        value["projectId"] = json!(project);
        value["sessionId"] = json!(session);
        drafts.push(value);
    }
    Ok(json!({ "drafts": drafts, "recoveryDrafts": crate::session_chat_draft_recovery::read(db)? }))
}

/// Text an armed Delayed Send must deliver instead of a bare Enter: the
/// session's synced chat composer draft, when one is non-empty. Chat clients
/// hand the terminal composer's text off into this draft, so an Enter fired
/// into the pty would land on an empty input line and silently drop the
/// message the user staged.
///
/// CDXC:DelayedSend 2026-09-09 DECISION:
/// User: spaces and newlines count as empty. Whitespace-only chat drafts must not take priority over terminal input.
pub fn armed_delayed_send_draft(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<SessionChatDraft>, DomainStateError> {
    Ok(read_snapshot(db, project_id, session_id)?
        .draft
        .filter(|draft| !draft.parked && !draft.content.trim().is_empty()))
}

/// Clears the synced composer draft after a Delayed Send delivered it. The
/// server origin id makes every client apply the empty draft instead of
/// treating the frame as an echo of its own edit.
pub fn clear_session_chat_draft_after_delivery(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    submitted: &SessionChatDraft,
) -> Result<(), DomainStateError> {
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
    if let Some(version) = submitted.version.as_ref() {
        crate::session_chat_draft_versions::consume_in(
            &transaction,
            project_id,
            session_id,
            version,
        )?;
    } else {
        clear_session_chat_draft_after_send(
            &transaction,
            project_id,
            session_id,
            &submitted.content,
            Some(submitted),
        )?;
    }
    crate::session_chat_delivered_drafts::record(
        &transaction,
        project_id,
        session_id,
        &submitted.content,
    )?;
    transaction.commit().map_err(sql_error)
}

/// CDXC:Drafts 2026-09-05 WHY:
/// Legacy clients lack draft identities. Only clear the unchanged exact snapshot;
/// versioned clients retire their identity through session_chat_draft_versions.
pub fn clear_session_chat_draft_after_send(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    sent_text: &str,
    draft_before_send: Option<&SessionChatDraft>,
) -> Result<bool, DomainStateError> {
    let Some(draft) = draft_before_send else {
        return Ok(false);
    };
    if draft.version.is_some()
        || draft.content.trim().is_empty()
        || sent_text.trim() != draft.content.trim()
    {
        return Ok(false);
    }
    let changed = db.execute(
        "UPDATE session_chat_drafts SET content = '', originClientId = 'gxserver-chat-send',
           updatedAt = CASE WHEN ?6 > updatedAt THEN ?6
             ELSE strftime('%Y-%m-%dT%H:%M:%fZ', updatedAt, '+0.001 seconds') END
         WHERE projectId = ?1 AND sessionId = ?2 AND content = ?3 AND updatedAt = ?4 AND originClientId = ?5",
        params![project_id, session_id, draft.content, draft.updated_at, draft.origin_client_id, now_iso()],
    ).map_err(sql_error)?;
    Ok(changed > 0)
}

/// Guarded claim: only a `queued` or `failed` row can move to `sending`, so two
/// scheduler ticks (or a tick racing a "Send now") cannot both deliver it.
fn claim_prompt(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> Result<(String, Vec<String>), DomainStateError> {
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
    Ok((prompt.text, prompt.image_paths))
}

/// Turns the row this session is delivering into a startup send, so it waits for the input box of
/// the agent the failed delivery just restarted (session_chat_queue_runtime/send_heal.rs) instead
/// of failing. False when no row is being delivered.
pub(crate) fn hold_sending_prompt_for_restart(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> bool {
    open_gxserver_database(paths).is_ok_and(|db| {
        db.execute(
            r#"
            UPDATE session_chat_queued_prompts
            SET startupSend = 1
            WHERE projectId = ?1 AND sessionId = ?2 AND state = 'sending'
            "#,
            params![project_id, session_id],
        )
        .is_ok_and(|changed| changed > 0)
    })
}

/// Turns the queued row `prompt_id` into a startup send, so it wakes its sleeping session and
/// waits for the woken agent's input box (CDXC:SessionChat 2026-10-29 in session_chat_send_wake.rs).
/// False when the row is not waiting (gone, being delivered, or already Not sent).
pub(crate) fn hold_queued_prompt_for_wake(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> bool {
    open_gxserver_database(paths).is_ok_and(|db| {
        db.execute(
            r#"
            UPDATE session_chat_queued_prompts
            SET startupSend = 1
            WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3 AND state = 'queued'
            "#,
            params![prompt_id, project_id, session_id],
        )
        .is_ok_and(|changed| changed > 0)
    })
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
        SELECT promptId, text, state, errorMessage, createdAt, updatedAt, startupSend, imagePaths
        FROM session_chat_queued_prompts
        WHERE promptId = ?1 AND projectId = ?2 AND sessionId = ?3
        "#,
        params![prompt_id, project_id, session_id],
        |row| {
            let id: String = row.get(0)?;
            Ok(SessionChatQueuedPrompt {
                wait_reason: queue_wait_reason(&id),
                id,
                text: row.get(1)?,
                state: row.get(2)?,
                error_message: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                startup_send: row.get(6)?,
                image_paths: crate::session_chat_queue_attachments::decode(row.get(7)?),
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
