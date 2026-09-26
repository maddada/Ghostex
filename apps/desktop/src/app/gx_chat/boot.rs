//! `Effect::ReadComposerBoot`: the one read the chat waits for before it publishes anything.
//!
//! The deleted QuickJS brain's `start` awaited `composer('read')` and only then built its
//! controller, and `ChatCore::republish` reproduces that: nothing ships until [`ComposerBootRead`] lands. This is
//! the Rust port of `nativeComposerRequest(sessionKey, {operation: 'read'})` in the deleted
//! `apps/desktop/sidebar/session-chat-runtime/native-composer.ts`, key for key and in the same
//! write order.

use ghostex_gx_chat_core::composer::storage::{
    StoredDraftRecord, decode_stored_draft, decode_summary, decode_verbose,
};
use ghostex_gx_chat_core::{ChatSettings, ComposerBootRead, StorageKey};
use serde_json::{Map, Value, json};

use super::storage;

/// What the boot read's records did, which is how the caller tells a refusal from an empty profile.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `native-host.ts`'s `start` had a `.catch` that emptied the transcript and published
/// `{status: 'error'}`, and it fired when `composer('read')` REJECTED, not when a record was absent:
/// every accessor inside that operation caught for itself. The one thing that makes all of them
/// fail at once is client storage being unavailable (`initializeClientStorage()` throwing), so the
/// host's equivalent test is "every read this pass attempted refused". A profile that has simply
/// never opened chat refuses nothing and boots normally, which is the case that must not be
/// mistaken for the failure.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BootReads {
    /// Reads that refused, which the caller adds to `storageRefused`.
    pub(super) errors: usize,
    /// Reads attempted, refused or not.
    pub(super) attempts: usize,
}

impl BootReads {
    /// Whether client storage answered nothing at all, which is the boot read failing.
    pub(super) fn all_refused(&self) -> bool {
        self.attempts > 0 && self.errors == self.attempts
    }

    fn attempt(&mut self) {
        self.attempts += 1;
    }

    fn refused(&mut self) {
        self.errors += 1;
    }
}

/// Reads everything the chat needs at boot for one session.
///
/// Every read that fails is treated as "nothing stored", which is what the TypeScript's `catch`
/// around each accessor did; the caller counts the refusals and tests [`BootReads::all_refused`].
pub(super) fn read(session_key: &str, now_ms: i64, errors: &mut BootReads) -> ComposerBootRead {
    let client_id = client_id(now_ms, errors);
    let stored = load("drafts", session_key, now_ms, errors).map(|raw| decode_stored_draft(&raw));
    let entry = entry_with_version(stored.as_ref());
    let model_catalog = parse(load("modelCatalog", "", now_ms, errors));
    let claude_context = parse(load("claudeContext", "", now_ms, errors));
    let codex_context = parse(load("codexContext", "", now_ms, errors));
    let cursor_context = parse(load("cursorContext", "", now_ms, errors));
    let dismissed_notice = parse(load("notices", session_key, now_ms, errors));
    let summary_mode = decode_summary(load("summary", session_key, now_ms, errors).as_deref());
    let verbose_override = decode_verbose(load("verbose", session_key, now_ms, errors).as_deref());
    ComposerBootRead {
        session_key: session_key.to_string(),
        client_id,
        entry,
        next_version: next_draft_version(),
        option_states: scoped("sessionOptions", session_key, now_ms, errors),
        model_outboxes: scoped("modelOutbox", session_key, now_ms, errors),
        model_catalog,
        chat_settings: serde_json::to_value(chat_settings()).unwrap_or(Value::Null),
        context_preferences: json!({
            "claude": claude_context,
            "codex": codex_context,
            "cursor": cursor_context,
        }),
        dismissed_notice,
        summary_mode,
        verbose_override: match verbose_override {
            Some(verbose) => Value::Bool(verbose),
            None => Value::Null,
        },
    }
}

/// `nativeChatSettings(sessionKey)`: the two settings the chat reads from the app rather than from
/// its own storage.
///
/// CDXC:SessionChat 2026-09-23 WHY:
/// The boot read is the ONLY channel these reach the chat through. The `chatSettings` push that the
/// deleted `broker.ts` would have sent never fired on desktop, because
/// `relay_session_chat_runtime_request` forwards only `limit` and `beforeOffset` and so dropped the
/// `catalog` flag that turned it on. The host used to answer a constant `false` here, so a chat under
/// the Rust brain showed full account emails in its status line, account panel and context rows
/// while Settings said to hide them. `hideAccountEmails` is the same Settings key the TypeScript
/// read (`hud.settings`, fed from this settings file). `title` stays `null`: the TypeScript looked
/// it up in the service's `sessionsById` by `projectId:sessionId`, and on desktop that map was never
/// fed (the `hydrate` message carried no groups), so it was `null` under QuickJS too.
pub(super) fn chat_settings() -> ChatSettings {
    ChatSettings {
        hide_account_emails: crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("hideAccountEmails")
            .and_then(Value::as_bool)
            == Some(true),
        title: None,
    }
}

/// `sessionChatDraftClientId()`: this computer's opaque draft-origin id, minted on first sight.
///
/// CDXC:Drafts 2026-09-10 WHY:
/// It is PERSISTED, and the startup outbox replay uses the same one as the composer: a fresh id
/// every mount makes this client's own last push look like another device and pops the conflict bar
/// against itself. The Step 4 host read the record and answered with an empty string when it was
/// absent, which is a fresh install, a new profile, or any user who had never opened chat: every
/// draft echo then came back unattributed and the outbox rows carried no `clientId` at all. The
/// shape is `packages/core-ui/chat/session-chat-client-id.ts`'s, `gx-` then two base-36 runs,
/// because an id is compared and stored but never parsed. A refused write is counted and the
/// in-memory id is used anyway, which is what the TypeScript's `catch` does for private mode.
pub(super) fn client_id(now_ms: i64, errors: &mut BootReads) -> String {
    let key = StorageKey {
        store: "chatClient".to_string(),
        suffix: String::new(),
    };
    if let Some(stored) = load("chatClient", "", now_ms, errors) {
        return stored;
    }
    let created = format!(
        "gx-{}{}",
        base36(u64::from_be_bytes(
            super::platform::random_bytes()[..8]
                .try_into()
                .unwrap_or_default()
        )),
        base36(now_ms.max(0) as u64)
    );
    errors.attempt();
    if storage::write(&key, Some(&created), now_ms).is_err() {
        errors.refused();
    }
    created
}

/// `Number.prototype.toString(36)`: lower-case digits, most significant first.
fn base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// One stored record, or `None` when it is absent, empty or unreadable.
fn load(store: &str, suffix: &str, now_ms: i64, errors: &mut BootReads) -> Option<String> {
    errors.attempt();
    match storage::read(
        &StorageKey {
            store: store.to_string(),
            suffix: suffix.to_string(),
        },
        now_ms,
    ) {
        Ok(value) => value.filter(|raw| !raw.is_empty()),
        Err(_) => {
            errors.refused();
            None
        }
    }
}

/// `entry: { ...stored, version }`.
///
/// CDXC:Drafts 2026-09-19 WHY:
/// A stored revision is reused only when the entry is neither submitted nor parked. A parked draft
/// was handed to the terminal, so the composer opens empty; reusing its revision made the next blur
/// save claim "" under the revision gxserver holds for the handed-off text, and every save failed
/// with "Two editors changed the same draft revision". It starts a fresh draft instead, which is
/// what the live handoff already does with `nextVersion`.
///
/// With nothing stored the object holds ONLY `version`, because `{ ...null, version }` has no other
/// key. A caller that expects `text` to be present would read a default it was never given.
fn entry_with_version(stored: Option<&StoredDraftRecord>) -> Value {
    let reuse = stored.and_then(|record| {
        if record.submitted || record.parked {
            return None;
        }
        record.version.clone()
    });
    let version = match reuse {
        Some(version) => json!({"draftId": version.draft_id, "revision": version.revision}),
        None => next_draft_version(),
    };
    let Some(record) = stored else {
        return json!({ "version": version });
    };
    let mut entry = Map::new();
    entry.insert("text".into(), Value::String(record.text.clone()));
    if let Some(updated_at) = record.updated_at {
        entry.insert("updatedAt".into(), Value::from(updated_at));
    }
    entry.insert("submitted".into(), Value::Bool(record.submitted));
    entry.insert("parked".into(), Value::Bool(record.parked));
    entry.insert("version".into(), version);
    Value::Object(entry)
}

/// `nextSessionChatDraftVersion()`: a fresh identity at revision 1.
///
/// The core generates no ids on purpose (it reads no random source and must cross UniFFI), so the
/// host mints them, in the same v4 shape `crypto.randomUUID()` produces.
pub(super) fn next_draft_version() -> Value {
    json!({"draftId": super::platform::uuid_v4(), "revision": 1})
}

/// `{ "<sessionKey>[#<scope>]": <value> }` for every stored record of this session.
///
/// `storedSessionChatOptionKeys` and `storedModelSelectionKeys` take exactly the keys that are the
/// session key itself or start with `<sessionKey>#`, and hand back the part after the store's own
/// prefix. The `#` scope is how one session's pills are remembered per worktree or per draft agent,
/// so a session with scopes whose unscoped row alone was read opened on another scope's options.
fn scoped(store: &str, session_key: &str, now_ms: i64, errors: &mut BootReads) -> Value {
    errors.attempt();
    let rows = match storage::scan(store, session_key, now_ms) {
        Ok(rows) => rows,
        Err(_) => {
            errors.refused();
            return Value::Object(Map::new());
        }
    };
    let mut states = Map::new();
    for (suffix, raw) in rows {
        // The scan is a prefix scan, so a sibling session whose key merely starts with this one's
        // (`p:s` against `p:s2`) would come back too. Only the key itself and its `#` scopes count.
        if suffix != session_key && !suffix.starts_with(&format!("{session_key}#")) {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            states.insert(suffix, value);
        }
    }
    Value::Object(states)
}

/// A stored JSON record, or `null` when it is missing or unreadable.
fn parse(raw: Option<String>) -> Value {
    raw.and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null)
}
