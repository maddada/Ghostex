//! Everything the core asks the host to do.
//!
//! The core performs no I/O: it returns effects and waits for the matching [`crate::Event`]. Each
//! effect is plain data so the same list works over UniFFI and over wasm.
//!
//! [`Effect`] is the typed form every host receives from `handle`. A host performs the I/O ones
//! itself and hands the ones only the view can do back as a [`HostRequest`], the wire form of the
//! `requests` array of a [`crate::Frame`] that `apps/desktop/src/app/native_chat/state.rs`
//! dispatches.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::event::StorageKey;
use crate::wire::ChatRpcMethod;

/// One thing the host must do.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Effect {
    /// Call gxserver. The answer comes back as [`crate::Event::RpcSettled`] with this id.
    SendRpc {
        request_id: u64,
        method: ChatRpcMethod,
        params: Box<Value>,
    },
    /// Subscribe this session's chat stream, asking for at least `limit` messages.
    Subscribe { limit: u32, catalog: bool },
    /// Drop the subscription.
    Unsubscribe,
    /// Tear the socket down and build a fresh one, the only recovery for a subscription that came
    /// up but never delivered its snapshot.
    Reconnect,
    /// Read a stored record; answered by [`crate::Event::StorageLoaded`].
    ReadStorage { key: StorageKey },
    /// Read several stored records as ONE host round trip; answered by
    /// [`crate::Event::StorageBatchLoaded`].
    ///
    /// Some of the host's storage operations read more than one record and answer once.
    /// `composer('asyncQuestionRead')` is the one this exists for: it hands back
    /// `{drafts, retired}` from two different stores in a single call
    /// (`apps/desktop/sidebar/session-chat-runtime/native-composer.ts`). Splitting it into two
    /// [`Effect::ReadStorage`]s would make the core take two round trips for what the host
    /// serves as one.
    ///
    /// A host performs the reads in the order given and answers once with the same order.
    ReadStorageBatch { keys: Vec<StorageKey> },
    /// Read this session's retained transcript; answered by
    /// [`crate::Event::RetainedSnapshotLoaded`].
    ///
    /// `readPersistedSessionChat` in
    /// `apps/desktop/sidebar/session-chat-runtime/persistence.ts`: one record of the managed store
    /// [`crate::session::persistence::SNAPSHOTS_STORE`], keyed by
    /// [`crate::session::persistence::storage_key`], which the host builds because only it knows
    /// the machine id. Its own effect rather than an [`Effect::ReadStorage`] for two reasons: the
    /// core must not build a storage key string, and this round trip belongs to the STORE rather
    /// than to an action, so it must not join `publish_awaits` and move a publish.
    ReadRetainedSnapshot,
    /// Write this session's retained transcript back. `value` of `None` deletes the record.
    ///
    /// `persistSessionChat`, which is `storage.update(key, previous => …)`: the host performs the
    /// same read-modify-write, keeping `previous` when its `savedAt` is NEWER than this record's,
    /// so two writers racing cannot roll the tail backwards. Nothing answers it; a failed cache
    /// write is not a failure the chat reports, and the live stream stays authoritative.
    WriteRetainedSnapshot { value: Option<String> },
    /// The presentation cache the sidebar and the chat share was updated.
    ///
    /// `createSessionChatPresentationStore(…, (state) => requests.push({kind: 'broker', method:
    /// 'presentation', params: {state}}))` in `native-host.ts:676` was the TypeScript's form, and
    /// the desktop host still dispatches it (`relay_session_chat_runtime_request`,
    /// `method == "presentation"`). `state` is the WHOLE merged snapshot, not the patch, because
    /// that is what the store's `onChange` received.
    ///
    /// CDXC:SessionChat 2026-09-14 DECISION:
    /// User: returning to a chat should immediately restore its account, context usage and status
    /// line, then refresh them in the background; initial waiting belongs only to a chat that has
    /// not loaded yet. Without this effect the core could read the cache at boot and never write
    /// it back, so the second visit was as empty as the first.
    UpdatePresentation { state: Box<Value> },
    /// Read everything the chat needs at boot in one go; answered by
    /// [`crate::Event::ComposerBootRead`].
    ///
    /// The host owns this as one operation (`composer('read')`), and `start` in `native-host.ts`
    /// published nothing until it answered, so the core waits on it the same way
    /// rather than issuing a dozen separate reads whose answers would each ship a frame.
    ReadComposerBoot { request_id: u64 },
    /// Write a stored record. `value` of `None` deletes it.
    WriteStorage {
        key: StorageKey,
        value: Option<String>,
        /// Flush to disk before reporting, for records that must survive a crash.
        durable: bool,
    },
    /// Write several stored records as ONE host round trip; answered by
    /// [`crate::Event::StorageBatchWritten`].
    ///
    /// The write counterpart of [`Effect::ReadStorageBatch`], for the same reason:
    /// `composer('asyncQuestionRetire')` writes the remaining drafts and the retired-id list in a
    /// single call. A host performs the writes in the order given and answers once.
    WriteStorageBatch { writes: Vec<StorageWrite> },
    /// Push a store's pending writes to disk and say when they are there; answered by
    /// [`crate::Event::StorageWritten`] with the same `store` and an empty suffix.
    ///
    /// This is the host's `composer('flush')`, which is `flushDraftSaves(sessionKey)`: the
    /// durable save outbox and its retry worker stay with the host
    /// (`docs/2026-09-21/rust-chat/HOST-TODO.md` section 3), and a submission must not deliver
    /// before the submitted revision is on disk. It is its own effect rather than a
    /// [`Effect::WriteStorage`] with no value, which would DELETE the record.
    FlushStorage { store: String },
    /// Record the drafts the daemon reports as delivered, so the sent-prompt history and the
    /// recall ring learn about them.
    ///
    /// `options.onDeliveredDrafts(syncedDraft?.deliveredDrafts ?? [])` ran on every change of
    /// the synced draft (`controller.ts`, the `useEffect` on `syncedDraft`), and the host's arm was
    /// `composer('deliveries', {deliveries})`, which was
    /// `recordDeliveredSessionChatDrafts(...)` in
    /// `apps/desktop/sidebar/session-chat-runtime/native-composer.ts`. Fire and forget: the
    /// TypeScript neither awaited it nor published on its answer, so nothing answers it here
    /// either. Its own variant because the records it touches belong to the host's sent-history
    /// store, whose keys the core does not build. Each entry is one `SessionChatDeliveredDraft`
    /// as the wire carries it.
    RecordDeliveries { deliveries: Vec<Value> },
    /// Wake the core with [`crate::Event::Tick`] in `delay_ms`, or cancel the pending wake when
    /// `delay_ms` is `None`.
    SetTimer { delay_ms: Option<u64> },
    /// Replace the composer text the host owns. `caret` is a UTF-16 offset, which is what a
    /// JavaScript string index is; the host converts it.
    SetComposerText {
        content: String,
        caret: Option<usize>,
        /// The text came from history recall, which the composer treats differently from an
        /// insert.
        from_history: bool,
    },
    /// Clear the composer only if it still holds exactly this text.
    ClearComposerIfUnchanged { text: String },
    /// Ask the host to open a path, a URL, or a file at a position.
    Open(OpenTarget),
    /// Put text on the clipboard.
    Copy { text: String },
    /// Show a toast.
    Toast { level: String, message: String },
    /// A prompt the agent handed back was claimed: put it in the composer.
    ///
    /// Its own variant rather than a [`Effect::HostAction`] string, because the host already has a
    /// dispatch arm for it ([`RequestKind::ReturnedPrompt`], `returnedPrompt`/`restore` in
    /// `apps/desktop/src/app/native_chat/state.rs`) and the app shell has none.
    RestoreReturnedPrompt { text: String },
    /// A Markdown document was written: put its path on the clipboard and say so.
    ///
    /// Its own variant rather than a [`Effect::HostAction`] string, because the host already has a
    /// dispatch arm for it (`markdownSaved` in `apps/desktop/src/app/native_chat/state.rs`) and the
    /// Effect-to-`HostRequest` mapping would otherwise have to special-case one action name.
    MarkdownSaved { path: String },
    /// Something only the app shell can do: switch to the terminal, pick attachments, report the
    /// composer ready. Free-form because the list belongs to the app, not to chat.
    HostAction { action: String, params: Box<Value> },
}

/// One write of a [`Effect::WriteStorageBatch`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageWrite {
    pub key: StorageKey,
    /// `None` deletes the record.
    pub value: Option<String>,
    /// Flush to disk before reporting.
    pub durable: bool,
}

/// What [`Effect::Open`] should open.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenTarget {
    Url {
        url: String,
    },
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<u32>,
    },
}

/// The wire form of an effect, as it rides in a [`crate::Frame`].
///
/// `kind` chooses the host's dispatch arm, `method` the operation inside it, and `params` carries
/// the rest. `id` is present only on the two kinds that expect an answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub kind: RequestKind,
    pub method: String,
    pub params: Map<String, Value>,
}

macro_rules! request_kinds {
    ($($(#[$meta:meta])* $variant:ident => $wire:literal),+ $(,)?) => {
        /// The host dispatch arms. Open, because a host newer than the core may know more.
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum RequestKind {
            $($(#[$meta])* $variant,)+
            /// A kind this build does not know; kept verbatim.
            Other(String),
        }

        impl RequestKind {
            /// The wire spelling.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value.as_str(),
                }
            }

            /// Maps a wire spelling to a kind; never fails.
            pub fn from_wire(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    other => Self::Other(other.to_string()),
                }
            }
        }
    };
}

request_kinds! {
    /// A gxserver call; the host answers it by id.
    Rpc => "rpc",
    /// A call into the shared transcript broker; answered by id when it carries one.
    Broker => "broker",
    /// Replace the composer text.
    Composer => "composer",
    /// The boot result: client id, draft entry, stored options.
    ComposerInit => "composerInit",
    /// Clear the composer only if it still holds this text.
    ComposerClearExpected => "composerClearExpected",
    /// Insert reference pills for the paths the host just attached.
    AttachmentReferences => "attachmentReferences",
    /// An image read finished or failed.
    ChatImage => "chatImage",
    /// Put the prompt the agent handed back into the composer.
    ReturnedPrompt => "returnedPrompt",
    /// A Markdown document was written; copy its path and say so.
    MarkdownSaved => "markdownSaved",
    /// Something only the app shell can do.
    Host => "host",
    /// A send left the composer.
    DraftSubmitted => "draftSubmitted",
    /// A send failed; put the text back.
    SubmissionFailed => "submissionFailed",
    /// A draft arrived from another client.
    DraftReceived => "draftReceived",
    /// An action finished.
    ActionComplete => "actionComplete",
    /// An action failed.
    ActionError => "actionError",
}

impl Serialize for RequestKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RequestKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_wire(&String::deserialize(deserializer)?))
    }
}
