//! The JSON a phone host sends for one [`Event`].
//!
//! `gx-chat-core`'s `Event` carries no serde on purpose (it is the Rust hosts' typed input), so the
//! mobile boundary spells it here: one object tagged by `type`, camelCase, every payload in the
//! shape the core's own serde types already use. The TypeScript mirror is `ChatCoreEvent` in
//! `apps/mobile/app/src/chat/rust/core-types.ts`; keep the two in lockstep.

use ghostex_gx_chat_core::protocol::{ChatAppendedFrame, ChatSnapshotFrame, ChatStateFrame};
use ghostex_gx_chat_core::{
    ChatFrame, ChatSettings, ComposerBootRead, ConnectionUpdate, Event, KnownSession, Measurement,
    OpenRowDetail, ReadAloudEvent, RpcOutcome, StartConfig, StorageKey, StorageRecord, UserAction,
};
use serde::Deserialize;
use serde_json::Value;

/// Parsed once and moved straight into an `Event`, so the variant size spread costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum EventJson {
    Start {
        config: StartConfig,
    },
    Frame {
        frame: Value,
    },
    Connection {
        update: ConnectionUpdate,
    },
    RpcSettled {
        request_id: u64,
        #[serde(default)]
        result: Value,
        #[serde(default)]
        error: Option<RpcErrorJson>,
    },
    Action {
        action: UserAction,
    },
    Tick,
    StorageLoaded {
        key: StorageKey,
        #[serde(default)]
        value: Option<String>,
    },
    StorageWritten {
        key: StorageKey,
        #[serde(default)]
        error: Option<String>,
    },
    StorageBatchLoaded {
        records: Vec<StorageRecord>,
    },
    StorageBatchWritten {
        keys: Vec<StorageKey>,
        #[serde(default)]
        error: Option<String>,
    },
    RetainedSnapshotLoaded {
        #[serde(default)]
        value: Option<String>,
    },
    ComposerBootRead {
        read: ComposerBootRead,
    },
    ComposerBootFailed {
        error: String,
    },
    SettingsChanged {
        settings: ChatSettings,
    },
    ContextPreferencesChanged {
        provider: String,
        #[serde(default)]
        preferences: Value,
    },
    ModelCatalogChanged {
        #[serde(default)]
        catalog: Value,
    },
    Measured {
        measurement: MeasurementJson,
    },
    DraftChanged {
        text: String,
    },
    SessionDirectoryChanged {
        sessions: Vec<KnownSession>,
    },
    /// `{"type": "readAloud", "event": {"kind": "setAvailable", "on": true}}`: the core's own
    /// serde form of a Read aloud input, already camelCase.
    ReadAloud {
        event: ReadAloudEvent,
    },
    /// `{"type": "visibility", "shown": false}` while the app is in the background: the chat's
    /// clocks and accounts poll wait until it is shown again.
    Visibility {
        shown: bool,
    },
}

#[derive(Deserialize)]
struct RpcErrorJson {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
}

/// `Measurement` spelled with camelCase fields; the core's own serde form keeps the struct
/// variants' fields in snake case, which no JavaScript caller should have to write.
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum MeasurementJson {
    ComposerOverflow {
        overflowed: Vec<String>,
        options_overflowed: bool,
    },
    OpenRowDetails {
        rows: Vec<OpenRowDetail>,
    },
}

/// Parses one event, or says why it is not one.
pub(crate) fn parse(json: &str) -> Result<Event, String> {
    let parsed: EventJson = serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(match parsed {
        EventJson::Start { config } => Event::Start(Box::new(config)),
        EventJson::Frame { frame } => Event::Frame(Box::new(chat_frame(frame)?)),
        EventJson::Connection { update } => Event::Connection(update),
        EventJson::RpcSettled {
            request_id,
            result,
            error,
        } => Event::RpcSettled {
            request_id,
            outcome: Box::new(match error {
                Some(error) => RpcOutcome::Err {
                    code: error.code,
                    // The desktop host's wording for a refusal that carried no message.
                    message: error
                        .message
                        .unwrap_or_else(|| "Request failed.".to_string()),
                    endpoint: error.endpoint,
                },
                None => RpcOutcome::Ok { result },
            }),
        },
        EventJson::Action { action } => Event::Action(Box::new(action)),
        EventJson::Tick => Event::Tick,
        EventJson::StorageLoaded { key, value } => Event::StorageLoaded { key, value },
        EventJson::StorageWritten { key, error } => Event::StorageWritten { key, error },
        EventJson::StorageBatchLoaded { records } => Event::StorageBatchLoaded { records },
        EventJson::StorageBatchWritten { keys, error } => {
            Event::StorageBatchWritten { keys, error }
        }
        EventJson::RetainedSnapshotLoaded { value } => Event::RetainedSnapshotLoaded { value },
        EventJson::ComposerBootRead { read } => Event::ComposerBootRead(Box::new(read)),
        EventJson::ComposerBootFailed { error } => Event::ComposerBootFailed { error },
        EventJson::SettingsChanged { settings } => Event::SettingsChanged(Box::new(settings)),
        EventJson::ContextPreferencesChanged {
            provider,
            preferences,
        } => Event::ContextPreferencesChanged {
            provider,
            preferences,
        },
        EventJson::ModelCatalogChanged { catalog } => Event::ModelCatalogChanged { catalog },
        EventJson::Measured { measurement } => Event::Measured(match measurement {
            MeasurementJson::ComposerOverflow {
                overflowed,
                options_overflowed,
            } => Measurement::ComposerOverflow {
                overflowed,
                options_overflowed,
            },
            MeasurementJson::OpenRowDetails { rows } => Measurement::OpenRowDetails { rows },
        }),
        EventJson::DraftChanged { text } => Event::DraftChanged { text },
        EventJson::SessionDirectoryChanged { sessions } => {
            Event::SessionDirectoryChanged { sessions }
        }
        EventJson::ReadAloud { event } => Event::ReadAloud(event),
        EventJson::Visibility { shown } => Event::Visibility { shown },
    })
}

/// One of the four frame types the chat socket admits, parsed the way the desktop host parses it
/// (`apps/desktop/src/app/gx_chat/events.rs`): a frame missing a required field is refused.
fn chat_frame(value: Value) -> Result<ChatFrame, String> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let invalid = |error: serde_json::Error| format!("{kind}: {error}");
    Ok(match kind.as_str() {
        "sessionChatSnapshot" => ChatFrame::Snapshot(Box::new(
            serde_json::from_value::<ChatSnapshotFrame>(value).map_err(invalid)?,
        )),
        "sessionChatReplaced" => ChatFrame::Replaced(Box::new(
            serde_json::from_value::<ChatSnapshotFrame>(value).map_err(invalid)?,
        )),
        "sessionChatAppended" => ChatFrame::Appended(Box::new(
            serde_json::from_value::<ChatAppendedFrame>(value).map_err(invalid)?,
        )),
        "sessionChatState" => ChatFrame::State(Box::new(
            serde_json::from_value::<ChatStateFrame>(value).map_err(invalid)?,
        )),
        other => return Err(format!("unknown chat frame type {other:?}")),
    })
}
