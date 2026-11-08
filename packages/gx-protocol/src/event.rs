//! The `/api/events` server frame envelope.

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::chat::{ChatAppendedFrame, ChatSnapshotFrame, ChatStateFrame};
use crate::delta::PresentationDelta;
use crate::peek::peek_event_type;
use crate::presentation::PresentationSnapshot;
use crate::side_state::{
    CustomSessionTagsState, SidebarProjectCollectionsState, SidebarSpacesState,
    SidebarWorkspacesState, WorkspaceSessionGroupsState,
};

/// Keys every non-chat server frame carries next to `type`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventHeader {
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub protocol_version: u64,
    /// Daemon identity; a change means another daemon instance or machine.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub server_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRequestHandledFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub path: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub request_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSnapshotFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    pub revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub snapshot: Box<PresentationSnapshot>,
}

/// The subscribe quoted the daemon's current revision: nothing to apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSnapshotCurrentFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    pub revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationDeltaFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    pub revision: i64,
    pub delta: PresentationDelta,
}

/// Side-channel frames share the presentation revision counter. `revision` is optional here only
/// because a daemon of another version may not send it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGroupsChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    pub groups: WorkspaceSessionGroupsState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarProjectCollectionsChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    pub sidebar_project_collections: SidebarProjectCollectionsState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSpacesChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    pub sidebar_spaces: SidebarSpacesState,
    /// The daemon's own Spaces switch, sent with every Spaces frame and on its own when only the
    /// switch moved. Absent on an older daemon, which leaves the client's copy alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_spaces_enabled: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarWorkspacesChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    pub sidebar_workspaces: SidebarWorkspacesState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSessionTagsChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    pub custom_session_tags: CustomSessionTagsState,
}

/// No payload: refetch `/api/readSidebarHud`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSidebarCommandsChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
}

/// No payload but the workspace: refetch `/api/readConnectors` (a sign-in, an app, Executor's
/// state or a workspace's rules changed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorsChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// A settings sync conflict on this computer: two computers changed one thing apart and the later
/// edit won (server/src/settings_sync/). `note` is the note as Settings > Sync lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSyncNoteFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    pub note: Value,
}

/// `approvalRequestsChanged`: an approval request was made, approved, denied or expired
/// (`crate::approval`, server/src/server/security_http.rs). The desktop shows a new one as a card
/// with Approve and Deny, and drops the card when it is decided elsewhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestsChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    /// `requested`, `approved`, `denied`, `expired`.
    #[serde(default)]
    pub change: String,
    #[serde(default)]
    pub request: Option<crate::approval::ApprovalRequest>,
}

/// A command the daemon dispatches to the registered renderer socket. It must be answered with a
/// `rendererCommandResult` before `timeout_ms`, or the originating RPC fails with
/// `dependencyUnavailable`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererCommand {
    pub action: String,
    pub command_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub created_at: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererCommandFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    pub command: RendererCommand,
}

/// One server frame on an `/api/events` socket.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    /// First frame on every socket.
    EventStreamReady(EventHeader),
    ServerStarted(EventHeader),
    ServerStopping(EventHeader),
    /// One per HTTP request served to any client; the highest-volume frame on a full stream.
    ApiRequestHandled(ApiRequestHandledFrame),
    PresentationSnapshot(PresentationSnapshotFrame),
    PresentationSnapshotCurrent(PresentationSnapshotCurrentFrame),
    PresentationDelta(PresentationDeltaFrame),
    WorkspaceGroupsChanged(WorkspaceGroupsChangedFrame),
    SidebarProjectCollectionsChanged(SidebarProjectCollectionsChangedFrame),
    SidebarSpacesChanged(SidebarSpacesChangedFrame),
    SidebarWorkspacesChanged(SidebarWorkspacesChangedFrame),
    CustomSessionTagsChanged(CustomSessionTagsChangedFrame),
    GlobalSidebarCommandsChanged(GlobalSidebarCommandsChangedFrame),
    /// No payload and no revision: refetch `/api/readNotificationFeed`.
    NotificationFeedChanged(EventHeader),
    ConnectorsChanged(ConnectorsChangedFrame),
    SettingsSyncNote(SettingsSyncNoteFrame),
    ApprovalRequestsChanged(Box<ApprovalRequestsChangedFrame>),
    CloudBoxesChanged(Box<crate::cloud_box::CloudBoxesChangedFrame>),
    RendererCommand(RendererCommandFrame),
    SessionChatSnapshot(Box<ChatSnapshotFrame>),
    SessionChatReplaced(Box<ChatSnapshotFrame>),
    SessionChatAppended(Box<ChatAppendedFrame>),
    SessionChatState(Box<ChatStateFrame>),
    /// A frame type this client does not know (for example `agentSkillStatus`). Not an error.
    Unknown {
        event_type: String,
    },
}

/// Why a frame could not be parsed.
#[derive(Debug)]
pub enum EventParseError {
    /// The text is not a JSON object with a string `type`.
    MissingType,
    /// The frame has a known `type` but does not match that type's shape.
    Shape {
        event_type: String,
        error: serde_json::Error,
    },
}

impl EventParseError {
    /// The frame's `type`, when it could be read.
    pub fn event_type(&self) -> Option<&str> {
        match self {
            Self::MissingType => None,
            Self::Shape { event_type, .. } => Some(event_type.as_str()),
        }
    }
}

impl std::fmt::Display for EventParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingType => formatter.write_str("gxserver frame has no string `type`"),
            Self::Shape { event_type, error } => {
                write!(
                    formatter,
                    "gxserver `{event_type}` frame is malformed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for EventParseError {}

/// Frame types a client may drop without parsing when it has no use for them.
pub const EVENT_TYPE_API_REQUEST_HANDLED: &str = "apiRequestHandled";

impl ServerEvent {
    /// The wire `type` of this frame.
    pub fn event_type(&self) -> &str {
        match self {
            Self::EventStreamReady(_) => "eventStreamReady",
            Self::ServerStarted(_) => "serverStarted",
            Self::ServerStopping(_) => "serverStopping",
            Self::ApiRequestHandled(_) => EVENT_TYPE_API_REQUEST_HANDLED,
            Self::PresentationSnapshot(_) => "presentationSnapshot",
            Self::PresentationSnapshotCurrent(_) => "presentationSnapshotCurrent",
            Self::PresentationDelta(_) => "presentationDelta",
            Self::WorkspaceGroupsChanged(_) => "workspaceGroupsChanged",
            Self::SidebarProjectCollectionsChanged(_) => "sidebarProjectCollectionsChanged",
            Self::SidebarSpacesChanged(_) => "sidebarSpacesChanged",
            Self::SidebarWorkspacesChanged(_) => "sidebarWorkspacesChanged",
            Self::CustomSessionTagsChanged(_) => "customSessionTagsChanged",
            Self::GlobalSidebarCommandsChanged(_) => "globalSidebarCommandsChanged",
            Self::NotificationFeedChanged(_) => "notificationFeedChanged",
            Self::ConnectorsChanged(_) => "connectorsChanged",
            Self::SettingsSyncNote(_) => "settingsSyncNote",
            Self::ApprovalRequestsChanged(_) => "approvalRequestsChanged",
            Self::CloudBoxesChanged(_) => crate::cloud_box::CLOUD_BOXES_CHANGED_EVENT_TYPE,
            Self::RendererCommand(_) => "rendererCommand",
            Self::SessionChatSnapshot(_) => "sessionChatSnapshot",
            Self::SessionChatReplaced(_) => "sessionChatReplaced",
            Self::SessionChatAppended(_) => "sessionChatAppended",
            Self::SessionChatState(_) => "sessionChatState",
            Self::Unknown { event_type } => event_type.as_str(),
        }
    }

    /// The `protocolVersion` the frame carries; `None` for a frame type this client does not
    /// know. A client applies a frame only when this equals
    /// [`crate::GXSERVER_PROTOCOL_VERSION`].
    pub fn protocol_version(&self) -> Option<u64> {
        let header = match self {
            Self::EventStreamReady(header)
            | Self::ServerStarted(header)
            | Self::ServerStopping(header)
            | Self::NotificationFeedChanged(header) => header,
            Self::ApiRequestHandled(frame) => &frame.header,
            Self::PresentationSnapshot(frame) => &frame.header,
            Self::PresentationSnapshotCurrent(frame) => &frame.header,
            Self::PresentationDelta(frame) => &frame.header,
            Self::WorkspaceGroupsChanged(frame) => &frame.header,
            Self::SidebarProjectCollectionsChanged(frame) => &frame.header,
            Self::SidebarSpacesChanged(frame) => &frame.header,
            Self::SidebarWorkspacesChanged(frame) => &frame.header,
            Self::CustomSessionTagsChanged(frame) => &frame.header,
            Self::GlobalSidebarCommandsChanged(frame) => &frame.header,
            Self::ConnectorsChanged(frame) => &frame.header,
            Self::SettingsSyncNote(frame) => &frame.header,
            Self::ApprovalRequestsChanged(frame) => &frame.header,
            Self::CloudBoxesChanged(frame) => &frame.header,
            Self::RendererCommand(frame) => &frame.header,
            Self::SessionChatSnapshot(frame) | Self::SessionChatReplaced(frame) => {
                return Some(frame.base.protocol_version)
            }
            Self::SessionChatAppended(frame) => return Some(frame.base.protocol_version),
            Self::SessionChatState(frame) => return Some(frame.base.protocol_version),
            Self::Unknown { .. } => return None,
        };
        Some(header.protocol_version)
    }

    /// Parses one raw frame. Peeks `type` first and then parses straight into that frame's
    /// struct, so no intermediate value tree is built for the envelope.
    pub fn parse(frame: &str) -> Result<Self, EventParseError> {
        match peek_event_type(frame) {
            Some(event_type) => Self::parse_typed(event_type, TextSource(frame)),
            // The peek refuses escaped type strings and non-objects; let the full parser decide.
            None => {
                let value: Value =
                    serde_json::from_str(frame).map_err(|_| EventParseError::MissingType)?;
                Self::from_value(value)
            }
        }
    }

    /// Parses a frame that is already a JSON value.
    pub fn from_value(value: Value) -> Result<Self, EventParseError> {
        let event_type = match value.get("type").and_then(Value::as_str) {
            Some(event_type) => event_type.to_string(),
            None => return Err(EventParseError::MissingType),
        };
        Self::parse_typed(&event_type, ValueSource(value))
    }

    fn parse_typed<S: FrameSource>(event_type: &str, source: S) -> Result<Self, EventParseError> {
        let parsed = match event_type {
            "eventStreamReady" => source.read().map(Self::EventStreamReady),
            "serverStarted" => source.read().map(Self::ServerStarted),
            "serverStopping" => source.read().map(Self::ServerStopping),
            EVENT_TYPE_API_REQUEST_HANDLED => source.read().map(Self::ApiRequestHandled),
            "presentationSnapshot" => source.read().map(Self::PresentationSnapshot),
            "presentationSnapshotCurrent" => source.read().map(Self::PresentationSnapshotCurrent),
            "presentationDelta" => source.read().map(Self::PresentationDelta),
            "workspaceGroupsChanged" => source.read().map(Self::WorkspaceGroupsChanged),
            "sidebarProjectCollectionsChanged" => {
                source.read().map(Self::SidebarProjectCollectionsChanged)
            }
            "sidebarSpacesChanged" => source.read().map(Self::SidebarSpacesChanged),
            "sidebarWorkspacesChanged" => source.read().map(Self::SidebarWorkspacesChanged),
            "customSessionTagsChanged" => source.read().map(Self::CustomSessionTagsChanged),
            "globalSidebarCommandsChanged" => source.read().map(Self::GlobalSidebarCommandsChanged),
            "notificationFeedChanged" => source.read().map(Self::NotificationFeedChanged),
            "connectorsChanged" => source.read().map(Self::ConnectorsChanged),
            "settingsSyncNote" => source.read().map(Self::SettingsSyncNote),
            "approvalRequestsChanged" => source.read().map(Self::ApprovalRequestsChanged),
            crate::cloud_box::CLOUD_BOXES_CHANGED_EVENT_TYPE => {
                source.read().map(Self::CloudBoxesChanged)
            }
            "rendererCommand" => source.read().map(Self::RendererCommand),
            "sessionChatSnapshot" => source.read().map(Self::SessionChatSnapshot),
            "sessionChatReplaced" => source.read().map(Self::SessionChatReplaced),
            "sessionChatAppended" => source.read().map(Self::SessionChatAppended),
            "sessionChatState" => source.read().map(Self::SessionChatState),
            _ => {
                return Ok(Self::Unknown {
                    event_type: event_type.to_string(),
                })
            }
        };
        parsed.map_err(|error| EventParseError::Shape {
            event_type: event_type.to_string(),
            error,
        })
    }
}

/// Where a frame body is read from: raw text (one pass) or an already-built value.
trait FrameSource {
    fn read<T: DeserializeOwned>(self) -> Result<T, serde_json::Error>;
}

struct TextSource<'a>(&'a str);

impl FrameSource for TextSource<'_> {
    fn read<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        serde_json::from_str(self.0)
    }
}

struct ValueSource(Value);

impl FrameSource for ValueSource {
    fn read<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.0)
    }
}

impl<'de> Deserialize<'de> for ServerEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(D::Error::custom)
    }
}
