//! What the queue scheduler does when a queued prompt does not go through:
//! hold and retry while the agent's input box is briefly missing, and tell the
//! sending agent when a message from another agent is marked failed.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
};

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value};

use crate::{
    domain::DomainRepository, ids::create_global_session_ref, paths::GxserverPaths,
    storage::open_gxserver_database,
};

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: a queued message must not fail because the agent's input box was off screen for a moment (a repaint, a redraw, a CLI still booting). The scheduler puts the row back and retries every few seconds; it is marked failed only when the input box stays missing for two minutes.
/// SEE-ALSO: session_chat_queue_runtime/scheduler.rs already holds without an attempt when the cached screen shows no input box; this covers the fresh screen the send itself reads.
pub(crate) const SESSION_CHAT_QUEUE_COMPOSER_HOLD_MS: i64 = 120_000;
const SESSION_CHAT_QUEUE_COMPOSER_RETRY_MS: i64 = 5_000;

/// How long a startup send (a message the user already sent, CDXC:SessionChat 2026-10-11 in
/// session_chat_send_wake.rs) waits between looks at the agent's input box: every 2 s while the
/// agent is most likely to come up, then less often, settling at every 15 s. It never gives up
/// while the session lives.
pub(crate) fn startup_send_retry_ms(waited_ms: i64) -> i64 {
    match waited_ms {
        ..30_000 => 2_000,
        ..120_000 => 5_000,
        ..300_000 => 10_000,
        _ => 15_000,
    }
}

/// How long the scheduler waits between checks that a session holding a startup send is awake
/// (CDXC:SessionChat 2026-10-12 in session_chat_send_wake.rs). The first check also waits, so a
/// wake a client or the send itself just started is not raced.
fn startup_send_wake_check_ms(waited_ms: i64) -> i64 {
    match waited_ms {
        ..120_000 => 10_000,
        ..600_000 => 30_000,
        _ => 60_000,
    }
}

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
    /// When the scheduler next re-reads the screen of a session whose startup send waits for its
    /// input box, so a long wait does not capture the screen every second.
    screen_reads: Arc<Mutex<HashMap<String, ComposerHold>>>,
    /// When the scheduler next checks that a session holding a startup send is awake.
    wake_checks: Arc<Mutex<HashMap<String, ComposerHold>>>,
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

    /// Whether a missing input box may still put this row back instead of failing it. A startup
    /// send is always put back.
    pub(crate) fn may_hold(
        &self,
        key: &str,
        prompt_id: &str,
        startup_send: bool,
        now: DateTime<Utc>,
    ) -> bool {
        if startup_send {
            return true;
        }
        self.holds.lock().is_ok_and(|holds| match holds.get(key) {
            Some(hold) if hold.prompt_id == prompt_id => {
                now.signed_duration_since(hold.since).num_milliseconds()
                    < SESSION_CHAT_QUEUE_COMPOSER_HOLD_MS
            }
            _ => true,
        })
    }

    /// Starts this row's two-minute budget again from `now`, keeping its retry spacing: the time
    /// it spent waiting for the user's answer to a prompt is no missing input box.
    pub(crate) fn restart_clock(&self, key: &str, prompt_id: &str, now: DateTime<Utc>) {
        if let Ok(mut holds) = self.holds.lock() {
            if let Some(hold) = holds
                .get_mut(key)
                .filter(|hold| hold.prompt_id == prompt_id)
            {
                hold.since = now;
            }
        }
    }

    pub(crate) fn record(
        &self,
        key: &str,
        prompt_id: &str,
        startup_send: bool,
        now: DateTime<Utc>,
    ) {
        let Ok(mut holds) = self.holds.lock() else {
            return;
        };
        backoff(&mut holds, key, prompt_id, startup_send, now);
    }

    /// Whether the scheduler should re-read the screen of a session whose startup send waits for
    /// its input box now; answering yes schedules the next read.
    pub(crate) fn screen_read_due(&self, key: &str, prompt_id: &str, now: DateTime<Utc>) -> bool {
        let Ok(mut reads) = self.screen_reads.lock() else {
            return true;
        };
        let due = reads
            .get(key)
            .is_none_or(|read| read.prompt_id != prompt_id || now >= read.retry_at);
        if due {
            backoff(&mut reads, key, prompt_id, true, now);
        }
        due
    }

    /// Whether the scheduler should check now that a session holding the startup send `prompt_id`
    /// is awake; answering yes schedules the next check.
    pub(crate) fn wake_check_due(&self, key: &str, prompt_id: &str, now: DateTime<Utc>) -> bool {
        let Ok(mut checks) = self.wake_checks.lock() else {
            return false;
        };
        let check = checks.get(key).filter(|check| check.prompt_id == prompt_id);
        let since = check.map_or(now, |check| check.since);
        if check.is_some_and(|check| now < check.retry_at) {
            return false;
        }
        let first = check.is_none();
        let waited_ms = now.signed_duration_since(since).num_milliseconds();
        checks.insert(
            key.to_string(),
            ComposerHold {
                prompt_id: prompt_id.to_string(),
                since,
                retry_at: now + Duration::milliseconds(startup_send_wake_check_ms(waited_ms)),
            },
        );
        !first
    }

    pub(crate) fn clear(&self, key: &str) {
        if let Ok(mut holds) = self.holds.lock() {
            holds.remove(key);
        }
        if let Ok(mut reads) = self.screen_reads.lock() {
            reads.remove(key);
        }
        if let Ok(mut checks) = self.wake_checks.lock() {
            checks.remove(key);
        }
    }

    pub(crate) fn retain(&self, live: impl Fn(&str) -> bool) {
        if let Ok(mut holds) = self.holds.lock() {
            holds.retain(|key, _| live(key));
        }
        if let Ok(mut reads) = self.screen_reads.lock() {
            reads.retain(|key, _| live(key));
        }
        if let Ok(mut checks) = self.wake_checks.lock() {
            checks.retain(|key, _| live(key));
        }
    }
}

/// Starts or extends `key`'s clock for `prompt_id` and sets its next retry.
fn backoff(
    clocks: &mut HashMap<String, ComposerHold>,
    key: &str,
    prompt_id: &str,
    startup_send: bool,
    now: DateTime<Utc>,
) {
    let since = clocks
        .get(key)
        .filter(|hold| hold.prompt_id == prompt_id)
        .map_or(now, |hold| hold.since);
    let retry_ms = if startup_send {
        startup_send_retry_ms(now.signed_duration_since(since).num_milliseconds())
    } else {
        SESSION_CHAT_QUEUE_COMPOSER_RETRY_MS
    };
    clocks.insert(
        key.to_string(),
        ComposerHold {
            prompt_id: prompt_id.to_string(),
            since,
            retry_at: now + Duration::milliseconds(retry_ms),
        },
    );
}

/// The `Reply to:` global ref of a `ghostex agents send` header, if `text` carries one.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs writes the header; packages/gx-chat-core/src/transcript/agent_message.rs parses it for display.
pub(crate) fn agent_message_reply_to(text: &str) -> Option<(&str, &str)> {
    if let Some(parts) = crate::coordinators::split_agent_message(text) {
        return Some((parts.reply_to, parts.body.trim()));
    }
    // The pre-2026-09-18 `MESSAGE FROM` header, which `split_agent_message` does not read.
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

/// Who initiated a session close, so we can suppress delivery-failure notices when the sender or
/// its coordinator intentionally closed/resolved the target session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CloseActor {
    pub closed_by_ref: Option<String>,
    pub close_source: String,
}

impl CloseActor {
    pub(crate) fn from_params(params: &Map<String, Value>) -> Self {
        let closed_by_ref = params
            .get(crate::session_close_log::CLOSED_BY_PARAM)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let close_source = crate::session_close_log::close_source(params);
        Self {
            closed_by_ref,
            close_source,
        }
    }
}

/// Returns true when `sender` should NOT receive a "not delivered" notice for `target` being
/// closed by `actor`:
/// - `sender` itself closed/resolved `target`,
/// - `sender`'s coordinator closed/resolved `target`, or
/// - `target` was resolved (`server.threadMarkedDone`) or cascaded (`server.coordinatorClosed`)
///   under `sender` or under the same coordinator as `sender`.
pub(crate) fn should_suppress_close_notice(
    db: &rusqlite::Connection,
    server_id: &str,
    target: (&str, &str),
    sender: (&str, &str),
    actor: &CloseActor,
) -> bool {
    let sender_ref = create_global_session_ref(server_id, sender.0, sender.1);
    if actor.closed_by_ref.as_deref() == Some(sender_ref.as_str()) {
        return true;
    }
    let sender_thread = crate::coordinators::read_thread(db, sender.0, sender.1)
        .ok()
        .flatten();
    if let Some(thread) = sender_thread.as_ref() {
        let coord_ref = create_global_session_ref(
            server_id,
            &thread.coordinator_project_id,
            &thread.coordinator_session_id,
        );
        if actor.closed_by_ref.as_deref() == Some(coord_ref.as_str()) {
            return true;
        }
    }
    if matches!(
        actor.close_source.as_str(),
        "server.threadMarkedDone" | "server.coordinatorClosed"
    ) {
        if let Ok(Some(target_thread)) = crate::coordinators::read_thread(db, target.0, target.1) {
            if target_thread.coordinator_project_id == sender.0
                && target_thread.coordinator_session_id == sender.1
            {
                return true;
            }
            if let Some(sender_thread) = sender_thread.as_ref() {
                if sender_thread.coordinator_project_id == target_thread.coordinator_project_id
                    && sender_thread.coordinator_session_id == target_thread.coordinator_session_id
                {
                    return true;
                }
            }
        }
    }
    false
}

/// One queued prompt that could not be delivered to a target session.
pub(crate) struct UndeliveredPrompt<'a> {
    pub prompt_id: &'a str,
    pub text: &'a str,
    pub reason: &'a str,
}

/// Cooldown before sending another delivery-failure notice to the same sender for the same target
/// and reason after one has already been delivered, so repeated failures never spam the chat.
const DELIVERY_NOTICE_COOLDOWN_SECS: i64 = 300;

#[derive(Default)]
struct DeliveryNoticeRecord {
    notified_prompt_ids: HashSet<String>,
    last_reason: String,
    sent_at: Option<DateTime<Utc>>,
}

static DELIVERY_NOTICE_MEMORY: LazyLock<Mutex<HashMap<(String, String), DeliveryNoticeRecord>>> =
    LazyLock::new(Default::default);

/// Clears dedupe memory for a prompt the user explicitly retried, so a fresh failure after an
/// explicit retry can report once again.
pub(crate) fn forget_delivery_notice_for_retry(
    server_id: &str,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) {
    let target_ref = create_global_session_ref(server_id, project_id, session_id);
    if let Ok(mut memory) = DELIVERY_NOTICE_MEMORY.lock() {
        for ((_, recorded_target), record) in memory.iter_mut() {
            if *recorded_target == target_ref {
                record.notified_prompt_ids.remove(prompt_id);
                record.sent_at = None;
            }
        }
    }
}

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: when a queued message from another agent is marked failed, tell the agent that sent it. The notice goes into the sender's own queue as a plain Ghostex note, so it reaches the sender at its next stop and can never trigger a notice of its own.
/// Only unattended failures notify; a failed "Send now" is shown to the person who pressed it.
/// CDXC:SessionChat 2026-11-13 WHY: User: "messages that are about failing to send a message to another agent are getting stuck in the queue until I hit Send now on them, please fix." The notice went in as an ordinary row, which waits for the turn to end like a Tab-queued message, while every other Ghostex report is a held message that a busy agent takes as soon as its input box is up (GitHub #591 in session_chat_queue_runtime/scheduler.rs). A running or sleeping sender now gets it as a held message, delivered at once when idle or as soon as the input box is free mid-turn; a failed one no longer blocks the rows behind it (`deliverable_head`). A closed sender keeps the ordinary row, since a held message would wake it.
/// CDXC:SessionChat 2026-11-24 WHY:
/// Closing a thread with two queued copies of a brief used to post two giant user-looking bubbles
/// into the coordinator's chat even when the coordinator itself had just closed/resolved that
/// thread. Notices are now suppressed whenever the sender (or its coordinator) closed/resolved the
/// target, coalesced into ONE compact `<ghostex-delivery-notice>` per target (updating an already
/// queued notice row in place if one is still waiting), capped to a short single-line preview, and
/// deduped against recently delivered notices for the same target and reason.
pub(crate) fn notify_senders_of_undelivered_agent_messages(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    prompts: &[UndeliveredPrompt<'_>],
    close_actor: Option<&CloseActor>,
) -> Vec<(String, String)> {
    use ghostex_gx_chat_core::transcript::delivery_notice::{
        DELIVERY_NOTICE_PREVIEW_MAX_CHARS, format_delivery_notice, parse_delivery_notice,
        short_delivery_reason, short_first_line,
    };

    if prompts.is_empty() {
        return Vec::new();
    }

    struct SenderBatch {
        prompt_ids: Vec<String>,
        count: usize,
        reason: String,
        preview: String,
    }

    let mut by_sender: BTreeMap<(String, String), SenderBatch> = BTreeMap::new();
    for prompt in prompts {
        let Some((reply_to, body)) = agent_message_reply_to(prompt.text) else {
            continue;
        };
        let mut parts = reply_to.split(':');
        let (Some(_), Some(sender_project), Some(sender_session)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if create_global_session_ref(server_id, sender_project, sender_session) != reply_to
            || (sender_project == project_id && sender_session == session_id)
        {
            continue;
        }
        let batch = by_sender
            .entry((sender_project.to_string(), sender_session.to_string()))
            .or_insert_with(|| SenderBatch {
                prompt_ids: Vec::new(),
                count: 0,
                reason: short_delivery_reason(prompt.reason),
                preview: short_first_line(body, DELIVERY_NOTICE_PREVIEW_MAX_CHARS),
            });
        batch.count += 1;
        if !prompt.prompt_id.is_empty()
            && !batch.prompt_ids.iter().any(|id| id == prompt.prompt_id)
        {
            batch.prompt_ids.push(prompt.prompt_id.to_string());
        }
        if batch.preview.is_empty() {
            batch.preview = short_first_line(body, DELIVERY_NOTICE_PREVIEW_MAX_CHARS);
        }
    }
    if by_sender.is_empty() {
        return Vec::new();
    }

    let Ok(db) = open_gxserver_database(paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, server_id);
    let recipient = repository.get_session(project_id, session_id).ok().flatten();
    let title = recipient
        .as_ref()
        .and_then(|session| session.get("title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("the other session");
    let recipient_ref = create_global_session_ref(server_id, project_id, session_id);
    let now = Utc::now();
    let mut notified = Vec::new();

    for ((sender_project, sender_session), batch) in by_sender {
        if let Some(actor) = close_actor {
            if should_suppress_close_notice(
                &db,
                server_id,
                (project_id, session_id),
                (&sender_project, &sender_session),
                actor,
            ) {
                continue;
            }
        }
        let Ok(Some(sender)) = repository.get_session(&sender_project, &sender_session) else {
            continue;
        };
        let sender_key = format!("{sender_project}:{sender_session}");
        let mem_key = (sender_key, recipient_ref.clone());

        // Filter out prompt IDs we already notified for this sender+target.
        let (fresh_ids, fresh_count, in_cooldown) = {
            let memory = DELIVERY_NOTICE_MEMORY
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let record = memory.get(&mem_key);
            let fresh_ids: Vec<String> = batch
                .prompt_ids
                .iter()
                .filter(|id| !record.is_some_and(|rec| rec.notified_prompt_ids.contains(*id)))
                .cloned()
                .collect();
            let fresh_count = if batch.prompt_ids.is_empty() {
                batch.count
            } else {
                fresh_ids.len()
            };
            let in_cooldown = record.is_some_and(|rec| {
                rec.last_reason == batch.reason
                    && rec.sent_at.is_some_and(|at| {
                        now.signed_duration_since(at).num_seconds() < DELIVERY_NOTICE_COOLDOWN_SECS
                    })
            });
            (fresh_ids, fresh_count, in_cooldown)
        };
        if fresh_count == 0 {
            continue;
        }

        // If a compact delivery notice for this exact target is still waiting in the sender's
        // queue (not yet taken by the agent), coalesce into that one row in place.
        let existing_queued = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
            &db,
            &sender_project,
            &sender_session,
        )
        .queue
        .into_iter()
        .find_map(|row| {
            if row.state != crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_QUEUED {
                return None;
            }
            let parsed = parse_delivery_notice(&row.text)?;
            (parsed.target == recipient_ref).then_some((row.id, parsed))
        });

        if let Some((existing_row_id, existing)) = existing_queued {
            let mut merged_ids = existing.prompt_ids;
            let mut added_ids = 0usize;
            for id in &fresh_ids {
                if !merged_ids.iter().any(|seen| seen == id) {
                    merged_ids.push(id.clone());
                    added_ids += 1;
                }
            }
            let merged_count = if merged_ids.is_empty() {
                existing.count + fresh_count
            } else {
                existing.count.saturating_add(added_ids).max(merged_ids.len())
            };
            let merged_preview = if !existing.preview.is_empty() {
                existing.preview
            } else {
                batch.preview.clone()
            };
            let updated_notice = format_delivery_notice(
                &recipient_ref,
                title,
                merged_count,
                &batch.reason,
                &merged_preview,
                &merged_ids,
            );
            if crate::session_chat_queue::replace_queued_prompt_text(
                &db,
                &sender_project,
                &sender_session,
                &existing_row_id,
                &updated_notice,
            )
            .unwrap_or(false)
            {
                if let Ok(mut memory) = DELIVERY_NOTICE_MEMORY.lock() {
                    let rec = memory.entry(mem_key).or_default();
                    rec.notified_prompt_ids.extend(fresh_ids);
                    rec.last_reason = batch.reason;
                    rec.sent_at = Some(now);
                }
                notified.push((sender_project, sender_session));
                continue;
            }
        }

        // No waiting row to merge into: if we already delivered a notice for the same target and
        // reason recently, absorb these prompt IDs silently rather than repeating the notice.
        if in_cooldown {
            if let Ok(mut memory) = DELIVERY_NOTICE_MEMORY.lock() {
                let rec = memory.entry(mem_key).or_default();
                rec.notified_prompt_ids.extend(fresh_ids);
            }
            continue;
        }

        let notice = format_delivery_notice(
            &recipient_ref,
            title,
            fresh_count,
            &batch.reason,
            &batch.preview,
            &fresh_ids,
        );
        // A closed sender keeps a plain row: a held message would wake it just to read this.
        let held = matches!(
            sender.get("lifecycleState").and_then(Value::as_str),
            Some("running" | "sleeping")
        );
        if crate::session_chat_queue::enqueue_session_chat_prompt(
            &db,
            &sender_project,
            &sender_session,
            &notice,
            held,
        )
        .is_ok()
        {
            if let Ok(mut memory) = DELIVERY_NOTICE_MEMORY.lock() {
                let rec = memory.entry(mem_key).or_default();
                rec.notified_prompt_ids.extend(fresh_ids);
                rec.last_reason = batch.reason;
                rec.sent_at = Some(now);
            }
            notified.push((sender_project, sender_session));
        }
    }

    notified
}

pub(crate) fn notify_sender_of_undelivered_agent_message(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    prompt_text: &str,
    reason: &str,
) -> Option<(String, String)> {
    notify_senders_of_undelivered_agent_messages(
        paths,
        server_id,
        project_id,
        session_id,
        &[UndeliveredPrompt {
            prompt_id,
            text: prompt_text,
            reason,
        }],
        None,
    )
    .into_iter()
    .next()
}
