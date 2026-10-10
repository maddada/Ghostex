//! The Settings pages, the open request (deep links), and what the modal asks its host to do.
use serde_json::{Map, Value};
use std::rc::Rc;

/// The Settings pages (`SETTINGS_MODAL_NAVIGATION_TABS`), with their stored ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SettingsTabId {
    General,
    Theme,
    Integrations,
    CloudBoxes,
    Extensions,
    OsIntegration,
    Remote,
    Projects,
    Agents,
    Accounts,
    Actions,
    OpenTargets,
    Hotkeys,
    Debugging,
    About,
    Workspaces,
}

impl SettingsTabId {
    /// The rail order (`createSettingsSidebarPages`); OS Integration and Debugging are filtered
    /// by the shell, and About is pinned to the rail's bottom.
    pub(crate) const RAIL_ORDER: [SettingsTabId; 16] = [
        SettingsTabId::General,
        SettingsTabId::Theme,
        SettingsTabId::Agents,
        SettingsTabId::Accounts,
        SettingsTabId::Workspaces,
        SettingsTabId::Integrations,
        SettingsTabId::CloudBoxes,
        SettingsTabId::Extensions,
        SettingsTabId::Remote,
        SettingsTabId::Projects,
        SettingsTabId::Hotkeys,
        SettingsTabId::Actions,
        SettingsTabId::OpenTargets,
        SettingsTabId::OsIntegration,
        SettingsTabId::Debugging,
        SettingsTabId::About,
    ];

    /// The id the settings file and the open message use (`SettingsModalTab`).
    pub(crate) fn id(self) -> &'static str {
        match self {
            SettingsTabId::General => "settings",
            SettingsTabId::Theme => "theme",
            SettingsTabId::Integrations => "integrations",
            SettingsTabId::CloudBoxes => "cloudBoxes",
            SettingsTabId::Extensions => "extensions",
            SettingsTabId::OsIntegration => "osIntegration",
            SettingsTabId::Remote => "remote",
            SettingsTabId::Projects => "projects",
            SettingsTabId::Agents => "agents",
            SettingsTabId::Accounts => "accounts",
            SettingsTabId::Actions => "actions",
            SettingsTabId::OpenTargets => "openTargets",
            SettingsTabId::Hotkeys => "hotkeys",
            SettingsTabId::Debugging => "debugging",
            SettingsTabId::About => "about",
            SettingsTabId::Workspaces => "workspaces",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::RAIL_ORDER.into_iter().find(|tab| tab.id() == id)
    }

    /// The rail label.
    pub(crate) fn title(self) -> &'static str {
        match self {
            SettingsTabId::General => "General",
            SettingsTabId::Theme => "Theme",
            SettingsTabId::Integrations => "Integrations",
            SettingsTabId::CloudBoxes => "Cloud Boxes",
            SettingsTabId::Extensions => "Extensions",
            SettingsTabId::OsIntegration => "OS Integration",
            SettingsTabId::Remote => "Remote",
            SettingsTabId::Projects => "Projects",
            SettingsTabId::Agents => "Agents",
            SettingsTabId::Accounts => "Accounts",
            SettingsTabId::Actions => "Actions",
            SettingsTabId::OpenTargets => "Open In",
            SettingsTabId::Hotkeys => "Hotkeys",
            SettingsTabId::Debugging => "Debugging",
            SettingsTabId::About => "About",
            SettingsTabId::Workspaces => "Workspaces",
        }
    }

    /// The rail icon (the Tabler icon `sidebar-pages.ts` (deleted 2026-10-01) draws).
    pub(crate) fn icon(self) -> &'static str {
        match self {
            SettingsTabId::General => "modals/settings/settings.svg",
            SettingsTabId::Theme => "modals/settings/palette.svg",
            SettingsTabId::Integrations => "modals/settings/tools.svg",
            SettingsTabId::CloudBoxes => "modals/settings/box.svg",
            SettingsTabId::Extensions => "modals/settings/puzzle.svg",
            SettingsTabId::OsIntegration => "modals/settings/device-desktop.svg",
            SettingsTabId::Remote => "modals/settings/cloud.svg",
            SettingsTabId::Projects => "modals/settings/folder-open.svg",
            SettingsTabId::Agents => "modals/settings/code-dots.svg",
            SettingsTabId::Accounts => "modals/settings/users.svg",
            SettingsTabId::Actions => "modals/settings/player-play.svg",
            SettingsTabId::OpenTargets => "modals/settings/external-link.svg",
            SettingsTabId::Hotkeys => "modals/settings/keyboard.svg",
            SettingsTabId::Debugging => "modals/settings/bug.svg",
            SettingsTabId::About => "modals/settings/info-circle.svg",
            SettingsTabId::Workspaces => "modals/settings/briefcase.svg",
        }
    }
}

/// The deep-link fields of a Settings `open` message (`modal-host.tsx` (deleted 2026-10-01)), kept for the pages that
/// honour them. Only an open of the `settings` kind carries them; the other kinds pick the start
/// page (`getSettingsInitialTab`).
#[derive(Clone, Debug, Default)]
pub(crate) struct SettingsOpenRequest {
    /// `initialTab` as the React modal received it: the `initialTab` field of a `settings` open,
    /// else the page the modal kind names, else `settings`.
    pub(crate) initial_tab: Option<SettingsTabId>,
    /// A General scroll target (`MainSettingsScrollTargetId`).
    pub(crate) initial_section: Option<String>,
    /// `createTag`: open Sidebar Tags' New tag form on arrival.
    pub(crate) initial_sidebar_tags_action: Option<String>,
    pub(crate) initial_search_query: Option<String>,
    pub(crate) initial_remote_machine_id: Option<String>,
    /// `easyConnect` or `tailscale`.
    pub(crate) initial_remote_section: Option<String>,
    /// `agentHooks`.
    pub(crate) initial_agents_section: Option<String>,
    pub(crate) initial_custom_view_id: Option<String>,
    pub(crate) initial_view_scope_key: Option<String>,
    /// The project the Projects page opens on: `initialProjectId`, else the opening window's
    /// current project (filled in by the host).
    pub(crate) initial_project_id: Option<String>,
    /// Opens the Pick Color dialog of this colour setting (the preview binary's `pick-color`
    /// state, and the host where it has no system colour panel; no open message carries it).
    pub(crate) open_color_picker: Option<String>,
    /// Opens this setting's dropdown on arrival (the preview binary's `select` state).
    pub(crate) open_select: Option<String>,
    /// The host can reach the local gxserver (the React page's `tailcatRpc` was defined): set
    /// by the host after reading the open message, never by the message itself.
    pub(crate) gxserver_rpc_available: bool,
    /// The preview binary's state name, for page states no open message reaches (an open
    /// dialog, an expanded row, a scripted interaction). Always `None` in the app.
    pub(crate) preview_state: Option<String>,
}

fn optional_text(message: &Value, key: &str) -> Option<String> {
    message.get(key).and_then(Value::as_str).map(str::to_string)
}

impl SettingsOpenRequest {
    /// Reads an `open` message for one of the Settings kinds (`settings`, `hotkeys`,
    /// `configureAgents`, `configureActions`, `openTargets`), the way modal-host.tsx (deleted 2026-10-01) does.
    pub(crate) fn from_open_message(modal_id: &str, message: &Value) -> Self {
        let kind_tab = match modal_id {
            "configureAgents" => Some(SettingsTabId::Agents),
            "configureActions" => Some(SettingsTabId::Actions),
            "hotkeys" => Some(SettingsTabId::Hotkeys),
            "openTargets" => Some(SettingsTabId::OpenTargets),
            _ => None,
        };
        if modal_id != "settings" {
            return Self {
                initial_tab: kind_tab,
                ..Self::default()
            };
        }
        Self {
            initial_tab: message
                .get("initialTab")
                .and_then(Value::as_str)
                .and_then(SettingsTabId::from_id),
            initial_section: optional_text(message, "initialSection"),
            initial_sidebar_tags_action: optional_text(message, "initialSidebarTagsAction")
                .filter(|action| action == "createTag"),
            initial_search_query: optional_text(message, "initialSearchQuery"),
            initial_remote_machine_id: optional_text(message, "initialRemoteMachineId")
                .filter(|id| !id.trim().is_empty()),
            initial_remote_section: optional_text(message, "initialRemoteSection")
                .filter(|section| section == "easyConnect" || section == "tailscale"),
            initial_agents_section: optional_text(message, "initialAgentsSection")
                .filter(|section| section == "agentHooks"),
            initial_custom_view_id: optional_text(message, "initialCustomViewId"),
            initial_view_scope_key: optional_text(message, "initialViewScopeKey"),
            initial_project_id: optional_text(message, "initialProjectId")
                .filter(|id| !id.trim().is_empty()),
            open_color_picker: None,
            open_select: None,
            gxserver_rpc_available: false,
            preview_state: None,
        }
    }

    /// `initialTab` as the React modal saw it (`settings` when nothing named a page).
    pub(crate) fn requested_tab(&self) -> SettingsTabId {
        self.initial_tab.unwrap_or(SettingsTabId::General)
    }
}

/// What the Settings modal asks the app to do. The native twin of the React modal's props
/// (`onPatch`, `onChange`, `onClose`) and of every `vscode.postMessage` it made.
pub(crate) enum SettingsModalCommand {
    /// `updateSettingsPatch`: a granular save merged onto the stored settings.
    SavePatch {
        patch: Map<String, Value>,
        source: String,
    },
    /// `updateSettings`: a whole-settings save (presets, Reset to defaults, the Ghostty buttons).
    SaveSettings {
        settings: Map<String, Value>,
        source: String,
    },
    /// A message the React modal posted with `vscode.postMessage` (it reached the app as
    /// `{ type: "sidebarCommand", message }`), handled exactly as that message is.
    PostMessage(Value),
    /// A toast in the app's toast window.
    Toast {
        level: String,
        title: String,
        description: String,
    },
    /// `ColorField`'s swatch (`<input type="color">`): the system colour panel, owned by the
    /// Settings window, starting at `initial` (`#rrggbb`). Each colour it picks comes back
    /// through `GpuiSettingsModalWindow::receive_system_color(key, ..)`, which saves it as the
    /// input's `onChange` did.
    PickSystemColor { key: String, initial: String },
    /// A POST to the local gxserver (`/api/tailcatStatus`, `/api/agentCliMaintenance`, ...) the
    /// way the React page's `RemoteSetupRpc` / `AgentCliConnection` called it; `reply` runs on
    /// the main thread with the envelope's `result` or the error message.
    GxserverRpc {
        path: String,
        params: Value,
        timeout: std::time::Duration,
        reply: GxserverRpcReply,
    },
    /// Writes `text` to the clipboard with the app's copy feedback (`playCopySound` and the
    /// "Copied!" bubble) the React copy buttons gave.
    CopyToClipboard(String),
    /// A plain HTTP GET the React page made with `fetch` (an extension's README, changelog and
    /// screenshots from the catalog). A `url` that starts with `/` is a gxserver path (the
    /// extension icons under `/ext/<id>/...`); `reply` runs on the main thread with the body.
    HttpGet { url: String, reply: HttpGetReply },
    /// A message the React page posted to the app-modal host itself (`postAppModalHostMessage`:
    /// `accountTitlebarChanged`, `accountSetup`), handled exactly as the host bridge handles it.
    HostMessage(Value),
    /// The modal removed its own window.
    Close,
    /// A sign-in the Accounts page started finished after the page closed: show Settings at
    /// Accounts (`tabs/accounts/sign_in_watch.rs`).
    OpenAccounts,
}

/// The answer of a [`SettingsModalCommand::GxserverRpc`].
pub(crate) type GxserverRpcReply = Box<dyn FnOnce(Result<Value, String>, &mut gpui::App)>;

/// The answer of a [`SettingsModalCommand::HttpGet`].
pub(crate) type HttpGetReply = Box<dyn FnOnce(Result<Vec<u8>, String>, &mut gpui::App)>;

pub(crate) type SettingsModalHost = Rc<dyn Fn(SettingsModalCommand, &mut gpui::App)>;
