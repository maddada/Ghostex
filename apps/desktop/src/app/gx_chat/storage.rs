//! The chat's client storage: the catalog rows it owns, and the two doors into the one database.
//!
//! The core names a store and a per-session suffix (`StorageKey { store, suffix }`) and never builds
//! a key string; this file owns the prefix, the backend and the budget. Every row below is the
//! matching entry of `packages/client-storage/catalog.ts`, which is the single source of truth, so a
//! record written by the TypeScript brain is read back unchanged, and the TypeScript pages that
//! still use the catalog read what this one writes.
//!
//! CDXC:Drafts 2026-09-22 DECISION:
//! User: a user's existing drafts, queued prompts, history and outbox must survive the switch to the
//! Rust brain. The prefixes, the backends and the bounds are the catalog's; nothing here invents a
//! key or a second storage file.
//!
//! **Two backends.** A catalog row on `local` is one raw string per key; a row on `indexeddb` is a
//! record with bounds and metadata. Where they live is `storage_backend.rs`: on the desktop the
//! `preferences` and `records` tables of the one client-storage database, in the GPUI web build
//! the page's own `packages/client-storage`, which applies the same catalog.

use ghostex_gx_chat_core::StorageKey;

use super::storage_backend;

const KIB: i64 = 1024;
const MIB: i64 = 1024 * 1024;
const DAY_MS: i64 = 86_400_000;

/// Where a catalogued store's rows live.
#[derive(Clone, Copy, Debug)]
pub(super) enum Backend {
    /// The `local` backend: one raw string per key.
    Local,
    /// The `indexeddb` backend: a record with bounds and metadata.
    Records(RecordBounds),
}

/// One `indexeddb` catalog row's bounds, which the desktop's record door enforces itself. The web
/// build's page storage applies its own copy of the catalog, so it reads only the id.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub(super) struct RecordBounds {
    /// The catalog `id`.
    pub(super) id: &'static str,
    /// The catalog `version`.
    pub(super) version: i64,
    pub(super) max_entry_bytes: i64,
    pub(super) max_bytes: i64,
    pub(super) max_entries: i64,
    /// The catalog `maxAgeMs`, `None` for a store that keeps its rows for ever.
    pub(super) max_age_ms: Option<i64>,
}

/// One chat-owned row of `packages/client-storage/catalog.ts`.
#[derive(Clone, Copy, Debug)]
struct ChatStore {
    /// The core's `StorageKey::store`.
    id: &'static str,
    /// The catalog's `key`: a prefix for a collection, the whole key for a singleton.
    prefix: &'static str,
    /// A singleton refuses a suffix, the way `managedStore` does.
    collection: bool,
    backend: Backend,
}

/// The `disk` shorthand: an indexeddb collection, 2 MiB an entry, 16 MiB a store, 2000 entries.
const fn disk(id: &'static str, max_age_ms: Option<i64>) -> Backend {
    Backend::Records(RecordBounds {
        id,
        version: 1,
        max_entry_bytes: 2 * MIB,
        max_bytes: 16 * MIB,
        max_entries: 2_000,
        max_age_ms,
    })
}

/// The `protectedDisk` shorthand: `disk` with 50,000 entries and 32 MiB, and no eviction.
const fn protected_disk(id: &'static str) -> Backend {
    Backend::Records(RecordBounds {
        id,
        version: 1,
        max_entry_bytes: 2 * MIB,
        max_bytes: 32 * MIB,
        max_entries: 50_000,
        max_age_ms: None,
    })
}

/// Every store the chat brain or this host reads or writes, in catalog order.
///
/// The four at the end are the host's own (`host_records.rs`); the core never names them.
const STORES: &[ChatStore] = &[
    ChatStore {
        id: "verbose",
        prefix: "ghostex.sessionChat.verbose.",
        collection: true,
        backend: disk("verbose", None),
    },
    ChatStore {
        id: "summary",
        prefix: "ghostex.sessionChat.summary.",
        collection: true,
        backend: disk("summary", None),
    },
    ChatStore {
        id: "tasksCollapsed",
        prefix: "ghostex.chat.agentTasks.collapsed",
        collection: false,
        backend: Backend::Local,
    },
    ChatStore {
        id: "claudeContext",
        prefix: "ghostex.chat.context-details.v1",
        collection: false,
        backend: Backend::Local,
    },
    ChatStore {
        id: "codexContext",
        prefix: "ghostex.chat.context-details.codex.v1",
        collection: false,
        backend: Backend::Local,
    },
    ChatStore {
        id: "cursorContext",
        prefix: "ghostex.chat.context-details.cursor.v1",
        collection: false,
        backend: Backend::Local,
    },
    ChatStore {
        id: "notices",
        prefix: "ghostex.sessionChat.noticeDismissed.",
        collection: true,
        backend: disk("notices", None),
    },
    ChatStore {
        id: "sessionOptions",
        prefix: "ghostex.sessionChat.options.",
        collection: true,
        backend: disk("sessionOptions", Some(30 * DAY_MS)),
    },
    ChatStore {
        id: "modelFavorites",
        prefix: "ghostex.model-favorites",
        collection: false,
        backend: Backend::Local,
    },
    // Not a chat store in the catalog's grouping, but the boot read carries it and the agent
    // launcher writes it, so the chat reads the same row rather than a copy of its own.
    ChatStore {
        id: "modelCatalog",
        prefix: "ghostex.agentModelCatalog.v1",
        collection: false,
        backend: Backend::Records(RecordBounds {
            id: "modelCatalog",
            version: 1,
            max_entry_bytes: 2 * MIB,
            max_bytes: 16 * MIB,
            max_entries: 1,
            max_age_ms: Some(30 * DAY_MS),
        }),
    },
    ChatStore {
        id: "modelOutbox",
        prefix: "ghostex.model-selection-outbox.",
        collection: true,
        backend: protected_disk("modelOutbox"),
    },
    // The one chat prefix that is not `ghostex.sessionChat.`; the colon is part of it.
    ChatStore {
        id: "retiredQuestions",
        prefix: "ghostex:async-questions:",
        collection: true,
        backend: protected_disk("retiredQuestions"),
    },
    ChatStore {
        id: "questionDrafts",
        prefix: "ghostex.sessionChat.questionDraft.",
        collection: true,
        backend: protected_disk("questionDrafts"),
    },
    ChatStore {
        id: "chatClient",
        prefix: "ghostex.sessionChat.clientId",
        collection: false,
        backend: Backend::Local,
    },
    ChatStore {
        id: "returnedPrompts",
        prefix: "ghostex.sessionChat.returnedPrompts.applied",
        collection: false,
        backend: Backend::Local,
    },
    ChatStore {
        id: "drafts",
        prefix: "ghostex.sessionChat.draft.",
        collection: true,
        backend: protected_disk("drafts"),
    },
    ChatStore {
        id: "recovery",
        prefix: "ghostex.sessionChat.recovery.",
        collection: true,
        backend: protected_disk("recovery"),
    },
    ChatStore {
        id: "recoveryDismissed",
        prefix: "ghostex.sessionChat.recoveryDismissed.",
        collection: true,
        backend: protected_disk("recoveryDismissed"),
    },
    ChatStore {
        id: "draftOutbox",
        prefix: "ghostex.sessionChat.outbox.",
        collection: true,
        backend: protected_disk("draftOutbox"),
    },
    // `sentHistory` is the only chat store the catalog gives a `retainedAt`: a row's eviction age is
    // its own `createdAt`, not its write time, and `maxAgeMs` is explicitly null so nothing expires.
    // The Rust record door has no `retainedAt` hook, so it stamps the write time instead; the effect
    // is that a cache eviction the service would make by prompt age this door makes by write age.
    ChatStore {
        id: "sentHistory",
        prefix: "ghostex.sessionChat.sent.",
        collection: true,
        backend: Backend::Records(RecordBounds {
            id: "sentHistory",
            version: 1,
            max_entry_bytes: 2 * MIB,
            max_bytes: 16 * MIB,
            max_entries: 50,
            max_age_ms: None,
        }),
    },
    ChatStore {
        id: "deliveryReceipts",
        prefix: "ghostex.sessionChat.delivered.",
        collection: true,
        backend: protected_disk("deliveryReceipts"),
    },
    // The retained transcript cache (`persistence.ts`), keyed by the retention key rather than by
    // the storage session key. Its catalog `retainedAt` reads the record's own `savedAt`, which is
    // also when the record is written, so the write stamp this door keeps instead means the same
    // thing here; `sentHistory`'s does not, which is why that one has a note of its own.
    ChatStore {
        id: "chatSnapshots",
        prefix: "ghostex.sessionChat.snapshot.",
        collection: true,
        backend: Backend::Records(RecordBounds {
            id: "chatSnapshots",
            version: 1,
            max_entry_bytes: 2 * MIB,
            max_bytes: 32 * MIB,
            max_entries: 24,
            max_age_ms: Some(7 * DAY_MS),
        }),
    },
];

/// The catalog row for a store id, or `None` for a store this build does not own.
fn store(id: &str) -> Option<&'static ChatStore> {
    STORES.iter().find(|store| store.id == id)
}

/// The full client-storage key a [`StorageKey`] names, or `None` when the store is unknown or a
/// singleton was handed a suffix (which is what `managedStore` refuses).
pub(super) fn full_key(key: &StorageKey) -> Option<String> {
    let store = store(&key.store)?;
    if !store.collection && !key.suffix.is_empty() {
        return None;
    }
    Some(format!("{}{}", store.prefix, key.suffix))
}

/// Reads one record, `None` when nothing is stored or the catalog no longer admits it.
///
/// A refusal is returned rather than swallowed so the caller can count it; the core treats a
/// `None` value and a failed read the same way, which is what the TypeScript's `catch` did.
pub(super) fn read(key: &StorageKey, now_ms: i64) -> Result<Option<String>, &'static str> {
    let store = store(&key.store).ok_or("unregistered")?;
    let name = full_key(key).ok_or("unregistered")?;
    storage_backend::read(store.id, store.backend, &name, now_ms)
}

/// Every live record of a collection store whose SUFFIX starts with `prefix`, as
/// `(suffix, raw)` pairs.
///
/// The suffix is what the TypeScript called the record's key once the store's own prefix was off
/// (`key.slice(STORAGE_PREFIX.length)` in `storedSessionChatOptionKeys`), so a caller compares and
/// stores exactly the strings the TypeScript brain wrote. A singleton store has no suffix to scan and
/// answers with nothing.
pub(super) fn scan(
    store_id: &str,
    prefix: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    let store = store(store_id).ok_or("unregistered")?;
    if !store.collection {
        return Ok(Vec::new());
    }
    let Backend::Records(definition) = store.backend else {
        // Only the `records` table is scannable. No chat store on `local` is a collection, so this
        // is unreachable rather than a gap; it answers empty rather than inventing a second scan.
        return Ok(Vec::new());
    };
    let full = format!("{}{}", store.prefix, prefix);
    let offset = store.prefix.len();
    Ok(storage_backend::scan(definition, &full, now_ms)?
        .into_iter()
        .filter_map(|(key, raw)| Some((key.get(offset..)?.to_string(), raw)))
        .collect())
}

/// Writes one record, or removes it when `value` is `None`.
///
/// **The entry bound is checked and a value that breaks it is refused, never truncated.** That is
/// the rule the sidebar's own door wrote down: a door that quietly made room by deleting another
/// feature's data is a loss nobody can trace back to it. For the `local` rows here the store total
/// cannot be the bound that breaks, because every one of them is a singleton whose 64 KiB entry
/// limit is half its 128 KiB store limit; the shared 2 MiB backend total stays the client-storage
/// service's to manage, as it always has been.
pub(super) fn write(
    key: &StorageKey,
    value: Option<&str>,
    now_ms: i64,
) -> Result<(), &'static str> {
    let store = store(&key.store).ok_or("unregistered")?;
    let name = full_key(key).ok_or("unregistered")?;
    if let (Backend::Local, Some(raw)) = (store.backend, value) {
        if storage_bytes(&name, raw) > 64 * KIB {
            return Err("entry");
        }
    }
    storage_backend::write(store.id, store.backend, &name, value, now_ms)
}

/// `packages/client-storage/budgets.ts`: conservative UTF-16 accounting of the key and the value
/// together. It is an app budget, not a claim about a browser's exact quota.
fn storage_bytes(key: &str, raw: &str) -> i64 {
    2 * (key.encode_utf16().count() as i64 + raw.encode_utf16().count() as i64)
}
