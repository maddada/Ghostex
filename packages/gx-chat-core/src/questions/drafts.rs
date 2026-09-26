//! The answers the user typed but has not sent, and the questions already retired.
//!
//! Port of `packages/shared/session-chat-controller/question-drafts.ts` and the record formats
//! `async-question-storage.ts` keeps. The stored JSON is unchanged, because existing saved
//! answers must still load: drafts are one object of `{indices, other}` per question key, and
//! retired questions are one array of keys capped at the newest 1000.
//!
//! CDXC:SessionChat 2026-09-15 DECISION:
//! User: answer text in question cards must survive session switches, reusing the composer's
//! draft storage system. Save each edit through the same local storage index, scoped to the
//! session and question, and clear only after successful delivery or explicit dismissal.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::document::QuestionDraft;
use crate::event::StorageKey;

/// The saved answers of one card or one session's async questions, by question key.
pub type AnswerDrafts = BTreeMap<String, QuestionDraft>;

/// Retired async question keys are capped so a long session cannot grow the record without bound.
pub const RETIRED_QUESTION_LIMIT: usize = 1000;

/// The client-storage store the blocking card's and the async strip's drafts share.
pub const DRAFTS_STORE: &str = "questionDrafts";
/// The client-storage store the retired async question keys live in.
pub const RETIRED_STORE: &str = "retiredQuestions";
/// The client-storage store one dismissed notice per session lives in.
pub const NOTICES_STORE: &str = "notices";

/// The fixed prompt key the async strip's own drafts are stored under.
pub const ASYNC_PROMPT_KEY: &str = "async";

/// Where one card's drafts are stored.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// The host owns the `ghostex.sessionChat.questionDraft.` prefix and nothing else, so the session
/// key is part of the suffix the core builds: `questionDraftStorageKey` spells it
/// `JSON.stringify([sessionKey, promptKey])`. Leaving the session out made every chat on a machine
/// share one card record, which the replay could not see because its storage was an in-memory map
/// keyed by the same wrong string on both sides.
pub fn drafts_key(session_key: &str, prompt_key: &str) -> StorageKey {
    StorageKey {
        store: DRAFTS_STORE.to_string(),
        suffix: serde_json::to_string(&[session_key, prompt_key]).unwrap_or_default(),
    }
}

/// The async strip's own draft record, which is the card record under the fixed `async` key.
pub fn async_drafts_key(session_key: &str) -> StorageKey {
    drafts_key(session_key, ASYNC_PROMPT_KEY)
}

/// Whether this is the async strip's record rather than a blocking card's.
pub fn is_async_drafts_key(key: &StorageKey) -> bool {
    decode_drafts_suffix(&key.suffix).is_some_and(|(_, prompt)| prompt == ASYNC_PROMPT_KEY)
}

/// The `[sessionKey, promptKey]` pair a drafts suffix carries.
pub fn decode_drafts_suffix(suffix: &str) -> Option<(String, String)> {
    let parts: Vec<String> = serde_json::from_str(suffix).ok()?;
    match parts.as_slice() {
        [session, prompt] => Some((session.clone(), prompt.clone())),
        _ => None,
    }
}

/// The retired async question keys, one record per session.
///
/// `ghostex:async-questions:<sessionKey>` (`async-question-storage.ts`).
pub fn retired_key(session_key: &str) -> StorageKey {
    StorageKey {
        store: RETIRED_STORE.to_string(),
        suffix: session_key.to_string(),
    }
}

/// The dismissed notice, one record per session.
///
/// `ghostex.sessionChat.noticeDismissed.<sessionKey>` (`notice-state.ts:57`).
pub fn notice_key(session_key: &str) -> StorageKey {
    StorageKey {
        store: NOTICES_STORE.to_string(),
        suffix: session_key.to_string(),
    }
}

/// Parses a stored draft record. A record that fails validation reads as no drafts at all, the
/// same as `decodeDrafts` returning null.
pub fn decode_drafts(raw: &str) -> AnswerDrafts {
    let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(raw) else {
        return AnswerDrafts::new();
    };
    let mut drafts = AnswerDrafts::new();
    for (key, value) in entries {
        let Some(entry) = value.as_object() else {
            return AnswerDrafts::new();
        };
        let Some(other) = entry.get("other").and_then(Value::as_str) else {
            return AnswerDrafts::new();
        };
        let Some(raw_indices) = entry.get("indices").and_then(Value::as_array) else {
            return AnswerDrafts::new();
        };
        let mut indices = Vec::with_capacity(raw_indices.len());
        for index in raw_indices {
            match index.as_u64() {
                // `Number.isSafeInteger` is the bound the writer honoured; anything past it was
                // never a real option index.
                Some(value) if value <= u32::MAX as u64 => indices.push(value as u32),
                _ => return AnswerDrafts::new(),
            }
        }
        drafts.insert(
            key,
            QuestionDraft {
                indices,
                other: other.to_string(),
            },
        );
    }
    drafts
}

/// The stored form of a draft record, or `None` when the record should be deleted instead.
///
/// `writeQuestionDrafts` removes an empty record rather than storing `{}`, so an answered card
/// leaves nothing behind.
pub fn encode_drafts(drafts: &AnswerDrafts) -> Option<String> {
    if drafts.is_empty() {
        return None;
    }
    serde_json::to_string(drafts).ok()
}

/// What is left after the answers in `submitted` were delivered.
///
/// A key only drops when its saved answer is exactly the one that was sent; an answer edited
/// after the send stays.
pub fn remaining_drafts(current: &AnswerDrafts, submitted: &AnswerDrafts) -> AnswerDrafts {
    let mut remaining = current.clone();
    for (question, answer) in submitted {
        if remaining.get(question) == Some(answer) {
            remaining.remove(question);
        }
    }
    remaining
}

/// Parses the retired-question record, ignoring anything that is not a string.
pub fn decode_retired(raw: &str) -> Vec<String> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| match item {
                Value::String(value) => Some(value),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The retired-question record after `key` is added: first-seen order, newest 1000 kept.
pub fn encode_retired(retired: &[String], key: &str) -> String {
    let mut ordered: Vec<&str> = Vec::with_capacity(retired.len() + 1);
    for entry in retired
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(key))
    {
        if !ordered.contains(&entry) {
            ordered.push(entry);
        }
    }
    let start = ordered.len().saturating_sub(RETIRED_QUESTION_LIMIT);
    serde_json::to_string(&ordered[start..]).unwrap_or_else(|_| "[]".to_string())
}
