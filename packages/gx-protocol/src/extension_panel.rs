//! Sidebar and side panel extension panels: the panel document an extension sends, and the
//! `extensionPanels` field of the presentation snapshot that carries every panel to clients.
//!
//! gxserver owns each panel (server/src/extensions/panel.rs): it validates a document strictly
//! when an extension writes it, keeps its revision, saves it and applies editable-checklist edits
//! itself. Clients only draw what arrives here and send taps back through
//! `POST /api/ext/v1/panelAction`. Parsing is lenient on the client side: a panel this client
//! cannot read is left out (see [`ExtensionPanels`]), so a newer daemon's panel never fails the
//! snapshot that carries it.
//!
//! SEE-ALSO: schemas/ghostex-sidebar-panel.schema.json in the Ghostex-extensions repo (the same
//! document for authors), packages/shared/ghostex-extensions.ts (TypeScript types).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The panel document schema this build reads and writes.
pub const EXTENSION_PANEL_SCHEMA: u64 = 1;
/// The frame gxserver broadcasts with the whole `extensionPanels` map whenever a panel changes.
pub const EXTENSION_PANELS_CHANGED_EVENT_TYPE: &str = "extensionPanelsChanged";

/// What a sidebar mode, a section or a side panel draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelDocument {
    pub schema: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Tabs across the top of a sidebar mode or side panel; a block names its tab with `tab`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabs: Option<Vec<PanelTab>>,
    pub blocks: Vec<PanelBlock>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelTab {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelBlock {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(flatten)]
    pub body: PanelBlockBody,
}

/// The v1 blocks. `web` is reserved for a later version; gxserver refuses it today.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PanelBlockBody {
    Rows {
        items: Vec<PanelRow>,
    },
    Checklist {
        /// The user may tick, add, rename and delete items; gxserver applies those edits itself.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        editable: Option<bool>,
        items: Vec<ChecklistItem>,
    },
    /// Grouped items under a root; becomes an indented tree where the space is narrow.
    Map {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<MapRoot>,
        groups: Vec<MapGroup>,
    },
    Progress {
        value: f64,
        /// 100 when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// A small Markdown subset (paragraphs, bold, italic, code, links, lists).
    Text {
        text: String,
    },
    Buttons {
        items: Vec<PanelButton>,
    },
    /// An empty state; its words are the block's `title` (required here) and `subtitle`.
    Empty {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<PanelAction>,
    },
    /// Reserved: a small page of the extension's own (phase 4).
    Web {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
    },
    /// A block type this client does not know; drawn as nothing.
    #[serde(other)]
    Unknown,
}

impl PanelBlockBody {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Rows { .. } => "rows",
            Self::Checklist { .. } => "checklist",
            Self::Map { .. } => "map",
            Self::Progress { .. } => "progress",
            Self::Text { .. } => "text",
            Self::Buttons { .. } => "buttons",
            Self::Empty { .. } => "empty",
            Self::Web { .. } => "web",
            Self::Unknown => "unknown",
        }
    }
}

/// A row of a `rows` block or an item of a `map` group. With `project` or `session`, Ghostex
/// fills in the name, live status dot, hover card and menu itself, and a click opens it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// `green`, `yellow`, `red`, `blue` or `gray`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    /// A project path (`~/dev/app`) or project id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// A session ref (`<projectId>:<sessionId>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<PanelAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<Vec<PanelMenuItem>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub id: String,
    pub text: String,
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<PanelAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<Vec<PanelMenuItem>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRoot {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapGroup {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub items: Vec<PanelRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelButton {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// `primary` or `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    pub action: PanelAction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelMenuItem {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub action: PanelAction,
}

/// What a tap does. Built-in actions run in the client, checked against the permissions the user
/// granted (`grantedPermissions` beside the panel); `extension` goes back to the extension as a
/// `panelAction` event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanelAction {
    /// A project path or id.
    OpenProject(String),
    /// A session ref (`<projectId>:<sessionId>`).
    FocusSession(String),
    NewAgent(NewAgentAction),
    /// An `https://` (or loopback `http://`) address, opened in the browser.
    OpenUrl(String),
    /// A Ghostex view id.
    OpenView(String),
    Extension(ExtensionActionTarget),
}

impl PanelAction {
    /// The scoped permission a client checks before it runs this action.
    pub fn required_permission(&self) -> Option<&'static str> {
        match self {
            Self::OpenProject(_) | Self::FocusSession(_) => Some("sessions.open"),
            Self::NewAgent(_) => Some("agents.start"),
            Self::OpenUrl(_) | Self::OpenView(_) | Self::Extension(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAgentAction {
    pub project: String,
    /// Text put in the new agent's chat box, not sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// An agent id (`claude`, `codex`, …); the user's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtensionActionTarget {
    Name(String),
    Detailed {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
}

impl ExtensionActionTarget {
    pub fn name(&self) -> &str {
        match self {
            Self::Name(name) | Self::Detailed { name, .. } => name,
        }
    }
}

/// Where a sidebar extension's section sits in the Projects list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SidebarSectionPlace {
    ProjectsTop,
    ProjectsBottom,
}

/// The `extensionPanels` field: every enabled sidebar extension's panels, by extension id.
/// Absent while no sidebar extension is on, and on older daemons. Read entry by entry: an entry
/// this client cannot parse is left out, never failing the snapshot.
///
/// CDXC:Extensions 2026-11-14 DECISION:
/// Q27, user: "just desktop and mobile for now please. not needed for web for now". The desktop and the phone draw these panels; the GPUI web build (`ghostex web`) does not show them for now, though it compiles the same sidebar files and must keep building.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExtensionPanels(pub BTreeMap<String, ExtensionPanelEntry>);

impl<'de> Deserialize<'de> for ExtensionPanels {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let Value::Object(entries) = raw else {
            return Ok(Self::default());
        };
        Ok(Self(
            entries
                .into_iter()
                .filter_map(|(id, entry)| {
                    serde_json::from_value::<ExtensionPanelEntry>(entry)
                        .ok()
                        .map(|entry| (id, entry))
                })
                .collect(),
        ))
    }
}

/// One sidebar extension as clients draw it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPanelEntry {
    pub title: String,
    /// The extension's icon, served by gxserver (`/ext/<id>/<icon>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The scoped permissions the user granted; a client runs a built-in action only when its
    /// [`PanelAction::required_permission`] is here.
    #[serde(default)]
    pub granted_permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<ExtensionSidebarPanel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_panel: Option<ExtensionSidePanel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSidebarPanel {
    /// Its own sidebar mode (a top-row button that swaps the whole list, like Bots).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ExtensionSidebarMode>,
    /// Blocks of the document shown inside the Projects list.
    #[serde(default)]
    pub sections: Vec<ExtensionSidebarSection>,
    pub revision: u64,
    pub document: PanelDocument,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSidebarMode {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSidebarSection {
    pub block: String,
    #[serde(rename = "where")]
    pub place: SidebarSectionPlace,
}

/// Whether a side panel view shows one document for the computer or one per session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SidePanelScope {
    Global,
    Session,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSidePanel {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub scope: SidePanelScope,
    /// The computer-wide document (`scope: global`), or the template a session starts from.
    pub revision: u64,
    pub document: PanelDocument,
    /// `scope: session`: the documents written for sessions, by `<projectId>:<sessionId>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sessions: BTreeMap<String, ExtensionSessionPanel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSessionPanel {
    pub revision: u64,
    pub document: PanelDocument,
}

/// `extensionPanelsChanged`: the whole map, sent whenever a panel or a sidebar extension changes.
/// It carries state, not a delta, so it needs no presentation revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPanelsChangedFrame {
    #[serde(flatten)]
    pub header: crate::event::EventHeader,
    pub extension_panels: ExtensionPanels,
}
