//! Answers a reader sent from a ```questions form are shown at once as an optimistic "You answered"
//! card. A page's answers use a queue echo when the Advanced wait setting is on, or the transcript
//! message when a normal send records it. The form or page already reads Answered.
//!
//! With the Advanced wait setting, answers travel through the session's chat queue. Otherwise
//! they use a normal chat send, which a busy agent can take after its current tool call. A queued
//! answers row becomes an echo the way a startup send does
//! (`session/startup_sends.rs`), so it keeps its card through the gap between delivery and the
//! transcript, and a form's own send draws one before gxserver has even answered. The echo goes
//! when the transcript carries that page's answers.
//!
//! CDXC:SessionChat 2026-10-14 DECISION:
//! User: "When I answered the question in the chat view (in the HTML visual modal), it took about 5 seconds for the answer to show in the chat. I want it as fast as possible. Could be an optimistic update with a spinner at the bottom left of the answers card until confirmed, but I want the user to see that the action had an effect faster." The answers card shows at once with a spinner at its bottom left, the form or page reads Answered, the confirmed card takes its place without a jump, and a failed delivery turns the card into an error with Retry that keeps the answers.

use crate::state::ChatRow;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

use ghostex_gx_protocol::{ChatBlock, ChatRole, ChatSource, StartupDelivery};

use crate::session::pending::message_is_after_pending_timestamp;
use crate::session::text::normalize_pending_text;
use crate::state::{ChatContext, ChatState, FormState, PendingSend};
use crate::transcript::page_answers::{answers_page_ids, page_answers_in, split_all_page_answers};
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_forms::answers_message;

/// The id prefix of the echo a form's own send draws before its queue row or transcript exists.
const FORM_ECHO_PREFIX: &str = "answers:";

/// Answer rows and boundary positions parsed once for a loaded transcript.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerIndex {
    key: Option<crate::session::composition::BoundariedKey>,
    positions: HashMap<String, usize>,
    pages: HashMap<String, HashMap<serde_json::Value, Vec<(usize, usize)>>>,
}

impl AnswerIndex {
    /// Whether a real transcript row (not this client's echo) carries answers for the page or form.
    pub fn holds(&self, page_id: &str) -> bool {
        self.pages.contains_key(page_id)
    }

    fn from_rows(transcript: &[ChatRow]) -> Self {
        let mut index = Self::default();
        for (at, message) in transcript.iter().enumerate() {
            if message.role != ChatRole::User
                || message.source != ChatSource::Transcript
                || message.queued
                || crate::transcript::page_answers::is_unsent_or_failed(message)
            {
                continue;
            }
            let text = match message.blocks.as_slice() {
                [ChatBlock::Text { text }] => Cow::Borrowed(text.as_str()),
                blocks => Cow::Owned(prose_markdown(blocks)),
            };
            for (occurrence, record) in split_all_page_answers(&text).0.into_iter().enumerate() {
                index
                    .pages
                    .entry(record.page_id)
                    .or_default()
                    .entry(record.answers)
                    .or_default()
                    .push((at, occurrence));
            }
        }
        if !index.pages.is_empty() {
            for (at, message) in transcript.iter().enumerate() {
                index.positions.entry(message.id.clone()).or_insert(at);
            }
        }
        index
    }

    fn is_after(&self, transcript: &[ChatRow], at: usize, pending: &PendingSend) -> bool {
        pending
            .after_message_id
            .as_deref()
            .and_then(|id| self.positions.get(id))
            .map_or_else(
                || message_is_after_pending_timestamp(&transcript[at], pending),
                |boundary| at > *boundary,
            )
    }
}

/// Whether [`settle`] has anything to do: a pending send that carries a page's answers, or a form
/// whose send is in flight. Otherwise it would keep every send and add none.
pub fn waiting(state: &ChatState) -> bool {
    state
        .pending
        .sends
        .iter()
        .any(|entry| !answers_page_ids(&entry.text).is_empty())
        || state.questions.forms.values().any(|form| {
            !form.confirmed
                && form.held_for_owner.is_none()
                && (form.send.is_some() || (form.sent.is_some() && !form.sent_waiting_for_idle))
        })
}

fn awaiting_confirmation(form: &FormState) -> Option<(&serde_json::Value, &PendingSend)> {
    if form.confirmed {
        return None;
    }
    let answers = form
        .send
        .as_ref()
        .map(|send| &send.answers)
        .or(form.sent.as_ref())?;
    Some((answers, form.confirmation_boundary.as_ref()?))
}

/// The RPC can name the queue row before or after its answer reaches the transcript.
pub(crate) fn remember_current_receipt(form: &mut FormState) {
    if !form.confirmed {
        return;
    }
    let (Some(id), Some(answers), Some(message_id)) = (
        form.queued_prompt_id.as_ref(),
        form.send
            .as_ref()
            .map(|send| &send.answers)
            .or(form.sent.as_ref()),
        form.confirmed_message_ids.last(),
    ) else {
        return;
    };
    if !form
        .confirmed_queue_answers
        .iter()
        .any(|(known, value, row)| known == id && value == answers && row == message_id)
    {
        form.confirmed_queue_answers
            .push((id.clone(), answers.clone(), message_id.clone()));
    }
    if let Some((_, _, occurrence, _)) = form.queue_receipt_versions.get_mut(id) {
        *occurrence = None;
    }
}

/// Follow newer queue text and failed-to-queued retries, even when gxserver keeps the same id.
pub(crate) fn observe_queue_updates(state: &mut ChatState) {
    if state.questions.forms.is_empty() {
        return;
    }
    let has_server_queue = state.session.server_queue_prompts.is_some();
    let rows = state
        .session
        .server_queue_prompts
        .as_ref()
        .or(state.session.queue_prompts.as_ref());
    for row in rows.into_iter().flatten() {
        let (Some(id), Some(raw_text)) = (
            row.get("id").and_then(serde_json::Value::as_str),
            row.get("text").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        let auth_text;
        let text = if !has_server_queue {
            if let Some(authoritative) =
                crate::questions::inline_answers::authoritative_prompt_text(state, id)
            {
                auth_text = authoritative;
                auth_text.as_str()
            } else {
                raw_text
            }
        } else {
            raw_text
        };
        let (Some(status), Some(updated)) = (
            row.get("state").and_then(serde_json::Value::as_str),
            crate::session::startup_sends::parse_iso_ms(
                row.get("updatedAt").and_then(serde_json::Value::as_str),
            ),
        ) else {
            continue;
        };
        if !matches!(status, "failed" | "queued" | "sending") {
            continue;
        }
        let envelopes = split_all_page_answers(text).0;
        let present: HashSet<&str> = envelopes
            .iter()
            .map(|answer| answer.page_id.as_str())
            .collect();
        for (page_id, form) in state.questions.forms.iter_mut() {
            if !present.contains(page_id.as_str())
                && form
                    .queue_receipt_versions
                    .get(id)
                    .is_some_and(|(at, _, _, _)| updated >= *at)
            {
                form.queue_receipt_versions.remove(id);
            }
        }
        if envelopes.is_empty() {
            continue;
        }
        let normalized_text = crate::composer::skill_invocation::agent_skill_text(
            text,
            state.session.agent.as_deref(),
        )
        .to_string();
        for answer in envelopes {
            let Some(form) = state.questions.forms.get_mut(&answer.page_id) else {
                continue;
            };
            let previous = form.queue_receipt_versions.get(id);
            if previous.is_some_and(|(at, failed, _, prev_text)| {
                updated < *at
                    || (updated == *at
                        && *prev_text == normalized_text
                        && *failed == (status == "failed"))
            }) {
                continue;
            }
            let mut occurrence = previous.and_then(|(_, _, occurrence, _)| *occurrence);
            if previous.is_some_and(|(_, failed, _, _)| *failed) && status != "failed" {
                let mut old_rows = Vec::new();
                form.confirmed_queue_answers.retain(|(known, _, row)| {
                    if known != id {
                        return true;
                    }
                    old_rows.push(row.clone());
                    false
                });
                for row in old_rows {
                    if !form.confirmed_message_ids.contains(&row) {
                        form.confirmed_message_ids.push(row);
                    }
                }
                let used = form.confirmed_message_ids.len()
                    + form
                        .confirmed_queue_answers
                        .iter()
                        .filter(|(_, value, row)| {
                            *value == answer.answers && !form.confirmed_message_ids.contains(row)
                        })
                        .count();
                occurrence = Some(u32::try_from(used).unwrap_or(u32::MAX).saturating_add(1));
            }
            form.queue_receipt_versions.insert(
                id.to_string(),
                (
                    updated,
                    status == "failed",
                    occurrence,
                    normalized_text.clone(),
                ),
            );
        }
    }
}

/// Restore the latest receipt text and keep a Retry until its own transcript row arrives.
pub(crate) fn apply_queue_receipts(
    forms: &BTreeMap<String, FormState>,
    inline_updates: &[crate::questions::inline_answers::InlineQueueUpdate],
    live_queue: Option<&[serde_json::Value]>,
    pending: &mut [PendingSend],
) {
    if forms.is_empty() {
        return;
    }
    for entry in pending {
        let Some(id) = entry.queued_prompt_id.as_deref() else {
            continue;
        };
        let pages = answers_page_ids(&entry.text);
        let Some((_, _, occurrence, text)) = pages.iter().find_map(|page| {
            forms
                .get(page)
                .and_then(|form| form.queue_receipt_versions.get(id))
        }) else {
            continue;
        };
        let keep_live_text = entry.left_queue_at_ms.is_none()
            && (inline_updates.iter().any(|update| update.prompt_id == id)
                || live_queue.is_some_and(|queue| {
                    queue
                        .iter()
                        .any(|row| row.get("id").and_then(serde_json::Value::as_str) == Some(id))
                }));
        if !keep_live_text && entry.text != *text {
            entry.text.clone_from(text);
        }
        if let Some(occurrence) = occurrence {
            entry.matching_occurrence =
                Some(entry.matching_occurrence.unwrap_or(0).max(*occurrence));
        }
    }
}

fn can_confirm_queued_entry(entry: &PendingSend) -> bool {
    entry.left_queue_at_ms.is_some()
        || entry
            .startup_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.state == "sending")
}

fn needs_observation(state: &ChatState) -> bool {
    if state.questions.forms.is_empty() {
        return false;
    }
    let known_page = |text: &str| {
        answers_page_ids(text)
            .iter()
            .any(|id| state.questions.forms.contains_key(id))
    };
    state
        .questions
        .forms
        .values()
        .any(|form| awaiting_confirmation(form).is_some())
        || state
            .pending
            .sends
            .iter()
            .any(|entry| entry.queued_prompt_id.is_some() && known_page(&entry.text))
        || state.session.queue_prompts.iter().flatten().any(|entry| {
            entry
                .get("text")
                .and_then(serde_json::Value::as_str)
                .is_some_and(known_page)
        })
}

/// Match new sends against the indexed transcript before either pruning pass can remove them.
pub(crate) fn observe_loaded(state: &mut ChatState) {
    if !needs_observation(state) || state.messages.boundaried.is_empty() {
        return;
    }
    if state.questions.form_answer_index.key != state.messages.boundaried_key {
        let mut index = AnswerIndex::from_rows(&state.messages.boundaried);
        index.key = state.messages.boundaried_key.clone();
        *state.questions.form_answer_index = index;
    }
    if state.questions.form_answer_index.pages.is_empty() {
        return;
    }
    let mut pending = state.session.queue_prompts.as_ref().map_or_else(
        || state.pending.sends.clone(),
        |queue| {
            let mut pending = crate::session::startup_sends::pending_with_startup_sends(
                &state.pending.sends,
                queue,
                state.session.agent.as_deref(),
            );
            let active: HashSet<&str> = queue
                .iter()
                .filter_map(|row| row.get("id").and_then(serde_json::Value::as_str))
                .collect();
            for entry in &mut pending {
                if entry
                    .queued_prompt_id
                    .as_deref()
                    .is_some_and(|id| !active.contains(id))
                {
                    entry.left_queue_at_ms.get_or_insert(entry.sent_at_ms);
                }
            }
            pending
        },
    );
    apply_queue_receipts(
        &state.questions.forms,
        &state.questions.inline_answers.queue_updates,
        state.session.queue_prompts.as_deref(),
        &mut pending,
    );
    observe_index(
        &mut state.questions.forms,
        &state.questions.form_answer_index,
        &state.messages.boundaried,
        &pending,
    );
}

/// Match real transcript rows to the sends they confirm, including rows read by Resources.
pub(crate) fn observe_rows(
    forms: &mut BTreeMap<String, FormState>,
    transcript: &[ChatRow],
    pending: &[PendingSend],
) {
    if forms.is_empty() {
        return;
    }
    observe_index(
        forms,
        &AnswerIndex::from_rows(transcript),
        transcript,
        pending,
    );
}

fn observe_index(
    forms: &mut BTreeMap<String, FormState>,
    index: &AnswerIndex,
    transcript: &[ChatRow],
    pending: &[PendingSend],
) {
    if index.pages.is_empty() {
        return;
    }
    let mut queued: HashMap<String, HashMap<serde_json::Value, Vec<&PendingSend>>> = HashMap::new();
    for entry in pending
        .iter()
        .filter(|entry| entry.queued_prompt_id.is_some() && can_confirm_queued_entry(entry))
    {
        let prompt_id = entry.queued_prompt_id.as_deref().unwrap_or_default();
        let mut seen_in_entry: HashMap<(String, serde_json::Value), usize> = HashMap::new();
        for answer in split_all_page_answers(&entry.text).0 {
            let Some(form) = forms.get(&answer.page_id) else {
                continue;
            };
            // When the send RPC has named a different prompt id, do not credit this local echo to
            // `prompt_id`. When it is still in flight (`queued_prompt_id.is_none()`), keep duplicate
            // envelopes on `prompt_id` in `queued` so only the already-confirmed slot is skipped.
            let is_local_echo =
                entry.id.strip_prefix(FORM_ECHO_PREFIX) == Some(answer.page_id.as_str());
            if is_local_echo
                && form
                    .queued_prompt_id
                    .as_deref()
                    .is_some_and(|id| id != prompt_id)
            {
                continue;
            }
            let slot = {
                let count = seen_in_entry
                    .entry((answer.page_id.clone(), answer.answers.clone()))
                    .or_default();
                let current = *count;
                *count += 1;
                current
            };
            let local_answers = form
                .send
                .as_ref()
                .map(|send| &send.answers)
                .or(form.sent.as_ref());
            let extra_confirmed = usize::from(
                is_local_echo
                    && form.queued_prompt_id.is_none()
                    && form.confirmed
                    && local_answers == Some(&answer.answers),
            );
            let confirmed_count = form
                .confirmed_queue_answers
                .iter()
                .filter(|(id, value, _)| id == prompt_id && *value == answer.answers)
                .count()
                + extra_confirmed;
            if confirmed_count > slot {
                continue;
            }
            queued
                .entry(answer.page_id)
                .or_default()
                .entry(answer.answers)
                .or_default()
                .push(entry);
        }
    }
    // A row still waiting in the queue must not take an identical send's delivered answer.
    for entries in queued.values_mut().flat_map(|answers| answers.values_mut()) {
        entries.sort_by_key(|entry| {
            if entry.left_queue_at_ms.is_some()
                || entry
                    .startup_delivery
                    .as_ref()
                    .is_some_and(|delivery| delivery.state == "sending")
            {
                0
            } else {
                1
            }
        });
    }

    for (page_id, by_answer) in queued {
        let (Some(form), Some(indexed)) = (forms.get_mut(&page_id), index.pages.get(&page_id))
        else {
            continue;
        };
        let mut used_occurrences: HashSet<(String, usize)> = form
            .confirmed_message_ids
            .iter()
            .chain(form.confirmed_queue_answers.iter().map(|(_, _, row)| row))
            .cloned()
            .collect();
        for (answers, entries) in by_answer {
            let Some(rows) = indexed.get(&answers) else {
                continue;
            };
            let mut remaining_by_prompt: HashMap<&str, usize> = HashMap::new();
            for entry in &entries {
                let prompt_id = entry.queued_prompt_id.as_deref().unwrap_or_default();
                *remaining_by_prompt.entry(prompt_id).or_default() += 1;
            }
            for (at, occurrence) in rows {
                let message_id = &transcript[*at].id;
                let target = (message_id.clone(), *occurrence);
                if used_occurrences.contains(&target) {
                    continue;
                }
                let eligible = |entry: &&PendingSend| {
                    let prompt_id = entry.queued_prompt_id.as_deref().unwrap_or_default();
                    let remaining = remaining_by_prompt
                        .get(prompt_id)
                        .copied()
                        .unwrap_or_default();
                    remaining > 0 && index.is_after(transcript, *at, entry)
                };
                let picked = entries
                    .iter()
                    .find(|entry| {
                        let prompt_id = entry.queued_prompt_id.as_deref().unwrap_or_default();
                        let confirmed_on_row = form
                            .confirmed_queue_answers
                            .iter()
                            .any(|(id, _, (row_id, _))| id == prompt_id && row_id == message_id)
                            || (entry.id.strip_prefix(FORM_ECHO_PREFIX)
                                == Some(page_id.as_str())
                                && form.queued_prompt_id.is_none()
                                && form.confirmed
                                && form
                                    .confirmed_message_ids
                                    .last()
                                    .is_some_and(|(row_id, _)| row_id == message_id));
                        eligible(entry) && confirmed_on_row
                    })
                    .or_else(|| entries.iter().find(|entry| eligible(entry)));
                let Some(entry) = picked else {
                    continue;
                };
                let prompt_id = entry.queued_prompt_id.as_deref().unwrap_or_default();
                if let Some((_, _, occurrence, _)) =
                    form.queue_receipt_versions.get_mut(prompt_id)
                {
                    *occurrence = None;
                }
                // Confirm the local form only when the matched row belongs to this form's own send
                // (its receipt or paired local echo); another client's row reserves its occurrences
                // without closing an unrelated local send still waiting on RPC or queue.
                let is_local_echo = entry.id.strip_prefix(FORM_ECHO_PREFIX) == Some(page_id.as_str());
                let local_answers = form
                    .send
                    .as_ref()
                    .map(|send| &send.answers)
                    .or(form.sent.as_ref());
                let belongs_to_local_form = local_answers == Some(&answers)
                    && (form.queued_prompt_id.as_deref() == Some(prompt_id)
                        || (form.queued_prompt_id.is_none() && is_local_echo));
                if !form.confirmed
                    && belongs_to_local_form
                    && form
                        .confirmation_boundary
                        .as_ref()
                        .is_some_and(|boundary| index.is_after(transcript, *at, boundary))
                {
                    form.sent = Some(answers.clone());
                    form.confirmed = true;
                    form.confirmed_message_ids.push(target.clone());
                }
                form.confirmed_queue_answers.push((
                    prompt_id.to_string(),
                    answers.clone(),
                    target.clone(),
                ));
                used_occurrences.insert(target);
                if let Some(left) = remaining_by_prompt.get_mut(prompt_id) {
                    *left -= 1;
                }
            }
        }
    }

    for (page_id, form) in forms.iter_mut() {
        if pending.iter().any(|entry| {
            entry.queued_prompt_id.is_some()
                && !can_confirm_queued_entry(entry)
                && (form.queued_prompt_id.as_deref() == entry.queued_prompt_id.as_deref()
                    || entry.id.strip_prefix(FORM_ECHO_PREFIX) == Some(page_id.as_str()))
        }) {
            continue;
        }
        let Some((answers, boundary)) = awaiting_confirmation(form) else {
            continue;
        };
        let confirmed_target = index.pages.get(page_id).and_then(|by_answer| {
            by_answer.get(answers)?.iter().find_map(|(at, occurrence)| {
                if !index.is_after(transcript, *at, boundary) {
                    return None;
                }
                let target = (transcript[*at].id.clone(), *occurrence);
                if form.confirmed_message_ids.contains(&target)
                    || form
                        .confirmed_queue_answers
                        .iter()
                        .any(|(_, _, row)| row == &target)
                {
                    return None;
                }
                Some(target)
            })
        });
        if let Some(target) = confirmed_target {
            form.confirmed = true;
            form.confirmed_message_ids.push(target);
            remember_current_receipt(form);
        }
    }
}

/// Keeps the answer echoes in step: one for each form send in flight, none for answers the
/// transcript already carries.
pub fn settle(state: &mut ChatState, context: &ChatContext, transcript: &[ChatRow]) {
    let confirmed: Vec<String> = page_answers_in(crate::state::messages_of(transcript))
        .into_iter()
        .map(|answers| answers.page_id)
        .collect();
    // gxserver's index saw the answers in the transcript, even when the loaded rows no longer hold
    // them or the file has since lost them. Only a row that left counts: a failed row's Retry keeps
    // its card whatever an earlier send of the same page did.
    let indexed: Vec<&str> = state
        .session
        .resource_index
        .iter()
        .filter(|entry| entry.get("status").and_then(serde_json::Value::as_str) == Some("answered"))
        .filter_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
        .collect();
    let mut untracked_index: Option<AnswerIndex> = None;
    if !state.questions.untracked_indexed_claims.is_empty() {
        let index = untracked_index.get_or_insert_with(|| AnswerIndex::from_rows(transcript));
        let mut resolved_pages = Vec::new();
        for (page_id, claim) in &state.questions.untracked_indexed_claims {
            let existing_claims = state
                .questions
                .untracked_confirmed_queue_answers
                .get(page_id);
            let matched = index
                .pages
                .get(page_id)
                .and_then(|by_value| by_value.get(&claim.answers))
                .and_then(|occurrences| {
                    occurrences.iter().find_map(|(at, occurrence)| {
                        if !index.is_after(transcript, *at, &claim.boundary) {
                            return None;
                        }
                        let target = (transcript[*at].id.clone(), *occurrence);
                        let taken = existing_claims.is_some_and(|claims| {
                            claims.iter().any(|(_, _, row)| row == &target)
                        });
                        (!taken).then_some(target)
                    })
                });
            if let Some(target) = matched {
                state
                    .questions
                    .untracked_confirmed_queue_answers
                    .entry(page_id.clone())
                    .or_default()
                    .push((claim.prompt_id.clone(), claim.answers.clone(), target));
                resolved_pages.push(page_id.clone());
            }
        }
        for page_id in resolved_pages {
            state.questions.untracked_indexed_claims.remove(&page_id);
        }
    }
    let mut sends = std::mem::take(&mut state.pending.sends);
    sends.retain(|entry| {
        let answers = split_all_page_answers(&entry.text).0;
        if answers.is_empty() {
            return true;
        }
        let has_known_queue_prompt = entry.queued_prompt_id.is_some();
        if has_known_queue_prompt && entry.left_queue_at_ms.is_none() {
            return true;
        }
        let mut all_confirmed = true;
        let mut sibling_unconfirmed = false;
        let mut seen_in_entry: HashMap<(String, serde_json::Value), usize> = HashMap::new();
        let mut entry_untracked: Vec<(String, serde_json::Value, (String, usize))> = Vec::new();
        let mut entry_indexed: Vec<(String, serde_json::Value)> = Vec::new();
        let prompt_id = entry.queued_prompt_id.as_deref().unwrap_or_default();
        for answer in &answers {
            let page_id = &answer.page_id;
            let slot = {
                let count = seen_in_entry
                    .entry((page_id.clone(), answer.answers.clone()))
                    .or_default();
                let current = *count;
                *count += 1;
                current
            };
            let same_confirmed_queue_send =
                entry.queued_prompt_id.as_deref().is_some_and(|prompt_id| {
                    state.questions.forms.get(page_id).is_some_and(|form| {
                        form.confirmed_queue_answers
                            .iter()
                            .filter(|(id, answers, _)| id == prompt_id && *answers == answer.answers)
                            .count()
                            > slot
                    })
                });
            // An old answer for the same page is not evidence for a later send or retry.
            let current_form = state.questions.forms.get(page_id).filter(|form| {
                form.confirmation_boundary.is_some()
                    && form
                        .send
                        .as_ref()
                        .map(|send| &send.answers)
                        .or(form.sent.as_ref())
                        == Some(&answer.answers)
            });
            let confirmed_local_send = slot == 0
                && entry.id.strip_prefix(FORM_ECHO_PREFIX) == Some(page_id.as_str())
                && current_form.is_some_and(|form| {
                    form.confirmed
                        && form.queued_prompt_id == entry.queued_prompt_id
                        && !entry.queued_prompt_id.as_deref().is_some_and(|id| {
                            form.queue_receipt_versions
                                .get(id)
                                .is_some_and(|(_, _, occurrence, _)| occurrence.is_some())
                        })
                });
            let confirmed_local_send = entry.left_queue_at_ms.is_some()
                && (same_confirmed_queue_send || confirmed_local_send);
            let untracked_queued_confirmed = if has_known_queue_prompt
                && entry.left_queue_at_ms.is_some()
                && !state.questions.forms.contains_key(page_id)
            {
                let claims_for_page = state
                    .questions
                    .untracked_confirmed_queue_answers
                    .get(page_id);
                let prior_claimed = claims_for_page.map_or(0, |claims| {
                    claims
                        .iter()
                        .filter(|(id, value, _)| id == prompt_id && *value == answer.answers)
                        .count()
                });
                let batch_claimed = entry_untracked
                    .iter()
                    .filter(|(p, value, _)| p == page_id && *value == answer.answers)
                    .count();
                if prior_claimed + batch_claimed > slot {
                    true
                } else {
                    let index = untracked_index
                        .get_or_insert_with(|| AnswerIndex::from_rows(transcript));
                    let matched_target = index
                        .pages
                        .get(page_id)
                        .and_then(|by_value| by_value.get(&answer.answers))
                        .and_then(|occurrences| {
                            occurrences.iter().find_map(|(at, occurrence)| {
                                if !index.is_after(transcript, *at, entry) {
                                    return None;
                                }
                                let target = (transcript[*at].id.clone(), *occurrence);
                                let already_taken = claims_for_page.is_some_and(|claims| {
                                    claims.iter().any(|(_, _, row)| row == &target)
                                }) || entry_untracked.iter().any(
                                    |(p, _, row)| p == page_id && row == &target,
                                );
                                (!already_taken).then_some(target)
                            })
                        });
                    if let Some(target) = matched_target {
                        entry_untracked.push((page_id.clone(), answer.answers.clone(), target));
                        true
                    } else if indexed.contains(&page_id.as_str())
                        && !index.holds(page_id)
                        && claims_for_page.is_none_or(Vec::is_empty)
                        && entry_untracked.iter().all(|(p, _, _)| p != page_id)
                        && state
                            .questions
                            .untracked_indexed_claims
                            .get(page_id)
                            .is_none_or(|claim| claim.prompt_id == prompt_id)
                        && !entry_indexed.iter().any(|(p, _)| p == page_id)
                    {
                        entry_indexed.push((page_id.clone(), answer.answers.clone()));
                        true
                    } else {
                        false
                    }
                }
            } else {
                false
            };
            let answer_confirmed = confirmed_local_send
                || untracked_queued_confirmed
                || (!has_known_queue_prompt
                    && confirmed.contains(page_id)
                    && current_form.is_none_or(|form| form.confirmed))
                || (!has_known_queue_prompt
                    && entry.left_queue_at_ms.is_some()
                    && indexed.contains(&page_id.as_str())
                    && current_form.is_none_or(|form| form.confirmed));
            if !answer_confirmed {
                all_confirmed = false;
                if entry
                    .id
                    .strip_prefix(FORM_ECHO_PREFIX)
                    .is_some_and(|primary| primary != page_id.as_str())
                    && entry.queued_prompt_id.is_none()
                    && state
                        .questions
                        .forms
                        .get(page_id)
                        .is_none_or(|sibling| sibling.held_for_owner.is_none())
                {
                    sibling_unconfirmed = true;
                }
            }
        }
        if !prompt_id.is_empty() {
            for (page_id, answers_value, target) in entry_untracked {
                let claims = state
                    .questions
                    .untracked_confirmed_queue_answers
                    .entry(page_id)
                    .or_default();
                if !claims
                    .iter()
                    .any(|(id, val, row)| id == prompt_id && val == &answers_value && row == &target)
                {
                    claims.push((prompt_id.to_string(), answers_value, target));
                }
            }
            for (page_id, answers_value) in entry_indexed {
                state
                    .questions
                    .untracked_indexed_claims
                    .entry(page_id)
                    .or_insert_with(|| crate::state::UntrackedIndexedClaim {
                        prompt_id: prompt_id.to_string(),
                        answers: answers_value,
                        boundary: entry.clone(),
                    });
            }
        }
        if all_confirmed {
            return false;
        }
        // A form's echo lives through an in-flight send and, for normal sends, until the
        // transcript records it. A queued send instead becomes its queue row.
        match entry.id.strip_prefix(FORM_ECHO_PREFIX) {
            Some(form) if entry.queued_prompt_id.is_none() => {
                let primary_active = state.questions.forms.get(form).is_some_and(|form| {
                    form.held_for_owner.is_none()
                        && (form.send.is_some() || (!form.confirmed && form.sent.is_some()))
                });
                primary_active || sibling_unconfirmed
            }
            _ => true,
        }
    });
    for (id, form_state) in &state.questions.forms {
        if form_state.confirmed || form_state.held_for_owner.is_some() {
            continue;
        }
        let answers = if let Some(send) = form_state.send.as_ref() {
            &send.answers
        } else if !form_state.sent_waiting_for_idle {
            let Some(sent) = form_state.sent.as_ref() else {
                continue;
            };
            sent
        } else {
            continue;
        };
        let Some(form) = state
            .transcript_view
            .question_forms
            .iter()
            .chain(crate::questions::resource_modal::fetched_form(state))
            .find(|form| &form.id == id)
        else {
            continue;
        };
        let text = form_state
            .send
            .as_ref()
            .and_then(|send| send.params.get("text").and_then(serde_json::Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| answers_message(form, answers));
        let key = normalize_pending_text(&text);
        // While gxserver is away the echo says it is reconnecting (`questions/unreachable_retry.rs`).
        let delivery = if form_state
            .send
            .as_ref()
            .is_some_and(|send| send.retry.reconnecting)
        {
            "reconnecting"
        } else {
            "sending"
        };
        if let Some(entry) = sends.iter_mut().find(|entry| {
            normalize_pending_text(&entry.text) == key
                || split_all_page_answers(&entry.text)
                    .0
                    .iter()
                    .any(|found| found.page_id == *id && found.answers == *answers)
        }) {
            if entry.id.starts_with(FORM_ECHO_PREFIX) {
                if let Some(echo) = entry.startup_delivery.as_mut() {
                    echo.state = delivery.to_string();
                }
            }
            continue;
        }
        sends.push(PendingSend {
            id: format!("{FORM_ECHO_PREFIX}{id}"),
            startup_delivery: Some(StartupDelivery {
                prompt_id: String::new(),
                state: delivery.to_string(),
                error_message: None,
                wait_reason: None,
            }),
            text,
            sent_at_ms: context.now_millis(),
            ..form_state.confirmation_boundary.clone().unwrap_or_default()
        });
    }
    state.pending.sends = sends;
}

#[cfg(test)]
mod tests {
    use crate::session::startup_sends::pending_with_startup_sends;
    use serde_json::json;

    const ANSWERS: &str = "<ghostex-page-answers page=\"0123456789abcdef0123456789abcdef\" title=\"Mockups\">\n{\n  \"Which layout?\": \"tabs\"\n}\n</ghostex-page-answers>";

    #[test]
    fn queued_answers_become_echoes_with_their_delivery_state() {
        let queue = vec![
            json!({"id": "a", "text": ANSWERS, "state": "queued", "createdAt": "2026-10-14T01:00:00.000Z"}),
            json!({"id": "b", "text": "an ordinary queued prompt", "state": "queued"}),
        ];
        let echoes = pending_with_startup_sends(&[], &queue, Some("claude"));
        assert_eq!(
            echoes.len(),
            1,
            "an ordinary queued prompt stays in the strip"
        );
        assert_eq!(echoes[0].queued_prompt_id.as_deref(), Some("a"));
        assert_eq!(echoes[0].startup_delivery.as_ref().unwrap().state, "queued");
    }

    #[test]
    fn a_waiting_row_carries_why_it_waits() {
        let reason = "Waiting: the agent's input box can't be read yet, trying again.";
        let queue = vec![json!({
            "id": "a", "text": ANSWERS, "state": "queued", "waitReason": reason
        })];
        let echoes = pending_with_startup_sends(&[], &queue, Some("claude"));
        let delivery = echoes[0].startup_delivery.as_ref().unwrap();
        assert_eq!(delivery.wait_reason.as_deref(), Some(reason));
        assert_eq!(
            serde_json::to_value(delivery).unwrap()["waitReason"],
            json!(reason)
        );
        // The reason moves on with the ladder, and leaves when gxserver drops it.
        let next = vec![json!({"id": "a", "text": ANSWERS, "state": "queued"})];
        let echoes = pending_with_startup_sends(&echoes, &next, Some("claude"));
        assert_eq!(
            echoes[0].startup_delivery.as_ref().unwrap().wait_reason,
            None
        );
        // The queue strip's row keeps it too.
        let row: crate::document::QueuedPrompt = serde_json::from_value(
            json!({"id": "b", "text": "x", "state": "queued", "createdAt": "", "updatedAt": "", "waitReason": reason}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&row).unwrap()["waitReason"],
            json!(reason)
        );
    }

    #[test]
    fn a_failed_delivery_keeps_its_error_for_retry() {
        let queue = vec![json!({
            "id": "a", "text": ANSWERS, "state": "failed", "errorMessage": "The agent is not running."
        })];
        let echoes = pending_with_startup_sends(&[], &queue, Some("claude"));
        let delivery = echoes[0].startup_delivery.as_ref().unwrap();
        assert_eq!(delivery.state, "failed");
        assert_eq!(
            delivery.error_message.as_deref(),
            Some("The agent is not running.")
        );
        assert!(
            pending_with_startup_sends(&echoes, &[], Some("claude")).is_empty(),
            "a failed row the reader removed takes its card with it"
        );
    }
}
