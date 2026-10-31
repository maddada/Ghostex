//! Sends that were accepted on another client, or before this view existed, reconstructed from the
//! queue.
//!
//! Ported from `packages/core-ui/chat/session-chat-startup-sends.ts`.

use ghostex_gx_protocol::StartupDelivery;
use serde_json::Value;

use crate::session::constants::{
    DELIVERY_UNCONFIRMED, LEFT_QUEUE_RETENTION_MS, LEFT_QUEUE_UNCONFIRMED_WORDS,
};
use crate::session::text::normalize_pending_text;
use crate::state::PendingSend;

fn queue_field<'a>(prompt: &'a Value, key: &str) -> Option<&'a str> {
    prompt.get(key)?.as_str()
}

/// The queue's copies of the pictures a row carries (CDXC:SessionChat 2026-10-11 in
/// server/src/session_chat_queue_attachments.rs).
fn queue_images(prompt: &Value) -> Vec<String> {
    prompt
        .get("imagePaths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Reconstructs accepted sends and reconciles each receipt with the immediate local echo.
///
/// CDXC:SessionChat 2026-09-27 WHY:
/// A queued row holds the draft as written, so a Claude skill pill is still its `[/name](…/SKILL.md)` link, while the local echo and the turn Claude records carry the bare `/name` gxserver types. Comparing the raw row left the first message of a new chat unpaired: the core built a second "Waiting for agent…" echo from the row, which never matched the recorded turn and stayed under it for good. The row is read as the agent receives it, like the echo.
pub fn pending_with_startup_sends(
    pending: &[PendingSend],
    queue: &[Value],
    agent: Option<&str>,
) -> Vec<PendingSend> {
    let queue_ids: Vec<&str> = queue
        .iter()
        .filter_map(|row| queue_field(row, "id"))
        .collect();
    let mut entries: Vec<PendingSend> = pending
        .iter()
        .filter(|entry| match &entry.startup_delivery {
            Some(delivery) => {
                delivery.state != "failed" || queue_ids.contains(&delivery.prompt_id.as_str())
            }
            None => true,
        })
        .cloned()
        .collect();
    let mut matched: Vec<String> = Vec::new();

    for prompt in queue {
        let Some(prompt_id) = queue_field(prompt, "id") else {
            continue;
        };
        let by_receipt = entries
            .iter()
            .position(|entry| entry.queued_prompt_id.as_deref() == Some(prompt_id));
        let startup_send = prompt
            .get("startupSend")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // Answers a page or question form sent are drawn in the transcript at once too
        // (`session/answer_echoes.rs`), never in the strip above the composer.
        let answers = crate::transcript::page_answers::answers_page_id(
            queue_field(prompt, "text").unwrap_or_default(),
        )
        .is_some();
        if !startup_send && !answers && by_receipt.is_none() {
            continue;
        }
        // A queue broadcast can beat the send response. Pair identical local sends one at a time
        // until their receipts arrive; the persisted identity is the row id.
        let prompt_text = crate::composer::skill_invocation::agent_skill_text(
            queue_field(prompt, "text").unwrap_or_default(),
            agent,
        );
        let prompt_text = prompt_text.as_ref();
        // The row holds copies of the echo's pictures, so only their count can be compared.
        let images = queue_images(prompt);
        let at = by_receipt.or_else(|| {
            entries.iter().position(|entry| {
                entry.queued_prompt_id.is_none()
                    && entry.image_paths.len() == images.len()
                    && !matched.contains(&entry.id)
                    && normalize_pending_text(&entry.text) == normalize_pending_text(prompt_text)
            })
        });
        let delivery = StartupDelivery {
            prompt_id: prompt_id.to_string(),
            state: queue_field(prompt, "state").unwrap_or_default().to_string(),
            error_message: queue_field(prompt, "errorMessage")
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            wait_reason: queue_field(prompt, "waitReason")
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        };
        match at {
            Some(at) => {
                matched.push(entries[at].id.clone());
                let entry = &mut entries[at];
                let changed = entry.queued_prompt_id.as_deref() != Some(prompt_id)
                    || entry
                        .startup_delivery
                        .as_ref()
                        .map(|value| value.state.as_str())
                        != Some(delivery.state.as_str())
                    || entry
                        .startup_delivery
                        .as_ref()
                        .and_then(|value| value.error_message.clone())
                        != delivery.error_message
                    || entry
                        .startup_delivery
                        .as_ref()
                        .and_then(|value| value.wait_reason.clone())
                        != delivery.wait_reason;
                if changed {
                    entry.queued_prompt_id = Some(prompt_id.to_string());
                    entry.startup_delivery = Some(delivery);
                }
            }
            None => entries.push(PendingSend {
                id: format!("startup:{prompt_id}"),
                queued_prompt_id: Some(prompt_id.to_string()),
                startup_delivery: Some(delivery),
                text: prompt_text.to_string(),
                image_paths: images,
                sent_at_ms: parse_iso_ms(queue_field(prompt, "createdAt")).unwrap_or(0),
                after_message_id: None,
                after_message_timestamp: None,
                matching_occurrence: None,
                matching_after_timestamp: None,
                sent_while_working: false,
                shell_command: false,
                left_queue_at_ms: None,
            }),
        }
    }
    entries
}

/// Sends whose queue row is gone stop saying they wait, and end with what is known about them.
///
/// `oldest_loaded_ms` is the first loaded row's time while older rows exist.
///
/// CDXC:SessionChat 2026-10-31 DECISION:
/// User: "ok please give the user a way to remove stuck cards like that please and we need to detect this also". Only the queue knows a send still waits, yet an echo kept "Waiting for the agent to be free…" after its row left, until the transcript carried the message. It may never carry it: a remote Windows machine lost power, Claude's file came back with the delivered answers zeroed out, and a reader opening that chat five hours later got the row back from a saved snapshot, so the card sat over a working agent for good. A row that left reads as on its way for [`LEFT_QUEUE_RETENTION_MS`]. Then a send older than the loaded rows goes, since the transcript above them can hold it and only a page read could tell; any other says plainly the conversation doesn't show it, with no spinner and with Remove. A row still in the queue is a real wait however long it lasts, and stays as the row says.
/// SEE-ALSO: session/answer_echoes.rs (gxserver's resource index confirms answers the window can't show), composer/actions.rs and composer/send.rs (Remove), apps/desktop/src/app/native_chat/page_answers.rs, startup_delivery.rs, inter_agent_message.rs; apps/mobile/app/src/chat/native/transcript/PageAnswersCard.tsx, SystemRows.tsx, MessageActions.tsx, AgentMessageCards.tsx.
pub fn settle_rows_gone_from_queue(
    entries: Vec<PendingSend>,
    queue: &[Value],
    now_ms: i64,
    oldest_loaded_ms: Option<i64>,
) -> Vec<PendingSend> {
    let in_queue = |id: &str| queue.iter().any(|row| queue_field(row, "id") == Some(id));
    entries
        .into_iter()
        .filter_map(|mut entry| {
            // A form's echo names no row until its send is answered; it has its own ending.
            let Some(row) = entry
                .queued_prompt_id
                .clone()
                .or_else(|| {
                    entry
                        .startup_delivery
                        .as_ref()
                        .map(|delivery| delivery.prompt_id.clone())
                })
                .filter(|row| !row.is_empty())
            else {
                return Some(entry);
            };
            if in_queue(&row) {
                entry.left_queue_at_ms = None;
                return Some(entry);
            }
            let left_at = *entry.left_queue_at_ms.get_or_insert(now_ms);
            let on_its_way = now_ms - left_at < LEFT_QUEUE_RETENTION_MS;
            if !on_its_way && oldest_loaded_ms.is_some_and(|oldest| entry.sent_at_ms < oldest) {
                return None;
            }
            entry.startup_delivery = Some(if on_its_way {
                StartupDelivery {
                    prompt_id: row,
                    state: "sending".to_string(),
                    error_message: None,
                    wait_reason: None,
                }
            } else {
                StartupDelivery {
                    prompt_id: row,
                    state: DELIVERY_UNCONFIRMED.to_string(),
                    error_message: None,
                    wait_reason: Some(LEFT_QUEUE_UNCONFIRMED_WORDS.to_string()),
                }
            });
            Some(entry)
        })
        .collect()
}

/// Preserves hidden startup sends in their slots when the visible queue is reordered.
pub fn full_queue_order(queue: &[Value], visible_ids: &[String]) -> Vec<String> {
    let mut requested: Vec<String> = Vec::new();
    for id in visible_ids {
        if requested.contains(id) {
            continue;
        }
        if queue
            .iter()
            .any(|prompt| queue_field(prompt, "id") == Some(id.as_str()))
        {
            requested.push(id.clone());
        }
    }
    let mut index = 0;
    queue
        .iter()
        .filter_map(|prompt| queue_field(prompt, "id"))
        .map(|id| {
            if requested.iter().any(|value| value == id) {
                let value = requested.get(index).cloned().unwrap_or_default();
                index += 1;
                value
            } else {
                id.to_string()
            }
        })
        .collect()
}

/// `Date.parse` of an ISO 8601 stamp in epoch milliseconds.
///
/// One rule for the whole crate since 2026-09-22 (`crate::jstime`). This copy ignored a numeric
/// offset outright and returned a whole second for a one- or two-digit fraction, so a `+04:00`
/// stamp was read four hours late and `.5` lost half a second.
pub fn parse_iso_ms(value: Option<&str>) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(value?).map(|millis| millis as i64)
}
