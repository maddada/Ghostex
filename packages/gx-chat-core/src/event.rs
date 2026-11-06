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
    /// Every session the host's own session list knows, on every machine, so a message between
    /// agents is named after the other session even when the transcript never says who it is.
    SessionDirectoryChanged { sessions: Vec<KnownSession> },
    /// What only the app knows about Read aloud (the feature is on, Auto-read, the chat on screen,
    /// a voice conversation, the microphone) and its speech engine's answers (`crate::read_aloud`).
    ReadAloud(crate::read_aloud::ReadAloudEvent),
    /// Whether any renderer is showing this chat.
    ///
    /// A hidden chat stays retained, followed and folded, but its once-a-second clocks, its meter
    /// clocks and its accounts poll wait; showing it runs whatever they owe in the same turn, so
    /// the first document it sends is current. A host that never sends this keeps every timer
    /// running.
    Visibility { shown: bool },
}

/// One session the host's session list knows (the sidebar on the desktop and the web build, the
/// inventory on the phone).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KnownSession {
    /// `<machine>:<project>:<session>`, the reference `ghostex agents send` takes; empty when the
    /// host does not know the machine's part.
    pub global_ref: String,
    pub project_id: String,
    pub session_id: String,
    /// The title the session list shows.
    pub title: String,
    /// The agent id (`claude`, `codex`), which names its logo.
    pub agent_id: String,
    /// The agent's display name (`Claude`, `Codex`).
    pub agent_name: String,
    /// The session is a coordinator (an orchestrator): its cards draw the crown, as its sidebar
    /// row does.
    pub coordinator: bool,
    /// `working`, `idle` or `attention`, for the composer's `+` picker. Not a name: a change to it
    /// alone does not rebuild the transcript ([`KnownSession::same_names`]).
    pub activity: String,
    /// `running`, `sleeping` or `stopped`, for the `+` picker.
    pub lifecycle_state: String,
    /// When the session last did something worth sorting by (an ISO time), for the `+` picker's
    /// newest-first order.
    pub active_at: String,
    /// The name of the computer it runs on, as the host's machine tab spells it, for the `+`
    /// picker's rows of sessions on another computer; empty when the host does not say.
    pub computer: String,
    /// The name of its project, for the `+` picker's rows of sessions on another computer.
    pub project_name: String,
    /// Its computer is not connected now (the host holds the rows it last saw): the transcript
    /// still names the session from it, but the `+` picker leaves it out.
    pub offline: bool,
}

impl KnownSession {
    /// Whether two lists name the same sessions the same way, so the transcript's cards and pills
    /// would read the same: everything but the `+` picker's live fields, in any order.
    ///
    /// CDXC:SessionChat 2026-11-06 WHY: User: "bro why is the app laggy now??". The list comes in
    /// the sidebar's order, which moves whenever a session becomes the newest, and comparing it in
    /// order bumped `session_directory_revision` on every such move. That revision is part of the
    /// settled prefix's key, so each open chat settled its whole transcript again: 20 seconds of
    /// the chat host thread on a 14,828-row coordinator chat, over and over while its threads
    /// worked, and every paste waited behind it. A reordered list names every session as before
    /// (`host_lists_name_alike`), so it rebuilds nothing.
    pub fn same_names(a: &[KnownSession], b: &[KnownSession]) -> bool {
        (a.len() == b.len() && a.iter().zip(b).all(|(a, b)| Self::named_alike(a, b)))
            || crate::transcript::sent_message::host_lists_name_alike(a, b)
    }

    /// The fields the transcript names a session by.
    pub(crate) fn named_alike(a: &KnownSession, b: &KnownSession) -> bool {
        a.global_ref == b.global_ref
            && a.project_id == b.project_id
            && a.session_id == b.session_id
            && a.title == b.title
            && a.agent_id == b.agent_id
            && a.agent_name == b.agent_name
            && a.coordinator == b.coordinator
    }
}

/// What the host knows when it opens a chat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    /// All three default, because the host does not always know them at boot: the desktop host
    /// learns the client id from its own storage read.
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
    /// `JSON.stringify([machineId, projectId, sessionId])`, the key of this session's retained
    /// transcript record.
    ///
    /// The host builds it with [`crate::session::persistence::storage_key`], because only it knows
    /// the machine id, and the core writes it back inside the record. Empty when the host retains
    /// nothing.
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
    /// The Simple override, or absent to follow the setting (`state/transcript_view.rs`).
    #[serde(default)]
    pub simple_override: Value,
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
    /// The Catch me up setting (`chatCatchMeUp`); absent from a host that has no settings file
    /// (the phone), which learns it from the chat read instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catch_up: Option<bool>,
    /// Whether the Session details panel built-in extension is on; absent from a host that has no
    /// settings file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_details: Option<bool>,
    /// The Send when you pick an answer setting (`chatSendOnPick`), the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_on_pick: Option<bool>,
    /// Whether answers wait for the agent's turn to end; the phone learns this from each read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers_wait_for_idle: Option<bool>,
    /// The Show all effort levels setting (`orchestratorAllEfforts`), the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_orchestrator_efforts: Option<bool>,
    /// The Handover heading color strength setting (`markedHandoversHeadingStrength`, 0 to 100),
    /// the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marked_heading_strength: Option<f64>,
    /// The Text strength setting (`sessionChatTextStrengthPercent`, 0 to 100), the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_text_strength: Option<f64>,
    /// The Tool call cards setting (`chatToolCallCards`), the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_cards: Option<bool>,
    /// The Default to Detailed view setting (`sessionChatDetailedView`): the level a chat with no
    /// level of its own opens in (`transcript::view_level`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detailed_default: Option<bool>,
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
