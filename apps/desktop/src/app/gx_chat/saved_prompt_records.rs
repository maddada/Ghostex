//! The chat records Quick Access's Saved Prompts tab reads and writes: the Recovered view (local
//! drafts and their recovery checkpoints) and the Sent view (the sent history).
//!
//! CDXC:SavedPrompts 2026-09-25 WHY:
//! The Quick Access controller ran in the app runtime and called the TypeScript chat stores
//! directly (`listRecoveredSessionChatDrafts`, `deleteStoredSessionChatDraft`,
//! `dismissDraftRecovery`, `importDraftRecovery`, `reconcileSessionChatDraftsFromServer`,
//! `listSentSessionChatMessages`, `deleteSentSessionChatMessage`, `recordDeliveredSessionChatDrafts`).
//! The controller is Rust now (gx-core `quick_access`), and these records are the chat host's, so
//! the eight operations are ported here beside the host's own record code and on its storage
//! doors, key for key, and Quick Access reaches them through `QuickAccessStorage`. What is left out
//! is the TypeScript's once-per-page checkpoint thinning, a cleanup whose rows this listing folds
//! into one row per session anyway.
//!
//! SEE-ALSO: packages/core-ui/chat/session-chat-draft-storage.ts,
//! packages/core-ui/chat/session-chat-draft-recovery.ts,
//! packages/core-ui/chat/session-chat-sent-history.ts,
//! apps/desktop/src/app/quick_access/storage.rs (the caller).

use ghostex_gx_chat_core::StorageKey;
use ghostex_gx_chat_core::composer::storage::{
    StoredDraftRecord, decode_stored_draft, encode_stored_draft,
};
use serde_json::Value;

use super::host_records::{self, DraftVersion, PendingDraft, RecoveryCheckpoint};
use super::storage;

/// A local draft the Recovered view lists (`RecoveredSessionChatDraft`, the top-level row).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecoveredDraft {
    pub(crate) session_key: String,
    pub(crate) project_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) text: String,
    pub(crate) updated_at: i64,
}

fn key(store: &str, suffix: &str) -> StorageKey {
    StorageKey {
        store: store.to_string(),
        suffix: suffix.to_string(),
    }
}

/// `parseDraftSessionKey`: the last two `:` parts are the ids in every key shape.
fn parse_session_key(session_key: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = session_key.split(':').collect();
    let non_empty = |part: &str| (!part.is_empty()).then(|| part.to_string());
    if parts.len() < 2 {
        return (None, non_empty(session_key));
    }
    (
        non_empty(parts[parts.len() - 2]),
        non_empty(parts[parts.len() - 1]),
    )
}

/// `listRecoveredSessionChatDrafts()`: one row per session, the newest text first.
pub(crate) fn list_recovered_drafts(now_ms: i64) -> Vec<RecoveredDraft> {
    let mut recovered: Vec<(RecoveredDraft, bool)> = Vec::new();
    for (session_key, raw) in storage::scan("drafts", "", now_ms).unwrap_or_default() {
        let entry = decode_stored_draft(&raw);
        if entry.text.is_empty() || entry.submitted {
            continue;
        }
        let (project_id, session_id) = parse_session_key(&session_key);
        recovered.push((
            RecoveredDraft {
                project_id,
                session_id,
                text: entry.text,
                updated_at: entry.updated_at.map(|at| at as i64).unwrap_or(now_ms),
                session_key,
            },
            false,
        ));
    }
    for (_, raw) in storage::scan("recovery", "", now_ms).unwrap_or_default() {
        let Some(entry) = host_records::decode_recovery(&raw) else {
            continue;
        };
        let (project_id, session_id) = parse_session_key(&entry.session_key);
        recovered.push((
            RecoveredDraft {
                session_key: entry.session_key,
                project_id,
                session_id,
                text: entry.text,
                updated_at: entry.updated_at,
            },
            true,
        ));
    }
    recovered.sort_by(|left, right| right.0.updated_at.cmp(&left.0.updated_at));
    let mut rows: Vec<RecoveredDraft> = Vec::new();
    for (entry, _) in recovered {
        let text = entry.text.replace("\r\n", "\n");
        if text.trim().is_empty() {
            continue;
        }
        if !rows.iter().any(|row| row.session_key == entry.session_key) {
            rows.push(entry);
        }
    }
    rows
}

/// `dismissDraftRecovery(id)`: the checkpoint becomes a dismissal marker, then the session's
/// markers are folded into ranges.
pub(crate) fn dismiss_recovery(recovery_id: &str, now_ms: i64) {
    let record = key("recovery", recovery_id);
    let Some(raw) = storage::read(&record, now_ms).ok().flatten() else {
        return;
    };
    let Ok(entry) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    if !entry.get("text").is_some_and(Value::is_string) {
        return;
    }
    let session_key = entry["sessionKey"].as_str().unwrap_or("").to_string();
    // `draftRecoveryDismissalMarker(sessionKey, version)`.
    let marker = match (
        entry["version"]["draftId"].as_str(),
        entry["version"]["revision"].as_i64(),
    ) {
        (Some(draft_id), Some(revision)) => {
            serde_json::json!([session_key, draft_id, revision]).to_string()
        }
        _ => "null".to_string(),
    };
    if storage::write(&record, Some(&marker), now_ms).is_ok() {
        let _ = super::dismissals::compact(&session_key, now_ms);
    }
}

/// `deleteStoredSessionChatDraft(sessionKey)`: an empty draft is written (a durable tombstone the
/// outbox carries to gxserver) and every recovery checkpoint of the session is dismissed.
pub(crate) fn delete_stored_draft(session_key: &str, now_ms: i64) {
    let record = key("drafts", session_key);
    let previous = storage::read(&record, now_ms)
        .ok()
        .flatten()
        .map(|raw| decode_stored_draft(&raw));
    // `writeStoredSessionChatDraft(sessionKey, '')`.
    let updated_at = (now_ms as f64).max(
        previous
            .as_ref()
            .and_then(|entry| entry.updated_at)
            .unwrap_or(0.0)
            + 1.0,
    );
    let version = match previous.as_ref() {
        Some(entry) if !entry.submitted => entry.version.as_ref().map(|version| {
            ghostex_gx_chat_core::composer::queue::DraftVersion {
                draft_id: version.draft_id.clone(),
                revision: version.revision + 1,
            }
        }),
        _ => None,
    }
    .unwrap_or_else(|| ghostex_gx_chat_core::composer::queue::DraftVersion {
        draft_id: super::platform::uuid_v4(),
        revision: 1,
    });
    if let Some(previous) = previous
        .as_ref()
        .filter(|entry| !entry.text.is_empty() && !entry.submitted)
    {
        let _ = host_records::preserve_draft_revision(
            &RecoveryCheckpoint {
                session_key: session_key.to_string(),
                text: previous.text.clone(),
                updated_at: previous.updated_at.map(|at| at as i64).unwrap_or(now_ms),
                version: previous.version.as_ref().map(|version| DraftVersion {
                    draft_id: version.draft_id.clone(),
                    revision: version.revision,
                }),
                dismissed: None,
            },
            now_ms,
        );
    }
    let mut boot = super::boot::BootReads::default();
    let _ = host_records::queue_draft_save(
        &PendingDraft {
            client_id: Some(super::boot::client_id(now_ms, &mut boot)),
            session_key: session_key.to_string(),
            content: String::new(),
            version: DraftVersion {
                draft_id: version.draft_id.clone(),
                revision: version.revision,
            },
            updated_at: updated_at as i64,
        },
        now_ms,
    );
    let entry = StoredDraftRecord {
        text: String::new(),
        updated_at: Some(updated_at),
        version: Some(version),
        submitted: false,
        parked: false,
    };
    let _ = storage::write(&record, Some(&encode_stored_draft(&entry)), now_ms);
    for (suffix, _) in
        storage::scan("recovery", &format!("{session_key}:"), now_ms).unwrap_or_default()
    {
        dismiss_recovery(&suffix, now_ms);
    }
}

/// `Date.parse` of a daemon stamp (always RFC 3339).
fn parse_iso_millis(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|stamp| stamp.timestamp_millis())
}

fn wire_version(value: &Value) -> Option<DraftVersion> {
    let draft_id = value.get("draftId")?.as_str()?;
    let revision = value.get("revision")?.as_i64()?;
    Some(DraftVersion {
        draft_id: draft_id.to_string(),
        revision,
    })
}

/// `importDraftRecovery(drafts)`: each daemon checkpoint preserved locally, once.
pub(crate) fn import_recovery(drafts: &Value, now_ms: i64) {
    for draft in drafts.as_array().into_iter().flatten() {
        let session_key = format!(
            "{}:{}",
            draft["projectId"].as_str().unwrap_or(""),
            draft["sessionId"].as_str().unwrap_or("")
        );
        let updated_at = draft["updatedAt"]
            .as_str()
            .and_then(parse_iso_millis)
            .unwrap_or(0);
        let _ = host_records::preserve_draft_revision(
            &RecoveryCheckpoint {
                session_key,
                text: draft["content"].as_str().unwrap_or("").to_string(),
                updated_at,
                version: wire_version(&draft["version"]),
                dismissed: None,
            },
            now_ms,
        );
    }
}

/// `recordDeliveredSessionChatDrafts(deliveries)`: each delivery joins the sent history once.
pub(crate) fn record_delivered(deliveries: &Value, now_ms: i64) {
    for delivery in deliveries.as_array().into_iter().flatten() {
        let (Some(id), Some(project_id), Some(session_id)) = (
            delivery["id"].as_str(),
            delivery["projectId"].as_str(),
            delivery["sessionId"].as_str(),
        ) else {
            continue;
        };
        let _ = host_records::record_delivered_draft(
            id,
            project_id,
            session_id,
            delivery["text"].as_str().unwrap_or(""),
            delivery["deliveredAt"].as_str().unwrap_or(""),
            now_ms,
        );
    }
}

/// `reconcileSessionChatDraftsFromServer(drafts)`: the daemon's durable copy of each session's
/// draft heals a local cache that lost it, without bringing back text that was already sent.
pub(crate) fn reconcile_drafts(drafts: &Value, now_ms: i64) {
    for draft in drafts.as_array().into_iter().flatten() {
        record_delivered(&draft["deliveredDrafts"], now_ms);
        let Some(server_at) = draft["updatedAt"].as_str().and_then(parse_iso_millis) else {
            continue;
        };
        let session_key = format!(
            "{}:{}",
            draft["projectId"].as_str().unwrap_or(""),
            draft["sessionId"].as_str().unwrap_or("")
        );
        let consumed: Vec<DraftVersion> = draft["consumedDrafts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(wire_version)
            .collect();
        for receipt in &consumed {
            let _ = super::draft_ops::retire_recovery(&session_key, receipt, now_ms);
        }
        let record = key("drafts", &session_key);
        let stored = storage::read(&record, now_ms)
            .ok()
            .flatten()
            .map(|raw| decode_stored_draft(&raw));
        if let Some(recovered) = recover_draft(stored.as_ref(), draft, &consumed, server_at) {
            let _ = storage::write(&record, Some(&encode_stored_draft(&recovered)), now_ms);
        }
    }
}

/// `recoverSessionChatDraft(stored, incoming)`.
fn recover_draft(
    stored: Option<&StoredDraftRecord>,
    incoming: &Value,
    consumed: &[DraftVersion],
    incoming_at: i64,
) -> Option<StoredDraftRecord> {
    let content = incoming["content"].as_str().unwrap_or("");
    let parked = incoming["parked"].as_bool() == Some(true);
    let incoming_version = wire_version(&incoming["version"]);
    let consumes = |version: &DraftVersion| {
        consumed.iter().any(|receipt| {
            receipt.draft_id == version.draft_id && receipt.revision >= version.revision
        })
    };
    let as_core = |version: &DraftVersion| ghostex_gx_chat_core::composer::queue::DraftVersion {
        draft_id: version.draft_id.clone(),
        revision: version.revision,
    };
    let stored_version = stored
        .and_then(|entry| entry.version.as_ref())
        .map(|version| DraftVersion {
            draft_id: version.draft_id.clone(),
            revision: version.revision,
        });
    if let Some(stored_version) = stored_version.as_ref().filter(|version| consumes(version)) {
        let incoming_consumed = incoming_version
            .as_ref()
            .is_some_and(|version| consumes(version));
        if let Some(version) = incoming_version
            .as_ref()
            .filter(|_| !incoming_consumed && !content.is_empty())
        {
            return Some(StoredDraftRecord {
                text: content.to_string(),
                updated_at: Some(incoming_at as f64),
                version: Some(as_core(version)),
                submitted: false,
                parked,
            });
        }
        return Some(StoredDraftRecord {
            text: String::new(),
            updated_at: Some(incoming_at as f64),
            version: Some(as_core(stored_version)),
            submitted: true,
            parked: false,
        });
    }
    if let (Some(stored), Some(stored_version), Some(version)) =
        (stored, stored_version.as_ref(), incoming_version.as_ref())
    {
        if version.draft_id == stored_version.draft_id {
            if version.revision == stored_version.revision && stored.parked != parked {
                return Some(StoredDraftRecord {
                    parked,
                    ..stored.clone()
                });
            }
            return (version.revision > stored_version.revision).then(|| StoredDraftRecord {
                text: content.to_string(),
                updated_at: Some(incoming_at as f64),
                version: Some(as_core(version)),
                submitted: false,
                parked,
            });
        }
    }
    // Another draft's retirement says nothing about this client's unsent text.
    if stored_version.is_some() && stored.is_some_and(|entry| !entry.text.is_empty()) {
        return None;
    }
    if content.is_empty()
        || incoming_version
            .as_ref()
            .is_some_and(|version| consumes(version))
    {
        return None;
    }
    if let Some(stored) = stored {
        match stored.updated_at {
            None => return None,
            Some(at) if at >= incoming_at as f64 => return None,
            _ => {}
        }
    }
    Some(StoredDraftRecord {
        text: content.to_string(),
        updated_at: Some(incoming_at as f64),
        version: incoming_version.as_ref().map(as_core),
        submitted: false,
        parked,
    })
}

/// `listSentSessionChatMessages()`: the newest fifty, newest first, the rest pruned.
pub(crate) fn list_sent(now_ms: i64) -> Vec<Value> {
    let mut messages: Vec<(String, Value)> = storage::scan("sentHistory", "", now_ms)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(suffix, raw)| {
            serde_json::from_str::<Value>(&raw)
                .ok()
                .map(|value| (suffix, value))
        })
        .collect();
    let text = |value: &Value, field: &str| value[field].as_str().unwrap_or("").to_string();
    messages.sort_by(|left, right| {
        text(&right.1, "createdAt")
            .cmp(&text(&left.1, "createdAt"))
            .then_with(|| text(&right.1, "promptId").cmp(&text(&left.1, "promptId")))
    });
    for (suffix, _) in messages.iter().skip(host_records::MAX_SENT_MESSAGES) {
        let _ = storage::write(&key("sentHistory", suffix), None, now_ms);
    }
    messages
        .into_iter()
        .take(host_records::MAX_SENT_MESSAGES)
        .map(|(_, value)| value)
        .collect()
}

/// `deleteSentSessionChatMessage(promptId)`.
pub(crate) fn delete_sent(prompt_id: &str, now_ms: i64) {
    let _ = storage::write(&key("sentHistory", prompt_id), None, now_ms);
}
