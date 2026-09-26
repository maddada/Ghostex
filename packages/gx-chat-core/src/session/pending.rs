//! Optimistic pending sends: how an echo is matched with the authoritative turn that replaces it.
//!
//! Ported from `packages/core-ui/chat/session-chat-pending.ts`. Pending echoes render identically
//! to real user turns, so replacement by the real transcript turn causes no visible state change.
//!
//! The TypeScript's `afterMessageId` had three states; only two ever occurred, because both
//! constructors wrote an explicit id or an explicit `null`. `None` here is that `null`: no prior
//! row, so the boundary falls back to the send time.

use std::collections::BTreeMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource, StartupDelivery};

use crate::session::constants::PENDING_ID_PREFIX;
use crate::session::text::{is_staged_path_input, normalize_pending_text, parse_command_envelope};
use crate::state::PendingSend;

/// `sessionChatPendingContentKey`: what two sends must agree on to be the same prompt.
pub fn content_key(text: &str, image_paths: &[String]) -> String {
    let normalized = normalize_pending_text(text);
    if !normalized.is_empty() {
        return format!("text:{normalized}");
    }
    let paths: Vec<&String> = image_paths.iter().filter(|path| !path.is_empty()).collect();
    if paths.is_empty() {
        return "empty".to_string();
    }
    format!(
        "images:{}",
        serde_json::to_string(&paths).unwrap_or_default()
    )
}

/// `sessionChatPendingMatchKey`: the content key under its boundary.
pub fn match_key(entry: &PendingSend) -> String {
    format!(
        "{}\0{}",
        entry.after_message_id.clone().unwrap_or("null".to_string()),
        content_key(&entry.text, &entry.image_paths)
    )
}

fn message_is_after_pending_timestamp(message: &ChatMessage, pending: &PendingSend) -> bool {
    let Some(stamp) = message.timestamp else {
        // Some transcripts (Grok) never carry timestamps; excluding them would strand a
        // rank-pinned bubble at the list tail forever.
        return true;
    };
    let boundary = pending
        .matching_after_timestamp
        .or(pending.after_message_timestamp)
        .unwrap_or(pending.sent_at_ms);
    match pending.after_message_timestamp {
        // The local send time: no existing record, so the comparison is inclusive.
        None => stamp >= boundary,
        // A transcript-clock boundary describes an EXISTING message, so it is exclusive.
        Some(_) => stamp > boundary,
    }
}

/// `messagesAfterPendingBoundary`.
pub fn messages_after_boundary<'a>(
    messages: &'a [ChatMessage],
    pending: &PendingSend,
) -> Vec<&'a ChatMessage> {
    let Some(after_id) = pending.after_message_id.as_ref() else {
        return messages
            .iter()
            .filter(|message| message_is_after_pending_timestamp(message, pending))
            .collect();
    };
    match messages.iter().position(|message| &message.id == after_id) {
        Some(index) => messages[index + 1..].iter().collect(),
        // A bounded read can page the boundary out: fall back to the send time, never to an
        // arbitrary older prompt.
        None => messages
            .iter()
            .filter(|message| message_is_after_pending_timestamp(message, pending))
            .collect(),
    }
}

fn block_text(message: &ChatMessage, separator: &str) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(separator)
}

fn user_message_content_key(message: &ChatMessage) -> String {
    let envelope = parse_command_envelope(&block_text(message, "\n"));
    let text = match &envelope {
        Some(envelope) => format!("{} {}", envelope.name, envelope.args),
        None => block_text(message, "\n"),
    };
    let image_paths: Vec<String> = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::ImageRef { path, url, .. } => path
                .clone()
                .or_else(|| url.clone())
                .filter(|value| !value.is_empty()),
            _ => None,
        })
        .collect();
    content_key(&text, &image_paths)
}

/// ALL user messages, counted by content key.
pub fn matching_user_content_counts(messages: &[&ChatMessage]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for message in messages {
        if !matches!(message.role, ChatRole::User) {
            continue;
        }
        *counts.entry(user_message_content_key(message)).or_insert(0) += 1;
    }
    counts
}

/// CDXC:SessionChat 2026-09-19 WHY:
/// A mid-turn send parked in the agent CLI's queue can be pulled back into the terminal input,
/// edited, and resubmitted, so its original text never reaches the transcript as a user row. An
/// echo that waited for that row stayed on screen labelled Queued forever and pinned the pane in
/// the optimistic working state. The server's queued row is the authoritative stand-in from the
/// moment it appears: it is retracted when the queue releases the entry and replaced by the
/// delivered row, so the echo retires as soon as that row exists.
pub fn advanced_user_content_counts(messages: &[&ChatMessage]) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut waiting: Vec<String> = Vec::new();
    for message in messages {
        if matches!(message.role, ChatRole::User) {
            let key = user_message_content_key(message);
            if parse_command_envelope(&block_text(message, "\n")).is_some() || message.queued {
                // Neither needs an assistant reply to advance the turn: local commands finish
                // without one, and a queued row is owned by the server.
                *counts.entry(key).or_insert(0) += 1;
            } else {
                waiting.push(key);
            }
            continue;
        }
        for key in waiting.drain(..) {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
}

fn user_texts(messages: &[&ChatMessage], advanced: bool) -> Vec<String> {
    let mut texts = Vec::new();
    let mut waiting: Vec<String> = Vec::new();
    for message in messages {
        if matches!(message.role, ChatRole::User) {
            let text = normalize_pending_text(&block_text(message, "\n"));
            if advanced {
                waiting.push(text);
            } else {
                texts.push(text);
            }
            continue;
        }
        if advanced {
            texts.append(&mut waiting);
        }
    }
    texts
}

/// Every user text, normalized.
pub fn matching_user_texts(messages: &[&ChatMessage]) -> Vec<String> {
    user_texts(messages, false)
}

/// User texts that have a later non-user turn.
pub fn advanced_user_texts(messages: &[&ChatMessage]) -> Vec<String> {
    user_texts(messages, true)
}

/// CDXC:SessionChat 2026-09-03 WHY:
/// Two glue shapes reach the transcript. A rapid-send burst lands byte-adjacent ("joke" +
/// "continue"). Prompts sent while the agent is busy are parked in the agent's own queue instead,
/// and when the turn ends it pops ALL of them and submits them as ONE user row joined by newlines,
/// which the whitespace folding turns into single spaces. Requiring byte adjacency alone left both
/// echoes on screen with the Queued label forever, which also pinned the pane in the optimistic
/// working state after the reply had long landed. Every piece is trimmed, so an optional single
/// space between pieces is the only separator to allow, and it cannot make a real prefix ("hi"
/// versus "history") match.
pub fn count_leading_pending_texts_glued(pending_texts: &[String], user_text: &str) -> usize {
    let mut consumed = 0usize;
    for (index, piece) in pending_texts.iter().enumerate() {
        if piece.is_empty() {
            return 0;
        }
        if index > 0 && user_text[consumed..].starts_with(' ') {
            consumed += 1;
        }
        if !user_text[consumed..].starts_with(piece.as_str()) {
            return 0;
        }
        consumed += piece.len();
        if consumed == user_text.len() {
            return index + 1;
        }
    }
    0
}

/// Which pending entries a glued user turn already represents.
pub fn select_pending_indices_represented(
    pending: &[&PendingSend],
    user_text_list: &[String],
) -> Vec<usize> {
    let mut represented = Vec::new();
    if pending.len() < 2 || user_text_list.is_empty() {
        return represented;
    }
    let mut remaining: Vec<(usize, String)> = pending
        .iter()
        .enumerate()
        .map(|(index, entry)| (index, normalize_pending_text(&entry.text)))
        .collect();
    for user_text in user_text_list {
        let texts: Vec<String> = remaining.iter().map(|(_, text)| text.clone()).collect();
        let glued = count_leading_pending_texts_glued(&texts, user_text);
        if glued < 2 {
            // 1 is an exact match: leave it to occurrence counting.
            continue;
        }
        for (index, _) in remaining.iter().take(glued) {
            represented.push(*index);
        }
        remaining = remaining.split_off(glued);
    }
    represented
}

/// Which counting mode a filter pass uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Prune: only a user text that an assistant turn has already answered consumes an echo.
    Advanced,
    /// Visibility: any matching user row hides the echo, even before the reply lands.
    Matching,
}

fn filter_pending_sends(
    pending: &[PendingSend],
    messages: &[ChatMessage],
    mode: Mode,
) -> Vec<PendingSend> {
    let counts = |rows: &[&ChatMessage]| match mode {
        Mode::Advanced => advanced_user_content_counts(rows),
        Mode::Matching => matching_user_content_counts(rows),
    };
    let texts = |rows: &[&ChatMessage]| match mode {
        Mode::Advanced => advanced_user_texts(rows),
        Mode::Matching => matching_user_texts(rows),
    };

    let mut consumed: BTreeMap<String, usize> = BTreeMap::new();
    let exact_keep: Vec<bool> = pending
        .iter()
        .map(|entry| {
            let content = content_key(&entry.text, &entry.image_paths);
            let key = match_key(entry);
            let available = counts(&messages_after_boundary(messages, entry))
                .get(&content)
                .copied()
                .unwrap_or(0);
            let used = consumed.get(&key).copied().unwrap_or(0);
            let occurrence = entry
                .matching_occurrence
                .map(|value| value as usize)
                .unwrap_or(used + 1);
            consumed.insert(key, used.max(occurrence));
            occurrence > available
        })
        .collect();

    let still_open: Vec<&PendingSend> = pending
        .iter()
        .zip(&exact_keep)
        .filter(|(_, keep)| **keep)
        .map(|(entry, _)| entry)
        .collect();
    let all_rows: Vec<&ChatMessage> = messages.iter().collect();
    let glued = select_pending_indices_represented(&still_open, &texts(&all_rows));

    let mut embedded: Vec<usize> = Vec::new();
    for (index, entry) in still_open.iter().enumerate() {
        let pending_text = normalize_pending_text(&entry.text);
        if pending_text.is_empty() {
            continue;
        }
        // The authoritative turn can prepend input that was already staged in the agent's input
        // line when the send landed, making the pending text an exact suffix rather than an exact
        // whole-turn match. Two known shapes: Codex's steering bundle (separator preserved in the
        // optimistic text), and a drag-dropped file path typed into the terminal before the send.
        // Anything else stays unconsumed so ordinary suffixes ("fun" in "jokes are fun") cannot
        // consume an echo.
        let is_steering_bundle = pending_text.contains(" --- ");
        let represented =
            texts(&messages_after_boundary(messages, entry))
                .iter()
                .any(|user_text| {
                    if user_text == &pending_text || !user_text.ends_with(&pending_text) {
                        return false;
                    }
                    if is_steering_bundle {
                        return true;
                    }
                    is_staged_path_input(&user_text[..user_text.len() - pending_text.len()])
                });
        if represented {
            embedded.push(index);
        }
    }

    let mut open_index: isize = -1;
    let next: Vec<PendingSend> = pending
        .iter()
        .zip(&exact_keep)
        .filter(|(_, keep)| **keep)
        .filter(|_| {
            open_index += 1;
            let at = open_index as usize;
            !glued.contains(&at) && !embedded.contains(&at)
        })
        .map(|(entry, _)| entry.clone())
        .collect();
    if next.len() == pending.len() {
        pending.to_vec()
    } else {
        next
    }
}

/// Prune rule (drop the echo): keep it through the user-only transcript phase, and prune only once
/// an assistant or other turn has landed after the matching user text. Otherwise a first turn
/// flashes the empty state before the reply arrives. A local command acknowledgment retires its
/// echo immediately, because it needs no reply.
pub fn prune_pending_sends(pending: &[PendingSend], messages: &[ChatMessage]) -> Vec<PendingSend> {
    filter_pending_sends(pending, messages, Mode::Advanced)
}

/// Visibility rule (hide the echo): identical structure, but counting ALL user messages, so an
/// echo is hidden as soon as the transcript carries its user row.
pub fn visible_pending_sends(
    pending: &[PendingSend],
    messages: &[ChatMessage],
) -> Vec<PendingSend> {
    filter_pending_sends(pending, messages, Mode::Matching)
}

/// `assignSessionChatPendingOccurrence`: which transcript occurrence this send is waiting for.
///
/// Pruning an earlier echo must not let a later identical send reuse the same occurrence.
pub fn assign_occurrence(existing: &[PendingSend], entry: PendingSend) -> PendingSend {
    let key = match_key(&entry);
    let matching: Vec<&PendingSend> = existing
        .iter()
        .filter(|candidate| match_key(candidate) == key)
        .collect();
    let Some(first) = matching.first() else {
        return entry;
    };
    let mut previous_occurrence = 0u32;
    for (index, candidate) in matching.iter().enumerate() {
        previous_occurrence =
            previous_occurrence.max(candidate.matching_occurrence.unwrap_or(index as u32 + 1));
    }
    PendingSend {
        matching_after_timestamp: first
            .matching_after_timestamp
            .or(first.after_message_timestamp)
            .or(Some(first.sent_at_ms)),
        matching_occurrence: Some(previous_occurrence + 1),
        ..entry
    }
}

/// `sessionChatPendingSendsAsMessages`: the echoes, as transcript rows.
pub fn pending_sends_as_messages(pending: &[PendingSend]) -> Vec<ChatMessage> {
    pending
        .iter()
        .map(|entry| {
            let mut blocks: Vec<ChatBlock> = entry
                .image_paths
                .iter()
                .map(|path| ChatBlock::ImageRef {
                    path: Some(path.clone()),
                    url: None,
                    alt: None,
                })
                .collect();
            if !entry.text.trim().is_empty() {
                blocks.push(ChatBlock::Text {
                    text: entry.text.clone(),
                });
            }
            let startup_delivery = entry.startup_delivery.clone().or_else(|| {
                entry
                    .queued_prompt_id
                    .clone()
                    .map(|prompt_id| StartupDelivery {
                        prompt_id,
                        state: "queued".to_string(),
                        error_message: None,
                    })
            });
            ChatMessage {
                id: format!("{PENDING_ID_PREFIX}{}", entry.id),
                role: ChatRole::User,
                blocks,
                async_questions: None,
                timestamp: Some(entry.sent_at_ms),
                // Lowest priority: the real transcript turn always supersedes.
                source: ChatSource::Client,
                turn_id: None,
                byte_offset: None,
                queued: startup_delivery.is_none() && entry.sent_while_working,
                deferred_work: None,
                startup_delivery,
            }
        })
        .collect()
}
