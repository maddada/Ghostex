//! The stored records the composer reads and writes, in the exact shape the TypeScript wrote them.
//!
//! CDXC:Drafts 2026-09-22 DECISION:
//! User: drafts and queued prompts must survive the switch to the Rust brain. Every record below
//! keeps the key prefix, the field names and the legacy-value tolerance of
//! `packages/core-ui/chat/session-chat-draft-storage.ts`, `session-chat-summary-override.ts` and
//! `session-chat-verbose-override.ts`, so a draft written by the TypeScript brain is read back
//! unchanged by this one.
//!
//! The stores and their prefixes:
//!
//! | store | key | value |
//! |---|---|---|
//! | `drafts` | `ghostex.sessionChat.draft.<sessionKey>` | [`StoredDraftRecord`] as JSON, or legacy raw text |
//! | `summary` | `ghostex.sessionChat.summary.<sessionKey>` | `"1"` or `"0"` |
//! | `verbose` | `ghostex.sessionChat.verbose.<sessionKey>` | `"1"`, `"0"`, or absent |
//! | `returnedPrompts` | `ghostex.sessionChat.returnedPrompts.applied` | family a's applied-id list |
//!
//! The recovery checkpoints (`ghostex.sessionChat.recovery.`), the save outbox
//! (`ghostex.sessionChat.outbox.`), the sent history (`ghostex.sessionChat.sent.`) and the delivery
//! receipts (`ghostex.sessionChat.delivered.`) are read and written by the desktop plumbing, not by
//! the brain: they are listed in the report, and the host keeps owning them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::composer::queue::DraftVersion;
use crate::jsnum::js_safe_integer;

/// The store ids the composer uses, as `StorageKey::store`.
pub const DRAFTS_STORE: &str = "drafts";
/// The per-session summary-mode flag.
pub const SUMMARY_STORE: &str = "summary";
/// The per-session verbose override.
pub const VERBOSE_STORE: &str = "verbose";
/// `composer('submitted', {text, version})`: clear the stored draft when it still holds the
/// submitted revision, record the send in the host's sent history, and flush.
///
/// A store rather than a plain write on `drafts` because the clear is CONDITIONAL on what is on
/// disk and because the sent history is a store the host owns alone
/// (`docs/2026-09-21/rust-chat/HOST-TODO.md` section 3). The value carries `{text, version}`.
pub const DRAFT_SUBMITTED_STORE: &str = "draftSubmitted";
/// `composer('park', {text, version})`: the draft was handed to the terminal, so it is kept under
/// its revision and marked parked. The value carries `{text, version}`.
pub const DRAFT_PARK_STORE: &str = "draftPark";
/// `composer('history')`: what this machine's composers have sent, oldest first.
///
/// A store of its own rather than a record, because the host answers it with a SCAN rather than a
/// key: `listSentSessionChatMessages().map(message => message.content).reverse()`
/// (`native-composer.ts:121`), which is the last 50 prompts across every session, newest-first
/// sorted and then reversed. The `sentHistory` store itself stays the host's, written by
/// [`DRAFT_SUBMITTED_STORE`]; the core only ever reads this projection of it, lazily, on the first
/// Up arrow.
///
/// CDXC:SavedPrompts 2026-09-08 DECISION:
/// User: retain the last 50 sent messages across all sessions for Up-arrow recall and the Saved
/// prompts Sent tab.
pub const COMPOSER_HISTORY_STORE: &str = "composerHistory";

/// `composer('receive', {text, version, current})`: a draft arrived from another client. The host
/// writes the recovery checkpoint and flushes the save outbox; the DISPOSITION is decided in the
/// core, because `classifyDraftHandoff` is a pure rule over state the core already holds.
pub const DRAFT_RECEIVE_STORE: &str = "draftReceive";

/// One stored composer draft.
///
/// The field names are the TypeScript's `DecodedStoredDraft`, and `updatedAt` stays a number of
/// epoch milliseconds. `submitted` and `parked` are written as real booleans, which is what
/// `entry.submitted === true` reads back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredDraftRecord {
    pub text: String,
    /// `None` for a legacy plain-string draft, which callers must treat as "age unknown".
    ///
    /// A double, not an `i64`: the stamp is compared with `>=` against `Date.parse(...)` in
    /// `session-chat-draft-storage.ts`, and `value as i64` truncated a fractional stamp and
    /// saturated anything past `i64::MAX`, which is a different answer from the one JavaScript
    /// gives. `js_optional_number` writes it back as `JSON.stringify` writes a number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::jsnum::js_optional_number"
    )]
    pub updated_at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<DraftVersion>,
    #[serde(default)]
    pub submitted: bool,
    #[serde(default)]
    pub parked: bool,
}

/// Decodes a stored draft, falling back to the legacy raw-text form.
///
/// A record is only JSON when it has a string `text` and a numeric `updatedAt`, and a version is
/// only kept when its `draftId` is a string and its `revision` is a positive safe integer: the same
/// three tests `decodeStoredDraft` applies, so a half-written record degrades the same way.
pub fn decode_stored_draft(raw: &str) -> StoredDraftRecord {
    if let Ok(Value::Object(parsed)) = serde_json::from_str::<Value>(raw) {
        let text = parsed.get("text").and_then(Value::as_str);
        let updated_at = parsed.get("updatedAt").and_then(Value::as_f64);
        if let (Some(text), Some(updated_at)) = (text, updated_at) {
            // `Number.isSafeInteger(version.revision) && version.revision > 0`, which is what
            // `js_safe_integer` is, so a `3.0` token reads as 3 rather than being thrown away.
            let version = parsed.get("version").and_then(|version| {
                let draft_id = version.get("draftId")?.as_str()?;
                let revision = js_safe_integer(version.get("revision"))?;
                (revision > 0).then(|| DraftVersion {
                    draft_id: draft_id.to_string(),
                    revision,
                })
            });
            return StoredDraftRecord {
                text: text.to_string(),
                updated_at: Some(updated_at),
                version,
                submitted: parsed.get("submitted") == Some(&Value::Bool(true)),
                parked: parsed.get("parked") == Some(&Value::Bool(true)),
            };
        }
    }
    // Legacy drafts are the raw composer text, not JSON.
    StoredDraftRecord {
        text: raw.to_string(),
        updated_at: None,
        version: None,
        submitted: false,
        parked: false,
    }
}

/// The record's JSON, as `writeStoredSessionChatDraft` writes it.
///
/// The key order is the TypeScript object literal's: `text`, `updatedAt`, `version`, `submitted`,
/// `parked`. `JSON.stringify` drops an `undefined` version, which is what the skip above does.
pub fn encode_stored_draft(record: &StoredDraftRecord) -> String {
    serde_json::to_string(record).unwrap_or_else(|_| "{}".to_string())
}

/// The text a stored draft puts in the composer: a parked draft opens empty.
pub fn stored_draft_text(record: Option<&StoredDraftRecord>) -> String {
    match record {
        Some(record) if !record.parked => record.text.clone(),
        _ => String::new(),
    }
}

/// `"1"` means summary mode is on; anything else, including a missing record, means off.
pub fn decode_summary(raw: Option<&str>) -> bool {
    raw == Some("1")
}

/// What the summary record is written as.
pub fn encode_summary(summary_mode: bool) -> &'static str {
    if summary_mode {
        "1"
    } else {
        "0"
    }
}

/// `"1"` and `"0"` pin the override; anything else follows the Ghostex setting.
pub fn decode_verbose(raw: Option<&str>) -> Option<bool> {
    match raw {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// What the verbose record is written as.
pub fn encode_verbose(verbose: bool) -> &'static str {
    if verbose {
        "1"
    } else {
        "0"
    }
}

/// `ghostex.sessionChat.summary.<sessionKey>`, as the store plus the suffix the host prefixes.
/// The client-storage store the applied returned-prompt ids live in.
pub const RETURNED_PROMPTS_STORE: &str = "returnedPrompts";

/// The applied returned-prompt ids, ONE record for the whole app.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `ghostex.sessionChat.returnedPrompts.applied` holds a JSON array of ids, not one record per id
/// (`session-chat-returned-prompt.ts`). The core was reading a per-id record, which the desktop
/// host refuses outright because that store is a singleton, so a prompt the agent handed back was
/// never claimed and never reached the composer.
pub fn returned_prompts_key() -> crate::event::StorageKey {
    crate::event::StorageKey {
        store: RETURNED_PROMPTS_STORE.to_string(),
        suffix: String::new(),
    }
}

/// `readAppliedIds`: the stored list, or empty when the record is missing or malformed.
pub fn decode_applied_returned_ids(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Value>>(value)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `markSessionChatReturnedPromptApplied`: the id moved to the end, the list cut to the last 32.
pub fn encode_applied_returned_ids(applied: &[String], id: &str) -> String {
    let mut next: Vec<&str> = applied
        .iter()
        .map(String::as_str)
        .filter(|value| *value != id)
        .collect();
    next.push(id);
    let limit = crate::session::constants::RETURNED_PROMPT_APPLIED_LIMIT;
    if next.len() > limit {
        next.drain(..next.len() - limit);
    }
    serde_json::to_string(&next).unwrap_or_else(|_| "[]".to_string())
}

/// The stored draft entry for this session.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// Every per-session record is keyed by the SESSION KEY the composer boot read hands back, not by
/// `<projectId>:<sessionId>`. On a remote chat the host spells that key
/// `remote-<machineId>:<projectId>:<sessionId>` (`broker.ts:113`), so building it from the project
/// and session alone wrote the draft under the local spelling and the two brains read different
/// records. Found by the desktop host agent on 2026-09-22.
pub fn draft_key(session_key: &str) -> crate::event::StorageKey {
    crate::event::StorageKey {
        store: DRAFTS_STORE.to_string(),
        suffix: session_key.to_string(),
    }
}

pub fn summary_key(session_key: &str) -> crate::event::StorageKey {
    crate::event::StorageKey {
        store: SUMMARY_STORE.to_string(),
        suffix: session_key.to_string(),
    }
}

/// `ghostex.sessionChat.verbose.<sessionKey>`.
pub fn verbose_key(session_key: &str) -> crate::event::StorageKey {
    crate::event::StorageKey {
        store: VERBOSE_STORE.to_string(),
        suffix: session_key.to_string(),
    }
}

/// The recall ring's read. Per session, because the host scopes the scan by the session it is
/// asked from, even though the rows themselves come from every composer.
pub fn composer_history_key(session_key: &str) -> crate::event::StorageKey {
    crate::event::StorageKey {
        store: COMPOSER_HISTORY_STORE.to_string(),
        suffix: session_key.to_string(),
    }
}

/// The answer to [`composer_history_key`]: a JSON array of strings, oldest last.
///
/// Anything else is an empty ring, which is what `await composer('history')` returning a non-array
/// would leave `entries` as.
pub fn decode_composer_history(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}
