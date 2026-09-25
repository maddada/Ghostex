//! Everything that can reach the chat core.
//!
//! One enum, one direction. The host performs I/O and turns each result into an event; the core
//! never calls back out, which is what keeps it usable from UniFFI and from wasm.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::UserAction;
use crate::wire::{ChatFrame, RpcOutcome};

/// Something that happened outside the core.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// The host is opening this chat. Carries the identity and whatever the host had cached, so
    /// the first document can be drawn before any read answers.
    Start(Box<StartConfig>),
    /// An accepted gxserver frame for this session.
    Frame(Box<ChatFrame>),
    /// The socket's own state changed: connected, lost, or rebuilt.
    Connection(ConnectionUpdate),
    /// A request the core asked for has settled.
    RpcSettled {
        request_id: u64,
        outcome: Box<RpcOutcome>,
    },
    /// The user did something in the renderer.
    Action(Box<UserAction>),
    /// A timer the core set is due, or the host is simply driving the clock.
    Tick,
    /// A stored record the core asked for, or `None` when nothing was stored.
    StorageLoaded {
        key: StorageKey,
        value: Option<String>,
    },
    /// A storage write the core asked for has settled.
    StorageWritten {
        key: StorageKey,
        error: Option<String>,
    },
    /// The answer to [`crate::Effect::ReadStorageBatch`], in the order the keys were asked for.
    StorageBatchLoaded { records: Vec<StorageRecord> },
    /// The answer to [`crate::Effect::WriteStorageBatch`].
    ///
    /// One outcome for the whole batch, because the host operation it stands for is one call: a
    /// refusal refuses all of it.
    StorageBatchWritten {
        keys: Vec<StorageKey>,
        error: Option<String>,
    },
    /// The answer to [`crate::Effect::ReadRetainedSnapshot`], or `None` when nothing was stored.
    RetainedSnapshotLoaded { value: Option<String> },
    /// [`crate::Effect::ReadComposerBoot`] was refused.
    ///
    /// `start`'s own `.catch` in `native-host.ts:659`: the transcript list is emptied and the
    /// document becomes `{status: 'error', error}`, whatever the chat had drawn. Without it a
    /// refused `composer('read')` leaves the core with no controller, and therefore silent, for
    /// ever.
    ComposerBootFailed { error: String },
    /// The answer to [`crate::Effect::ReadComposerBoot`].
    ///
    /// One read rather than a dozen, because the host already performs it as one:
    /// `composer('read')` in `apps/desktop/sidebar/session-chat-runtime/native-composer.ts`
    /// returns the client id, the stored draft, the stored option states and model outboxes, the
    /// dismissed notice, the bundled model catalog, the chat settings, the context preferences and
    /// the two transcript modes in a single object, and `start` in `native-host.ts` waited for it
    /// before it published anything, as the core does.
    ComposerBootRead(Box<ComposerBootRead>),
    /// The chat settings the host pushes changed.
    SettingsChanged(Box<ChatSettings>),
    /// The context-details preferences changed.
    ContextPreferencesChanged {
        provider: String,
        preferences: Value,
    },
    /// The agent model catalog changed.
    ModelCatalogChanged { catalog: Value },
    /// The renderer measured something the document's layout depends on.
    Measured(Measurement),
    /// The host's draft text changed outside the core (it owns the text field).
    DraftChanged { text: String },
}

/// What the host knows when it opens a chat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    /// All three default, because the host does not always know them at boot: the desktop host
    /// learns the client id from its own storage read, and a preview chat (the retired Chat Lab's)
    /// has a preview backend instead of a session.
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub session_id: String,
    /// The transcript the host had cached, so the first paint is not blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_snapshot: Option<Value>,
    /// The presentation cache the sidebar and the chat share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_presentation: Option<Value>,
    /// The preview scenario (the retired Chat Lab's), when this chat is a preview rather than a
    /// session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<Value>,
    /// `JSON.stringify([machineId, projectId, sessionId])`, the key of this session's retained
    /// transcript record.
    ///
    /// The host builds it with [`crate::session::persistence::storage_key`], because only it knows
    /// the machine id, and the core writes it back inside the record. Empty when the host retains
    /// nothing, as a preview chat does.
    #[serde(default)]
    pub retained_key: String,
    /// The host's composer is a touch field with no Enter-to-send (the phone): the composer's
    /// resting placeholder then names the tap and hold gestures instead of the keys.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub touch_composer: bool,
}

/// Everything `composer('read')` answers with, in the order that host writes it.
///
/// Free-form under the typed lid because each subtree belongs to a different family and every one
/// of them is a stored record whose shape is the user's data, not this crate's: the draft entry,
/// the option states, the model outboxes and the context preferences are all parsed by their own
/// owner. Nothing here is invented; `native-composer.ts:142` is the whole list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerBootRead {
    /// `<projectId>:<sessionId>`, with a `remote-<machineId>:` prefix off the local machine. The
    /// storage key for every per-session record, so the core carries it rather than rebuilding it.
    #[serde(default)]
    pub session_key: String,
    /// The persistent per-installation id a draft save is attributed to.
    #[serde(default)]
    pub client_id: String,
    /// The stored draft, with its revision already replaced when the stored one cannot be reused.
    #[serde(default)]
    pub entry: Value,
    /// A fresh revision, for the composer that has to start a new draft.
    #[serde(default)]
    pub next_version: Value,
    /// `{ "<sessionKey>[#<scope>]": SessionChatOptionState }`.
    #[serde(default)]
    pub option_states: Value,
    /// `{ "<sessionKey>[#<scope>]": ModelSelectionOutbox }`.
    #[serde(default)]
    pub model_outboxes: Value,
    /// The catalog bundled with this build, before any push arrives.
    #[serde(default)]
    pub model_catalog: Value,
    /// `{hideAccountEmails, title}`.
    #[serde(default)]
    pub chat_settings: Value,
    /// `{claude, codex}` context-details preferences.
    #[serde(default)]
    pub context_preferences: Value,
    /// The notice this session already dismissed, or null.
    #[serde(default)]
    pub dismissed_notice: Value,
    /// Whether the transcript opens folded into one row per turn.
    #[serde(default)]
    pub summary_mode: bool,
    /// The verbose override, or absent to follow the setting.
    #[serde(default)]
    pub verbose_override: Value,
}

/// The socket's state, which decides whether the core waits or re-reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionUpdate {
    /// The subscription is live and its first authoritative frame may now arrive.
    Subscribed,
    /// The socket went away; the core holds what it has.
    Lost,
    /// A fresh socket replaced the old one, so the stream position restarts.
    Resubscribed,
}

/// A record in client storage the chat owns.
///
/// Spelled as the store plus the per-session suffix rather than the full key, so the host owns the
/// prefix and the core never builds a storage key string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageKey {
    /// The catalog store id, for example `questionDrafts` or `notices`.
    pub store: String,
    /// The part of the key after the store's prefix; empty for a singleton record.
    pub suffix: String,
}

/// One record of a [`crate::Event::StorageBatchLoaded`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRecord {
    pub key: StorageKey,
    /// `None` when nothing was stored.
    pub value: Option<String>,
}

/// The two settings the host pushes into the chat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSettings {
    /// Masks account text everywhere the chat shows it.
    pub hide_account_emails: bool,
    /// The session's display title, or `null` when it has none.
    pub title: Option<String>,
}

/// A size or position only the renderer can know, folded back into the document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Measurement {
    /// Which composer toolbar controls did not fit.
    ComposerOverflow {
        overflowed: Vec<String>,
        options_overflowed: bool,
    },
    /// Where the status line wraps.
    ContextStatusRows { rows: Vec<u32> },
    /// The rows the renderer currently draws open, so only those ship their detail.
    OpenRowDetails { rows: Vec<OpenRowDetail> },
}

/// One open row, identified the way the renderer keys it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRowDetail {
    pub key: String,
    /// `file` for a change card, anything else for a tool row.
    pub kind: String,
    pub message_id: String,
    pub index: usize,
}
