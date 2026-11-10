//! Optimistic pending sends: how an echo is matched with the authoritative turn that replaces it.
//!
//! Ported from `packages/core-ui/chat/session-chat-pending.ts`. Pending echoes render identically
//! to real user turns, so replacement by the real transcript turn causes no visible state change.
//!
//! The TypeScript's `afterMessageId` had three states; only two ever occurred, because both
//! constructors wrote an explicit id or an explicit `null`. `None` here is that `null`: no prior
//! row, so the boundary falls back to the send time.

use crate::state::ChatRow;
use std::collections::BTreeMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource, StartupDelivery};

use crate::session::appended_notes::without_appended_notes;
use crate::session::constants::{DELIVERY_UNCONFIRMED, PENDING_ID_PREFIX};
use crate::session::text::{
    is_js_space, is_staged_path_input, normalize_pending_text, parse_command_envelope,
};
use crate::state::PendingSend;
use crate::transcript::local_command::{shell_command_message, shell_command_prompt_text};

/// `sessionChatPendingContentKey`: what two sends must agree on to be the same prompt.
///
/// CDXC:SessionChat 2026-11-08 WHY:
/// The key ignores whitespace altogether, not only its amount (#509). Codex on Windows recorded a
/// queued file-annotation message with its line breaks gone ("(lines 9–10)Comment on:" where the
/// chat sent "(lines 9–10)\nComment on:"), so the folded texts differed by one space and the queue
/// card stayed under the finished turn saying the conversation didn't show it. Two prompts that
/// differ only in where their spaces and line breaks fall are the same prompt.
pub fn content_key(text: &str, image_paths: &[String]) -> String {
    let normalized: String = shell_command_key(normalize_pending_text(text))
        .split(is_js_space)
        .collect();
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

pub(crate) fn message_is_after_pending_timestamp(
    message: &ChatMessage,
    pending: &PendingSend,
) -> bool {
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
    messages: &'a [ChatRow],
    pending: &PendingSend,
) -> Vec<&'a ChatMessage> {
    let Some(after_id) = pending.after_message_id.as_ref() else {
        return messages
            .iter()
            .map(|message| &**message)
            .filter(|message| message_is_after_pending_timestamp(message, pending))
            .collect();
    };
    match messages.iter().position(|message| &message.id == after_id) {
        Some(index) => messages[index + 1..]
            .iter()
            .map(|message| &**message)
            .collect(),
        // A bounded read can page the boundary out: fall back to the send time, never to an
        // arbitrary older prompt.
        None => messages
            .iter()
            .map(|message| &**message)
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

/// `!aws login` and `! aws login` run the same command: Claude records both as `aws login`.
fn shell_command_key(normalized: String) -> String {
    match normalized.strip_prefix('!') {
        Some(command) => format!("!{}", command.trim_start()),
        None => normalized,
    }
}

/// A user row's text as the composer would have sent it: a `!` command's `<bash-input>` row reads
/// as the `!` line that ran it, and a turn gxserver added a note under reads without that note
/// (`session/appended_notes.rs`).
fn user_message_text(message: &ChatMessage) -> String {
    shell_command_prompt_text(message)
        .unwrap_or_else(|| without_appended_notes(&block_text(message, "\n")).to_string())
}

fn user_message_content_key(message: &ChatMessage) -> String {
    let envelope = parse_command_envelope(&block_text(message, "\n"));
    let text = match &envelope {
        Some(envelope) => format!("{} {}", envelope.name, envelope.args),
        None => user_message_text(message),
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
            if parse_command_envelope(&block_text(message, "\n")).is_some()
                || shell_command_prompt_text(message).is_some()
                || message.queued
            {
                // None needs an assistant reply to advance the turn: local and `!` commands finish
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
            let text = shell_command_key(normalize_pending_text(&user_message_text(message)));
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

/// User rows as (id, normalized text); with `answered`, only those a later non-user turn follows.
fn user_rows<'a>(messages: &[&'a ChatMessage], answered: bool) -> Vec<(&'a str, String)> {
    let mut rows = Vec::new();
    let mut waiting = Vec::new();
    for message in messages {
        if matches!(message.role, ChatRole::User) {
            waiting.push((
                message.id.as_str(),
                shell_command_key(normalize_pending_text(&user_message_text(message))),
            ));
            continue;
        }
        rows.append(&mut waiting);
    }
    if !answered {
        rows.append(&mut waiting);
    }
    rows
}

/// A user turn that is the pending text typed more than once back to back.
fn repeats_pending_text(pending_text: &str, user_text: &str) -> bool {
    if pending_text.is_empty() {
        return false;
    }
    let pieces = vec![pending_text.to_string(); user_text.len() / pending_text.len() + 1];
    count_leading_pending_texts_glued(&pieces, user_text) >= 2
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
    messages: &[ChatRow],
    mode: Mode,
) -> Vec<PendingSend> {
    // A send still waiting for its pictures has not left, so no transcript row can be it: it stays,
    // and the others are matched as if it were not there (`crate::composer::attaching`).
    if pending.iter().any(|entry| entry.awaiting_attachments) {
        let open: Vec<PendingSend> = pending
            .iter()
            .filter(|entry| !entry.awaiting_attachments)
            .cloned()
            .collect();
        let kept = filter_sent_pending_sends(&open, messages, mode);
        return pending
            .iter()
            .filter(|entry| {
                entry.awaiting_attachments || kept.iter().any(|kept| kept.id == entry.id)
            })
            .cloned()
            .collect();
    }
    filter_sent_pending_sends(pending, messages, mode)
}

fn filter_sent_pending_sends(
    pending: &[PendingSend],
    messages: &[ChatRow],
    mode: Mode,
) -> Vec<PendingSend> {
    // CDXC:SessionChat 2026-10-10 WHY: matching an echo normalizes the text of every user message in the transcript, and this runs on every event the core handles (a live update, the composer's width while a panel slides). With nothing pending the answer is always nothing, so a very long thread no longer pays for its whole history on each event.
    if pending.is_empty() {
        return Vec::new();
    }
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
    // Every echo matched its row exactly: nothing below can keep one.
    if still_open.is_empty() {
        return Vec::new();
    }
    // CDXC:SessionChat 2026-10-23 WHY: the glued match reads the text of every user message in
    // the transcript, and it can only match two echoes or more. A prompt queued behind a long turn
    // kept one echo open for the whole turn, and a 3,400-message chat paid about 35 ms of every
    // event for texts nothing read.
    let glued = if still_open.len() < 2 {
        Vec::new()
    } else {
        let all_rows: Vec<&ChatMessage> = messages.iter().map(|message| &**message).collect();
        select_pending_indices_represented(&still_open, &texts(&all_rows))
    };

    let mut embedded: Vec<usize> = Vec::new();
    for (index, entry) in still_open.iter().enumerate() {
        let pending_text = shell_command_key(normalize_pending_text(&entry.text));
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
                    if is_steering_bundle || repeats_pending_text(&pending_text, user_text) {
                        return true;
                    }
                    is_staged_path_input(&user_text[..user_text.len() - pending_text.len()])
                });
        if represented {
            embedded.push(index);
        }
    }

    /*
    CDXC:SessionChat 2026-10-08 WHY:
    An echo is matched by text, so a recorded prompt that differs from what the composer sent stranded it under the real turn forever: on 2026-10-08 a late first paste made Claude record the message twice as one prompt, and the single-message echo stayed below the answer. A repeated message is matched above; for any other difference, an echo still unmatched once two answered user turns follow its send is retired by order, taking the first of them. One turn is not enough: a send typed while an earlier one is still being answered is recorded only after that turn ends.
    CDXC:SessionChat 2026-11-08 WHY:
    A queued send whose row left the queue and that still matches no turn once its time on its way is up (`DELIVERY_UNCONFIRMED`, session/startup_sends.rs) is retired by order too, by the first user turn after its send that no echo claims, and that turn need not be answered yet to hide the card. gxserver types one queued row per idle window, so that turn is the one it typed. On #509 Codex on Windows recorded a queued file-annotation message in other words than the chat sent, and its card sat for good under a turn that had finished, saying the conversation didn't show it. Rows still in the queue or on their way keep waiting for their own text.
    */
    let pending_texts: Vec<String> = pending
        .iter()
        .map(|entry| shell_command_key(normalize_pending_text(&entry.text)))
        .collect();
    let mut claimed: Vec<&str> = Vec::new();
    let mut superseded: Vec<usize> = Vec::new();
    for (index, entry) in still_open.iter().enumerate() {
        let delivered = entry
            .startup_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.state == DELIVERY_UNCONFIRMED);
        if glued.contains(&index)
            || embedded.contains(&index)
            || (!delivered
                && (entry.queued_prompt_id.is_some() || entry.startup_delivery.is_some()))
        {
            continue;
        }
        let answered = !delivered || mode == Mode::Advanced;
        let rows: Vec<(&str, String)> =
            user_rows(&messages_after_boundary(messages, entry), answered)
                .into_iter()
                .filter(|(id, text)| !claimed.contains(id) && !pending_texts.contains(text))
                .collect();
        if rows.len() >= if delivered { 1 } else { 2 } {
            claimed.push(rows[0].0);
            superseded.push(index);
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
            !glued.contains(&at) && !embedded.contains(&at) && !superseded.contains(&at)
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
pub fn prune_pending_sends(pending: &[PendingSend], messages: &[ChatRow]) -> Vec<PendingSend> {
    filter_pending_sends(pending, messages, Mode::Advanced)
}

/// Visibility rule (hide the echo): identical structure, but counting ALL user messages, so an
/// echo is hidden as soon as the transcript carries its user row.
pub fn visible_pending_sends(pending: &[PendingSend], messages: &[ChatRow]) -> Vec<PendingSend> {
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
pub fn pending_sends_as_messages(pending: &[PendingSend]) -> Vec<ChatRow> {
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
                        wait_reason: None,
                    })
            });
            let echo = ChatMessage {
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
                api_error: None,
                mention_paths: None,
                custom_message: None,
            };
            let command = entry
                .text
                .trim()
                .strip_prefix('!')
                .map(str::trim)
                .filter(|command| entry.shell_command && !command.is_empty());
            std::sync::Arc::new(match command {
                Some(command) => shell_command_message(&echo, command, None),
                None => echo,
            })
        })
        .collect()
}
