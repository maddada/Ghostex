use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionManifest {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub icon: String,
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<ExtensionPreference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<ExtensionPermission>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placements: Vec<ExtensionPlacement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_placement: Option<ExtensionPlacement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ExtensionServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ExtensionKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<ExtensionTerminal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modal: Option<ExtensionSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popup: Option<ExtensionSize>,
    /// `kind: "sidebar"`: its own sidebar mode and/or sections in the Projects list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<ExtensionSidebarManifest>,
    /// `kind: "sidebar"`: a side panel view drawn from the same panel blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_panel: Option<ExtensionSidePanelManifest>,
}

/// CDXC:Extensions 2026-11-14 DECISION:
/// Sidebar extensions (Q23, user: native blocks plus a web block, option C): an extension sends a JSON panel document and Ghostex draws it with its own rows, menus and tooltips; a `web` block for what blocks can't draw comes later. Q24: a sidebar extension gets its own sidebar mode (a top-row button that swaps the whole list, like Bots); sections inside the Projects list are optional in the manifest. User: "I want to be able to use the same thing for the side panel views please (use the extensions api to get stuff from agent etc)", so `sidePanel` draws the same blocks from the same API, one document for the computer or one per session.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSidebarManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ExtensionSidebarModeManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<ExtensionSidebarSectionManifest>,
    /// The first panel document, a JSON file in the payload. An empty panel without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSidebarModeManifest {
    pub title: String,
    /// An SVG in the payload; the extension's icon when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSidebarSectionManifest {
    /// The id of a block in the sidebar panel document.
    pub block: String,
    #[serde(rename = "where")]
    pub place: ghostex_gx_protocol::extension_panel::SidebarSectionPlace,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSidePanelManifest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// `global` (one document) when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<ghostex_gx_protocol::extension_panel::SidePanelScope>,
    /// The first document (and, for `scope: session`, the one a session starts from).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionPlacement {
    View,
    ChatBar,
    Popup,
    Modal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionPermission {
    Exec,
    Cli,
    Ssh,
    Network,
    Clipboard,
    /// CDXC:Extensions 2026-11-14 DECISION:
    /// Q26 (user picked new scoped permissions over reusing `cli`): `cli` runs the whole CLI, including closing sessions and typing into agents, which is too much for a sidebar that only shows your projects. These four reach only `/api/ext/v1` (ext_api.rs) and the panel actions clients run (`PanelAction::required_permission` in gx-protocol), and the install card lists each one.
    #[serde(rename = "projects.read")]
    ProjectsRead,
    #[serde(rename = "sessions.read")]
    SessionsRead,
    #[serde(rename = "sessions.open")]
    SessionsOpen,
    #[serde(rename = "agents.start")]
    AgentsStart,
}

impl ExtensionPermission {
    /// The wire name (`projects.read`, `exec`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Cli => "cli",
            Self::Ssh => "ssh",
            Self::Network => "network",
            Self::Clipboard => "clipboard",
            Self::ProjectsRead => "projects.read",
            Self::SessionsRead => "sessions.read",
            Self::SessionsOpen => "sessions.open",
            Self::AgentsStart => "agents.start",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        serde_json::from_value(Value::String(value.to_string())).ok()
    }
}

/// What the user agreed to in the install consent: the version and the capabilities it listed.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionInstallApproval {
    pub version: String,
    #[serde(default)]
    pub permissions: Vec<ExtensionPermission>,
    #[serde(default)]
    pub background_process: bool,
    #[serde(default)]
    pub remote_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionKind {
    TerminalPane,
    Sidebar,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionServer {
    #[serde(rename = "static", skip_serializing_if = "Option::is_none")]
    pub static_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ExtensionReadiness>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub install: BTreeMap<String, ExtensionPlatformInstall>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionReadiness {
    pub http_get: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPlatformInstall {
    pub url: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionTerminal {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPreference {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "type")]
    pub preference_type: ExtensionPreferenceType,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<ExtensionPreferenceOption>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionPreferenceType {
    Textfield,
    Password,
    Checkbox,
    Dropdown,
    File,
    Directory,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtensionPreferenceOption {
    pub title: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ExtensionSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStoreEntry {
    pub enabled: bool,
    pub pinned: bool,
    #[serde(default)]
    pub chat_bar_auto_open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<ExtensionPlacement>,
    pub terminal_placement: ExtensionTerminalPlacement,
    #[serde(default)]
    pub preferences: BTreeMap<String, Value>,
    #[serde(default)]
    pub storage: BTreeMap<String, Value>,
    pub version: String,
    #[serde(default)]
    pub granted_permissions: Vec<ExtensionPermission>,
    /// A sidebar extension's own sidebar mode is turned off in Settings > Extensions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sidebar_mode_hidden: bool,
    /// A sidebar extension's sections in the Projects list are turned off in Settings > Extensions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sidebar_sections_hidden: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionTerminalPlacement {
    SplitRight,
    Tab,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStatePatch {
    pub enabled: Option<bool>,
    pub pinned: Option<bool>,
    pub chat_bar_auto_open: Option<bool>,
    pub placement: Option<ExtensionPlacement>,
    pub terminal_placement: Option<ExtensionTerminalPlacement>,
    pub preferences: Option<BTreeMap<String, Value>>,
    pub storage: Option<BTreeMap<String, Value>>,
    pub granted_permissions: Option<Vec<ExtensionPermission>>,
    pub sidebar_mode_hidden: Option<bool>,
    pub sidebar_sections_hidden: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExtension {
    pub id: String,
    pub manifest: ExtensionManifest,
    pub state: ExtensionStoreEntry,
    pub runtime: ExtensionRuntimeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<ExtensionBadge>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionRuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionRuntimeStatus {
    pub state: ExtensionRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ExtensionRuntimeStatus {
    pub(crate) fn stopped() -> Self {
        Self {
            state: ExtensionRuntimeState::Stopped,
            url: None,
            pid: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionBadge {
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionLaunchContext {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub project_path: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub worktree: bool,
    #[serde(default)]
    pub worktree_branch: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionCatalog {
    pub schema_version: u64,
    pub published_at: String,
    pub extensions: Vec<ExtensionCatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtensionCatalogEntry {
    #[serde(flatten)]
    pub manifest: ExtensionManifest,
    pub readme: String,
    pub changelog: String,
    #[serde(default)]
    pub screenshots: Vec<String>,
    pub zip: String,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionCatalogSource {
    Remote,
    Cache,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionCatalogSnapshot {
    pub catalog: ExtensionCatalog,
    pub source: ExtensionCatalogSource,
    pub url: String,
}

#[derive(Debug)]
pub struct ExtensionError {
    pub code: &'static str,
    pub message: String,
}

impl ExtensionError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internalError",
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtensionError {}

pub type ExtensionResult<T> = Result<T, ExtensionError>;

impl ExtensionManifest {
    pub fn validate(
        &self,
        payload_dir: Option<&Path>,
        expected_id: Option<&str>,
    ) -> ExtensionResult<()> {
        if !valid_extension_id(&self.name) {
            return Err(ExtensionError::bad_request(format!(
                "Extension name {:?} must be a kebab-case id.",
                self.name
            )));
        }
        if expected_id.is_some_and(|expected| expected != self.name) {
            return Err(ExtensionError::bad_request(format!(
                "Manifest name {:?} does not match extension id {:?}.",
                self.name,
                expected_id.unwrap_or_default()
            )));
        }
        for (field, value) in [
            ("title", self.title.as_str()),
            ("description", self.description.as_str()),
            ("author", self.author.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ExtensionError::bad_request(format!(
                    "Extension {} must not be empty.",
                    field
                )));
            }
        }
        if !valid_semver(&self.version) {
            return Err(ExtensionError::bad_request(format!(
                "Extension version {:?} is not valid semantic versioning.",
                self.version
            )));
        }
        if self.categories.is_empty() || self.categories.iter().any(|value| value.trim().is_empty())
        {
            return Err(ExtensionError::bad_request(
                "Extension categories must contain at least one non-empty value.",
            ));
        }
        validate_relative_path(&self.icon, "icon")?;
        if !self.icon.to_ascii_lowercase().ends_with(".svg") {
            return Err(ExtensionError::bad_request(
                "Extension icon must reference an SVG file.",
            ));
        }
        self.validate_shape()?;
        self.validate_preferences()?;
        if let Some(payload_dir) = payload_dir {
            let icon_path = payload_dir.join(&self.icon);
            if !icon_path.is_file() {
                return Err(ExtensionError::bad_request(format!(
                    "Extension icon does not exist: {}",
                    icon_path.display()
                )));
            }
            if let Some(static_dir) = self
                .server
                .as_ref()
                .and_then(|server| server.static_dir.as_ref())
            {
                let static_path = payload_dir.join(static_dir);
                if !static_path.is_dir() {
                    return Err(ExtensionError::bad_request(format!(
                        "Extension static directory does not exist: {}",
                        static_path.display()
                    )));
                }
            }
            if self.kind == Some(ExtensionKind::Sidebar) {
                self.validate_sidebar_payload(payload_dir)?;
            }
        }
        Ok(())
    }

    fn validate_shape(&self) -> ExtensionResult<()> {
        if self.kind != Some(ExtensionKind::Sidebar)
            && (self.sidebar.is_some() || self.side_panel.is_some())
        {
            return Err(ExtensionError::bad_request(
                "Only a kind \"sidebar\" extension can declare sidebar or sidePanel.",
            ));
        }
        match self.kind {
            Some(ExtensionKind::Sidebar) => self.validate_sidebar_shape()?,
            Some(ExtensionKind::TerminalPane) => {
                if self.terminal.is_none()
                    || self.server.is_some()
                    || !self.placements.is_empty()
                    || self.default_placement.is_some()
                {
                    return Err(ExtensionError::bad_request(
                        "A terminal-pane extension requires terminal and cannot declare web placements or server.",
                    ));
                }
                let terminal = self.terminal.as_ref().expect("checked terminal");
                if terminal.command.trim().is_empty() {
                    return Err(ExtensionError::bad_request(
                        "A terminal-pane extension requires a non-empty command.",
                    ));
                }
            }
            None => {
                if self.terminal.is_some() || self.placements.is_empty() {
                    return Err(ExtensionError::bad_request(
                        "A web extension requires at least one placement and cannot declare terminal.",
                    ));
                }
                let Some(default_placement) = self.default_placement else {
                    return Err(ExtensionError::bad_request(
                        "A web extension requires defaultPlacement.",
                    ));
                };
                if !self.placements.contains(&default_placement) {
                    return Err(ExtensionError::bad_request(
                        "defaultPlacement must occur in placements.",
                    ));
                }
                let Some(server) = &self.server else {
                    return Err(ExtensionError::bad_request(
                        "A web extension requires server.",
                    ));
                };
                validate_server(server)?;
            }
        }
        let unique_placements = self
            .placements
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if unique_placements.len() != self.placements.len() {
            return Err(ExtensionError::bad_request(
                "Extension placements must be unique.",
            ));
        }
        let unique_permissions = self
            .permissions
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if unique_permissions.len() != self.permissions.len() {
            return Err(ExtensionError::bad_request(
                "Extension permissions must be unique.",
            ));
        }
        if let Some(modal) = self.modal {
            if modal.width == 0 || modal.width > 1400 || modal.height == 0 || modal.height > 900 {
                return Err(ExtensionError::bad_request(
                    "Modal size must be within 1400x900.",
                ));
            }
        }
        if let Some(popup) = self.popup {
            if popup.width == 0 || popup.width > 420 || popup.height == 0 || popup.height > 640 {
                return Err(ExtensionError::bad_request(
                    "Popup size must be within 420x640.",
                ));
            }
        }
        Ok(())
    }

    /// `kind: "sidebar"`: a mode, sections or a side panel, an optional command or static server
    /// (for outside data and, later, web blocks), and no web placements or terminal.
    fn validate_sidebar_shape(&self) -> ExtensionResult<()> {
        if self.terminal.is_some()
            || !self.placements.is_empty()
            || self.default_placement.is_some()
            || self.modal.is_some()
            || self.popup.is_some()
        {
            return Err(ExtensionError::bad_request(
                "A sidebar extension cannot declare placements, defaultPlacement, terminal, modal or popup.",
            ));
        }
        let sidebar_shows = self
            .sidebar
            .as_ref()
            .is_some_and(|sidebar| sidebar.mode.is_some() || !sidebar.sections.is_empty());
        if self.sidebar.is_some() && !sidebar_shows {
            return Err(ExtensionError::bad_request(
                "sidebar needs a mode, sections, or both.",
            ));
        }
        if !sidebar_shows && self.side_panel.is_none() {
            return Err(ExtensionError::bad_request(
                "A sidebar extension needs sidebar (a mode or sections) or sidePanel.",
            ));
        }
        if let Some(sidebar) = &self.sidebar {
            if let Some(mode) = &sidebar.mode {
                if mode.title.trim().is_empty() {
                    return Err(ExtensionError::bad_request(
                        "sidebar.mode.title must not be empty.",
                    ));
                }
                if let Some(icon) = &mode.icon {
                    validate_svg_path(icon, "sidebar.mode.icon")?;
                }
            }
            let mut blocks = std::collections::BTreeSet::new();
            for section in &sidebar.sections {
                if section.block.trim().is_empty() || !blocks.insert(section.block.as_str()) {
                    return Err(ExtensionError::bad_request(
                        "sidebar.sections must name different, non-empty blocks.",
                    ));
                }
            }
            if let Some(panel) = &sidebar.panel {
                validate_json_path(panel, "sidebar.panel")?;
            }
        }
        if let Some(side_panel) = &self.side_panel {
            if side_panel.title.trim().is_empty() {
                return Err(ExtensionError::bad_request(
                    "sidePanel.title must not be empty.",
                ));
            }
            if let Some(icon) = &side_panel.icon {
                validate_svg_path(icon, "sidePanel.icon")?;
            }
            if let Some(panel) = &side_panel.panel {
                validate_json_path(panel, "sidePanel.panel")?;
            }
        }
        if let Some(server) = &self.server {
            if server.url.is_some() {
                return Err(ExtensionError::bad_request(
                    "A sidebar extension's server is a command or a static folder, not a url.",
                ));
            }
            validate_server(server)?;
        }
        Ok(())
    }

    /// Checks the panel files a sidebar extension ships against the panel schema, and that its
    /// sections name blocks of its sidebar document.
    fn validate_sidebar_payload(&self, payload_dir: &Path) -> ExtensionResult<()> {
        if let Some(sidebar) = &self.sidebar {
            if let Some(icon) = sidebar.mode.as_ref().and_then(|mode| mode.icon.as_ref()) {
                require_payload_file(payload_dir, icon, "sidebar.mode.icon")?;
            }
            if let Some(panel) = &sidebar.panel {
                let document = super::read_panel_file(payload_dir, panel, "sidebar.panel", self)?;
                for section in &sidebar.sections {
                    if !document
                        .blocks
                        .iter()
                        .any(|block| block.id == section.block)
                    {
                        return Err(ExtensionError::bad_request(format!(
                            "sidebar.sections names block {:?}, which {panel} doesn't have.",
                            section.block
                        )));
                    }
                }
            }
        }
        if let Some(side_panel) = &self.side_panel {
            if let Some(icon) = &side_panel.icon {
                require_payload_file(payload_dir, icon, "sidePanel.icon")?;
            }
            if let Some(panel) = &side_panel.panel {
                super::read_panel_file(payload_dir, panel, "sidePanel.panel", self)?;
            }
        }
        Ok(())
    }

    fn validate_preferences(&self) -> ExtensionResult<()> {
        let mut names = std::collections::BTreeSet::new();
        for preference in &self.preferences {
            if !valid_preference_name(&preference.name) || !names.insert(&preference.name) {
                return Err(ExtensionError::bad_request(format!(
                    "Invalid or duplicate preference name: {}",
                    preference.name
                )));
            }
            if preference.title.trim().is_empty() || preference.description.trim().is_empty() {
                return Err(ExtensionError::bad_request(format!(
                    "Preference {} requires title and description.",
                    preference.name
                )));
            }
            if preference.default.as_ref().is_some_and(|value| {
                !value.is_string() && !value.is_boolean() && !value.is_number()
            }) {
                return Err(ExtensionError::bad_request(format!(
                    "Preference {} has an unsupported default value.",
                    preference.name
                )));
            }
            if preference.preference_type == ExtensionPreferenceType::Dropdown
                && preference.data.is_empty()
            {
                return Err(ExtensionError::bad_request(format!(
                    "Dropdown preference {} requires data options.",
                    preference.name
                )));
            }
        }
        Ok(())
    }
}

/// One of a static folder, a command server or a remote URL, with the fields that kind allows.
fn validate_server(server: &ExtensionServer) -> ExtensionResult<()> {
    match (&server.static_dir, &server.command, &server.url) {
        (Some(static_dir), None, None) => {
            validate_relative_path(static_dir, "server.static")?;
            if server.readiness.is_some() || server.cwd.is_some() || !server.install.is_empty() {
                return Err(ExtensionError::bad_request(
                    "A static server cannot declare command-server fields.",
                ));
            }
        }
        (None, None, Some(url)) => {
            validate_remote_server_url(url)?;
            if server.readiness.is_some() || server.cwd.is_some() || !server.install.is_empty() {
                return Err(ExtensionError::bad_request(
                    "A URL server cannot declare command-server fields.",
                ));
            }
        }
        (None, Some(command), None) if !command.trim().is_empty() => {
            let readiness = server.readiness.as_ref().ok_or_else(|| {
                ExtensionError::bad_request("A command server requires readiness.httpGet.")
            })?;
            if !readiness.http_get.starts_with('/') {
                return Err(ExtensionError::bad_request(
                    "readiness.httpGet must start with '/'.",
                ));
            }
            for install in server.install.values() {
                validate_sha256(&install.sha256)?;
                url::Url::parse(&install.url).map_err(|_| {
                    ExtensionError::bad_request(format!(
                        "Invalid command-server install URL: {}",
                        install.url
                    ))
                })?;
            }
        }
        _ => {
            return Err(ExtensionError::bad_request(
                "server must declare exactly one of static, command, or url.",
            ));
        }
    }
    Ok(())
}

pub fn validate_extension_id(id: &str) -> ExtensionResult<()> {
    if valid_extension_id(id) {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(format!(
            "Invalid extension id: {id:?}."
        )))
    }
}

pub fn validate_sha256(value: &str) -> ExtensionResult<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(
            "Expected SHA-256 must contain exactly 64 hexadecimal characters.",
        ))
    }
}

fn valid_extension_id(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn valid_preference_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_semver(value: &str) -> bool {
    let (without_build, build) = value
        .split_once('+')
        .map(|(version, build)| (version, Some(build)))
        .unwrap_or((value, None));
    if build.is_some_and(|build| !valid_semver_identifiers(build, false)) {
        return false;
    }
    let (core, prerelease) = without_build
        .split_once('-')
        .map(|(core, prerelease)| (core, Some(prerelease)))
        .unwrap_or((without_build, None));
    if prerelease.is_some_and(|prerelease| !valid_semver_identifiers(prerelease, true)) {
        return false;
    }
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == &"0" || !part.starts_with('0'))
        })
}

fn valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!reject_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || identifier == "0"
                    || !identifier.starts_with('0'))
        })
}

/*
A `server.url` extension hosts a web page gxserver never starts, serves, or
verifies, so the manifest URL is the whole trust boundary: it must be absolute,
carry no embedded credentials, and be transport-encrypted. Plain HTTP is
accepted only for loopback hosts, where there is no network to intercept.
*/
fn validate_remote_server_url(value: &str) -> ExtensionResult<()> {
    let url = url::Url::parse(value).map_err(|_| {
        ExtensionError::bad_request(format!(
            "Extension server.url must be an absolute URL: {value}"
        ))
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ExtensionError::bad_request(
            "Extension server.url must not embed credentials.",
        ));
    }
    let host_is_loopback = match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => {
            return Err(ExtensionError::bad_request(
                "Extension server.url must name a host.",
            ))
        }
    };
    if url.scheme() == "https" || (url.scheme() == "http" && host_is_loopback) {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(
            "Extension server.url must use https, except for loopback hosts.",
        ))
    }
}

fn validate_svg_path(value: &str, field: &str) -> ExtensionResult<()> {
    validate_relative_path(value, field)?;
    if value.to_ascii_lowercase().ends_with(".svg") {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(format!(
            "Extension {field} must reference an SVG file."
        )))
    }
}

fn validate_json_path(value: &str, field: &str) -> ExtensionResult<()> {
    validate_relative_path(value, field)?;
    if value.to_ascii_lowercase().ends_with(".json") {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(format!(
            "Extension {field} must reference a JSON file."
        )))
    }
}

fn require_payload_file(payload_dir: &Path, relative: &str, field: &str) -> ExtensionResult<()> {
    if payload_dir.join(relative).is_file() {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(format!(
            "Extension {field} does not exist: {relative}"
        )))
    }
}

pub(crate) fn validate_relative_path(value: &str, field: &str) -> ExtensionResult<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ExtensionError::bad_request(format!(
            "Extension {field} must stay inside the extension folder."
        )));
    }
    Ok(())
}
