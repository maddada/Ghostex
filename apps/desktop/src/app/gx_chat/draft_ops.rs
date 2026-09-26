//! The three draft operations the core names as stores but the host performs.
//!
//! `packages/gx-chat-core/src/composer/storage.rs` gives `composer('submitted')`,
//! `composer('park')` and `composer('receive')` store ids of their own (`draftSubmitted`,
//! `draftPark`, `draftReceive`) and sends them as an [`Effect::WriteStorage`] each, because their
//! effect is CONDITIONAL on what is already on disk and because three of the records they touch
//! (the recovery checkpoints, the save outbox and the sent history) are the host's alone
//! (`docs/2026-09-21/rust-chat/HOST-TODO.md` section 3). They are not rows: `storage::write` would
//! answer `unregistered` for every one of them, which the core reads as a refused submission and
//! puts the text back in the composer.
//!
//! The bodies are ports of `nativeComposerRequest`'s three arms in the deleted
//! `apps/desktop/sidebar/session-chat-runtime/native-composer.ts`, in their order.

use ghostex_gx_chat_core::StorageKey;
use ghostex_gx_chat_core::composer::storage::{
    StoredDraftRecord, decode_stored_draft, encode_stored_draft,
};
use serde_json::{Value, json};

use super::host_records::{self, DraftVersion, RecoveryCheckpoint};
use super::locale::iso_from_millis;
use super::outbox;
use super::storage;

/// What `composer('park')` hands back, which the host owes the view when the send finishes.
#[derive(Clone, Debug)]
pub(super) struct ParkResult {
    /// `crypto.randomUUID()`: the transfer's identity, which the terminal acknowledges by.
    pub(super) handoff_id: String,
    pub(super) content: String,
    pub(super) draft_version: Value,
}

/// `composer('submitted', {text, version})`.
///
/// The stored draft is cleared only when it still holds exactly the submitted revision, the outbox
/// row and the recovery checkpoints of that revision are retired, and the prompt joins the sent
/// history. A send that raced an edit leaves the newer text alone.
///
/// `refusals` counts the one write in here that is allowed to fail: see the sent-history call.
pub(super) fn submitted(
    session_key: &str,
    value: &Value,
    now_ms: i64,
    refusals: &mut u64,
) -> Result<(), &'static str> {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = version(value);
    if let Some(version) = version.as_ref() {
        retire_recovery(session_key, version, now_ms)?;
        outbox::acknowledge(session_key, version, now_ms)?;
    }
    if let Some(current) = stored(session_key, now_ms)?
        && current.text == text
        && matches(&current, version.as_ref())
    {
        let cleared = StoredDraftRecord {
            text: String::new(),
            updated_at: Some(now_ms as f64),
            version: current.version.clone(),
            submitted: true,
            parked: false,
        };
        write_draft(session_key, &cleared, now_ms)?;
    }
    // `recordSentSessionChatMessage(text, sessionKey)`, the one call site the Step 4 host was
    // missing: without it Up-arrow recall and the Saved prompts Sent tab stayed empty under the Rust
    // brain while the QuickJS brain kept filling them.
    //
    // CDXC:SavedPrompts 2026-09-22 WHY:
    // Its refusal is COUNTED, never returned. `recordSentSessionChatMessage` catches its own write
    // failure and answers false, with the reason written next to the catch: "Delivery has already
    // succeeded; a history write must not restore and resend the prompt." Returned from here it
    // became a refused `composer('submitted')`, which the core reads as a failed submission and
    // answers by putting the text back in the composer, so a full or refusing `sentHistory` store
    // would have offered the user a prompt gxserver had already taken and invited them to send it
    // twice. Everything above this line still propagates: those are the stored draft, the outbox
    // row and the recovery checkpoints, and the TypeScript arm let each of them throw.
    if host_records::record_sent_prompt(
        text,
        Some(session_key),
        None,
        &iso_from_millis(now_ms),
        now_ms,
    )
    .is_err()
    {
        *refusals += 1;
    }
    Ok(())
}

/// `composer('park', {text, version})`: the draft was handed to the terminal.
///
/// It refuses rather than parking when the stored draft moved, because the text the terminal is
/// about to receive would then not be the text this composer is clearing. The message is the
/// TypeScript's, word for word, and reaches the user as the handoff failure.
pub(super) fn park(
    session_key: &str,
    value: &Value,
    now_ms: i64,
) -> Result<ParkResult, &'static str> {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = version(value);
    let current = stored(session_key, now_ms)?;
    let Some(current) =
        current.filter(|entry| entry.text == text && matches(entry, version.as_ref()))
    else {
        return Err("The draft changed during transfer. It has been kept in Chat.");
    };
    let updated_at = current.updated_at.unwrap_or(now_ms as f64);
    host_records::preserve_draft_revision(
        &RecoveryCheckpoint {
            session_key: session_key.to_string(),
            text: current.text.clone(),
            updated_at: updated_at as i64,
            version: host_version(current.version.as_ref()),
            dismissed: None,
        },
        now_ms,
    )?;
    let parked = StoredDraftRecord {
        text: current.text.clone(),
        updated_at: Some(updated_at),
        version: current.version.clone(),
        submitted: false,
        parked: true,
    };
    write_draft(session_key, &parked, now_ms)?;
    Ok(ParkResult {
        handoff_id: super::platform::uuid_v4(),
        content: current.text,
        draft_version: current
            .version
            .as_ref()
            .map(|version| json!({"draftId": version.draft_id, "revision": version.revision}))
            .unwrap_or(Value::Null),
    })
}

/// `composer('receive', {text, version, current})`: a draft arrived from another client.
///
/// The disposition and the stored write are the core's (`classifyDraftHandoff` is a pure rule over
/// state it already holds). What is left here is the one thing the host owes on every disposition:
/// the incoming revision is checkpointed before anything can overwrite the local text.
pub(super) fn receive(session_key: &str, value: &Value, now_ms: i64) -> Result<(), &'static str> {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    host_records::preserve_draft_revision(
        &RecoveryCheckpoint {
            session_key: session_key.to_string(),
            text: text.to_string(),
            updated_at: now_ms,
            version: version(value),
            dismissed: None,
        },
        now_ms,
    )
}

/// `retireDraftRecovery(sessionKey, [version])`: every checkpoint of that draft at or below the
/// receipt becomes a dismissal marker.
///
/// The marker is `JSON.stringify([sessionKey, draftId, revision])` written OVER the checkpoint,
/// which is what `draftRecoveryDismissalMarker` writes and what `decode_recovery` already refuses
/// to read back as a checkpoint. `dismissDraftRecovery` then compacts, and so does this: the
/// markers become `recoveryDismissed` ranges and their records are removed
/// (`super::dismissals`).
pub(super) fn retire_recovery(
    session_key: &str,
    version: &DraftVersion,
    now_ms: i64,
) -> Result<(), &'static str> {
    let prefix = format!("{session_key}:{}:", version.draft_id);
    for (suffix, raw) in storage::scan("recovery", &prefix, now_ms)? {
        let Some(checkpoint) = host_records::decode_recovery(&raw) else {
            continue;
        };
        let Some(stored) = checkpoint.version.as_ref() else {
            continue;
        };
        if stored.draft_id != version.draft_id || stored.revision > version.revision {
            continue;
        }
        let marker = Value::Array(vec![
            Value::String(checkpoint.session_key.clone()),
            Value::String(stored.draft_id.clone()),
            Value::from(stored.revision),
        ])
        .to_string();
        storage::write(
            &StorageKey {
                store: "recovery".to_string(),
                suffix,
            },
            Some(&marker),
            now_ms,
        )?;
    }
    super::dismissals::compact(session_key, now_ms)
}

/// The stored draft record of one session, or `None` when nothing is stored.
fn stored(session_key: &str, now_ms: i64) -> Result<Option<StoredDraftRecord>, &'static str> {
    Ok(storage::read(
        &StorageKey {
            store: "drafts".to_string(),
            suffix: session_key.to_string(),
        },
        now_ms,
    )?
    .filter(|raw| !raw.is_empty())
    .map(|raw| decode_stored_draft(&raw)))
}

fn write_draft(
    session_key: &str,
    record: &StoredDraftRecord,
    now_ms: i64,
) -> Result<(), &'static str> {
    storage::write(
        &StorageKey {
            store: "drafts".to_string(),
            suffix: session_key.to_string(),
        },
        Some(&encode_stored_draft(record)),
        now_ms,
    )
}

/// The core's draft identity as the host's. Two names for one `{draftId, revision}`: the core's
/// belongs to family d's queue rules and the host's to the records it owns alone.
fn host_version(
    version: Option<&ghostex_gx_chat_core::composer::queue::DraftVersion>,
) -> Option<DraftVersion> {
    version.map(|version| DraftVersion {
        draft_id: version.draft_id.clone(),
        revision: version.revision,
    })
}

/// `{draftId, revision}` off an operation's payload.
fn version(value: &Value) -> Option<DraftVersion> {
    let version = value.get("version")?;
    Some(DraftVersion {
        draft_id: version.get("draftId")?.as_str()?.to_string(),
        revision: version.get("revision")?.as_i64()?,
    })
}

/// Whether a stored record still carries the revision an operation claimed.
///
/// With no claimed version the TypeScript compared `updatedAt` instead; nothing in the Rust send
/// path omits it, so an operation without one matches only a record that has none either.
fn matches(record: &StoredDraftRecord, version: Option<&DraftVersion>) -> bool {
    match (record.version.as_ref(), version) {
        (Some(stored), Some(claimed)) => {
            stored.draft_id == claimed.draft_id && stored.revision == claimed.revision
        }
        (None, None) => true,
        _ => false,
    }
}
