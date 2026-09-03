// C1 wave-3 re-cluster: the GpuiAppModalKind enum (every native app-modal window's id, title, size, and open-message shape) and its hotkey-action lookup, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAppModalKind {
    AddProject,
    AgentHooksRequired,
    Settings,
    Hotkeys,
    MissingProjectFolder,
    CommandPalette,
    FindPrompts,
    PreviousSessions,
    RecentProjects,
    StashedPrompts,
    AgentsHub,
    DelayedSend,
    RenameSession,
    SessionNote,
    ExportTranscriptResult,
    ConfigureAgents,
    ConfigureActions,
    OpenTargets,
    FirstLaunchSetup,
    WatchGhostexVideo,
    RemoteGxserverInstall,
    RemoteProjectPicker,
    RemoteSetup,
    Worktree,
    DeleteWorktree,
    RenameWorktree,
    SidebarSpaceEditor,
    GitCommit,
    GitFileDiff,
    PortlessSetup,
    DiscoverGhostex,
    Extension(ExtensionId),
    UpdateAvailable,
}

impl GpuiAppModalKind {
    pub(crate) fn from_modal_id(value: &str) -> Option<Self> {
        match value {
            "addProject" => Some(Self::AddProject),
            "agentHooksRequired" => Some(Self::AgentHooksRequired),
            "settings" => Some(Self::Settings),
            "hotkeys" => Some(Self::Hotkeys),
            "missingProjectFolder" => Some(Self::MissingProjectFolder),
            "commandPalette" => Some(Self::CommandPalette),
            "findPrompts" => Some(Self::FindPrompts),
            "previousSessions" => Some(Self::PreviousSessions),
            "recentProjects" => Some(Self::RecentProjects),
            "stashedPrompts" => Some(Self::StashedPrompts),
            "agentsHub" => Some(Self::AgentsHub),
            "delayedSend" => Some(Self::DelayedSend),
            "renameSession" => Some(Self::RenameSession),
            "sessionNote" => Some(Self::SessionNote),
            "exportTranscriptResult" => Some(Self::ExportTranscriptResult),
            "configureAgents" => Some(Self::ConfigureAgents),
            "configureActions" => Some(Self::ConfigureActions),
            "openTargets" => Some(Self::OpenTargets),
            "firstLaunchSetup" | "tipsAndTricks" => Some(Self::FirstLaunchSetup),
            "watchGhostexVideo" => Some(Self::WatchGhostexVideo),
            "remoteGxserverInstall" => Some(Self::RemoteGxserverInstall),
            "remoteProjectPicker" => Some(Self::RemoteProjectPicker),
            "remoteSetup" => Some(Self::RemoteSetup),
            "worktree" => Some(Self::Worktree),
            "deleteWorktree" => Some(Self::DeleteWorktree),
            "renameWorktree" => Some(Self::RenameWorktree),
            "sidebarSpaceEditor" => Some(Self::SidebarSpaceEditor),
            "gitCommit" => Some(Self::GitCommit),
            "gitFileDiff" => Some(Self::GitFileDiff),
            "portlessSetup" => Some(Self::PortlessSetup),
            "discoverGhostex" => Some(Self::DiscoverGhostex),
            value if value.starts_with("extension:") => {
                ExtensionId::new(value.trim_start_matches("extension:")).map(Self::Extension)
            }
            "updateAvailable" => Some(Self::UpdateAvailable),
            _ => None,
        }
    }

    pub(crate) fn modal_id(self) -> &'static str {
        match self {
            Self::AddProject => "addProject",
            Self::AgentHooksRequired => "agentHooksRequired",
            Self::Settings => "settings",
            Self::Hotkeys => "hotkeys",
            Self::MissingProjectFolder => "missingProjectFolder",
            Self::CommandPalette => "commandPalette",
            Self::FindPrompts => "findPrompts",
            Self::PreviousSessions => "previousSessions",
            Self::RecentProjects => "recentProjects",
            Self::StashedPrompts => "stashedPrompts",
            Self::AgentsHub => "agentsHub",
            Self::DelayedSend => "delayedSend",
            Self::RenameSession => "renameSession",
            Self::SessionNote => "sessionNote",
            Self::ExportTranscriptResult => "exportTranscriptResult",
            Self::ConfigureAgents => "configureAgents",
            Self::ConfigureActions => "configureActions",
            Self::OpenTargets => "openTargets",
            Self::FirstLaunchSetup => "firstLaunchSetup",
            Self::WatchGhostexVideo => "watchGhostexVideo",
            Self::RemoteGxserverInstall => "remoteGxserverInstall",
            Self::RemoteProjectPicker => "remoteProjectPicker",
            Self::RemoteSetup => "remoteSetup",
            Self::Worktree => "worktree",
            Self::DeleteWorktree => "deleteWorktree",
            Self::RenameWorktree => "renameWorktree",
            Self::SidebarSpaceEditor => "sidebarSpaceEditor",
            Self::GitCommit => "gitCommit",
            Self::GitFileDiff => "gitFileDiff",
            Self::PortlessSetup => "portlessSetup",
            Self::DiscoverGhostex => "discoverGhostex",
            Self::Extension(id) => extension_modal_id(id),
            Self::UpdateAvailable => "updateAvailable",
        }
    }

    pub(crate) fn window_title(self) -> &'static str {
        match self {
            Self::AddProject => "Ghostex Add Project",
            Self::AgentHooksRequired => "Ghostex Install Required Hooks",
            Self::Settings => "Ghostex Settings",
            Self::Hotkeys => "Ghostex Hotkeys",
            Self::MissingProjectFolder => "Ghostex Project Folder Missing",
            Self::CommandPalette
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts => "Ghostex Quick Access",
            Self::FindPrompts => "Ghostex Search by Prompt",
            Self::AgentsHub => "Ghostex Agents Hub",
            Self::DelayedSend => "Ghostex Session Automations",
            Self::RenameSession => "Ghostex Rename Session",
            Self::SessionNote => "Ghostex Session Note",
            Self::ExportTranscriptResult => "Ghostex Handoff / Export",
            Self::ConfigureAgents => "Ghostex Configure Agents",
            Self::ConfigureActions => "Ghostex Actions",
            Self::OpenTargets => "Ghostex Open Targets",
            Self::FirstLaunchSetup => "Welcome to Ghostex",
            Self::WatchGhostexVideo => "Ghostex Tutorial Video",
            Self::RemoteGxserverInstall => "Ghostex Remote Setup",
            Self::RemoteProjectPicker => "Ghostex Remote Project",
            Self::RemoteSetup => "Ghostex Mobile & Remote",
            Self::Worktree => "Ghostex Add Worktree",
            Self::DeleteWorktree => "Ghostex Delete Worktree",
            Self::RenameWorktree => "Ghostex Rename Worktree",
            Self::SidebarSpaceEditor => "Ghostex Space",
            Self::GitCommit => "Ghostex Commit Changes",
            Self::GitFileDiff => "Ghostex File Diff",
            Self::PortlessSetup => "Ghostex Portless Setup",
            Self::DiscoverGhostex => "Discover Ghostex",
            Self::Extension(_) => "Ghostex Extension",
            Self::UpdateAvailable => "Ghostex Update",
        }
    }

    pub(crate) fn window_size(self) -> Size<Pixels> {
        match self {
            /* All four Quick Access tabs share one stable child-window frame. */
            Self::CommandPalette
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts => size(
                px(APP_MODAL_HOST_COMMAND_PALETTE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_PREVIOUS_SESSIONS_WINDOW_HEIGHT),
            ),
            Self::DelayedSend => size(
                px(APP_MODAL_HOST_DELAYED_SEND_WINDOW_WIDTH),
                px(APP_MODAL_HOST_DELAYED_SEND_WINDOW_HEIGHT),
            ),
            /*
            CDXC:SessionAgentNotes 2026-08-24:
            The note editor is Rename Session's dialog with a taller text field,
            so it opens on the same frame and the one-shot fit-height pass sizes
            the child window down to whatever it actually rendered.
            */
            Self::RenameSession | Self::SessionNote => size(
                px(APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH),
                px(APP_MODAL_HOST_RENAME_SESSION_WINDOW_HEIGHT),
            ),
            Self::ExportTranscriptResult => size(
                px(APP_MODAL_HOST_EXPORT_TRANSCRIPT_RESULT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_EXPORT_TRANSCRIPT_RESULT_WINDOW_HEIGHT),
            ),
            Self::MissingProjectFolder => size(
                px(APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_WIDTH),
                px(APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_HEIGHT),
            ),
            Self::AgentHooksRequired => size(
                px(APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH),
                px(APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_HEIGHT),
            ),
            /*
            CDXC:GPUIAppModalSizes 2026-07-26-07:20:
            Settings, Hotkeys, Configure Agents, Configure Actions, and Open Targets all render the one tabbed Settings dialog in the modal host, so they must keep the full Settings frame even though their legacy standalone stylesheets are narrower.
            */
            Self::Settings
            | Self::Hotkeys
            | Self::FindPrompts
            | Self::AgentsHub
            | Self::ConfigureAgents
            | Self::ConfigureActions
            | Self::OpenTargets
            | Self::GitFileDiff => size(
                px(APP_MODAL_HOST_WINDOW_WIDTH),
                px(APP_MODAL_HOST_WINDOW_HEIGHT),
            ),
            Self::AddProject => size(
                px(APP_MODAL_HOST_ADD_PROJECT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_ADD_PROJECT_WINDOW_HEIGHT),
            ),
            Self::RemoteProjectPicker => size(
                px(APP_MODAL_HOST_REMOTE_PROJECT_PICKER_WINDOW_WIDTH),
                px(APP_MODAL_HOST_REMOTE_PROJECT_PICKER_WINDOW_HEIGHT),
            ),
            Self::Worktree => size(
                px(APP_MODAL_HOST_WORKTREE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_WORKTREE_WINDOW_HEIGHT),
            ),
            Self::GitCommit => size(
                px(APP_MODAL_HOST_GIT_COMMIT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_GIT_COMMIT_WINDOW_HEIGHT),
            ),
            Self::DeleteWorktree => size(
                px(APP_MODAL_HOST_COMPACT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_DELETE_WORKTREE_WINDOW_HEIGHT),
            ),
            /*
            CDXC:WorktreeRename 2026-08-09-18:40:
            Rename Worktree is one field, a preview, a checkbox, and however many
            warnings the checkout has, so it opens on the same compact frame as
            Delete Worktree and the one-shot fit-height pass sizes it down to
            whatever it actually rendered.
            */
            Self::RenameWorktree => size(
                px(APP_MODAL_HOST_COMPACT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_DELETE_WORKTREE_WINDOW_HEIGHT),
            ),
            /*
            CDXC:SidebarSpaces 2026-08-28:
            New/Edit Space is a name field, an icon grid, and a color row, so it
            uses the compact modal width with a 380px GPUI content height. The
            native titlebar and frame are outside these window content bounds.
            */
            Self::SidebarSpaceEditor => size(
                px(APP_MODAL_HOST_COMPACT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_SIDEBAR_SPACE_EDITOR_WINDOW_HEIGHT),
            ),
            Self::PortlessSetup => size(
                px(APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_WIDTH),
                px(APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_HEIGHT),
            ),
            Self::UpdateAvailable => size(
                px(APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_HEIGHT),
            ),
            Self::DiscoverGhostex => size(px(1120.0), px(850.0)),
            Self::Extension(id) => extension_modal_window_size(id),
            Self::RemoteGxserverInstall => size(
                px(APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_WIDTH),
                px(APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_HEIGHT),
            ),
            Self::RemoteSetup => size(
                px(APP_MODAL_HOST_REMOTE_SETUP_WINDOW_WIDTH),
                px(APP_MODAL_HOST_REMOTE_SETUP_WINDOW_HEIGHT),
            ),
            Self::FirstLaunchSetup => size(px(1120.0), px(850.0)),
            Self::WatchGhostexVideo => size(px(1120.0), px(750.0)),
        }
    }

    pub(crate) fn is_resizable(self) -> bool {
        /*
        CDXC:GPUICommandAppModalSize 2026-06-27-09:57:
        Native command-pane Rename Session and Delayed Send are fixed-size child windows. GPUI must not apply a generic resizable minimum to these compact dialogs because Delayed Send is intentionally 470x365.

        CDXC:GPUIAppModalSizes 2026-07-26-07:20:
        Every app-modal window is now fitted to its own dialog, so none of them are resizable. The React dialogs own their internal scrolling, and a resizable frame only ever produced dead space around a fixed-height form or a stretched compact dialog.

        CDXC:SettingsExtensionsTab 2026-08-30:
        The Extensions browser was the last resizable app modal; it is a
        Settings tab now, so this is unconditionally false again.
        */
        false
    }

    pub(crate) fn window_min_size(self) -> Size<Pixels> {
        self.window_size()
    }

    pub(crate) fn uses_react_modal_host(self) -> bool {
        !matches!(
            self,
            Self::FindPrompts | Self::WatchGhostexVideo | Self::Extension(_)
        )
    }

    pub(crate) fn is_settings_modal_entry(self) -> bool {
        matches!(
            self,
            Self::Settings
                | Self::Hotkeys
                | Self::ConfigureAgents
                | Self::ConfigureActions
                | Self::OpenTargets
        )
    }

    pub(crate) fn requires_sidebar_state(self) -> bool {
        matches!(
            self,
            Self::Settings
                | Self::Hotkeys
                | Self::ConfigureAgents
                | Self::ConfigureActions
                | Self::OpenTargets
                | Self::FirstLaunchSetup
                | Self::AgentsHub
                | Self::DelayedSend
                | Self::RenameSession
                // The export result dialog's agent picker renders the user's
                // configured agents, which only reach the modal host through
                // the sidebar-state snapshot.
                | Self::ExportTranscriptResult
                | Self::Worktree
                | Self::DeleteWorktree
                | Self::RenameWorktree
                | Self::GitCommit
                | Self::GitFileDiff
                | Self::PortlessSetup
                | Self::DiscoverGhostex
        )
    }

    pub(crate) fn open_message(self) -> serde_json::Value {
        match self {
            Self::CommandPalette => serde_json::json!({
                "initialQuery": "",
                "modal": self.modal_id(),
                "type": "open",
            }),
            Self::Settings
            | Self::Hotkeys
            | Self::FindPrompts
            | Self::ConfigureAgents
            | Self::ConfigureActions
            | Self::OpenTargets
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts
            | Self::AgentsHub
            | Self::DelayedSend
            | Self::RenameSession
            | Self::SessionNote
            | Self::WatchGhostexVideo
            | Self::RemoteSetup
            | Self::FirstLaunchSetup => serde_json::json!({
                "modal": self.modal_id(),
                "type": "open",
            }),
            Self::Extension(_) => serde_json::Value::Null,
            Self::UpdateAvailable => serde_json::json!({
                "modal": self.modal_id(),
                "type": "open",
            }),
            /*
            CDXC:AddProject 2026-07-30:
            The menu/palette path opens the dialog with no machine preselected,
            so it starts on its machine step whenever more than one machine is
            available. Entry points that own a machine send `machineId` through
            the normal open message instead.
            */
            Self::AddProject => serde_json::json!({
                "modal": self.modal_id(),
                "type": "open",
            }),
            Self::RemoteGxserverInstall => serde_json::json!({
                "modal": self.modal_id(),
                "remoteMachineId": "",
                "remoteMachineName": "Remote",
                "type": "open",
            }),
            Self::RemoteProjectPicker => serde_json::json!({
                "modal": self.modal_id(),
                "remoteMachineId": "",
                "remoteMachineName": "Remote",
                "type": "open",
            }),
            // These modals are normally opened through bridge messages that
            // carry their full payload (worktree and diff drafts); the bare
            // open message is the menu-path shape. For the Space editor it is
            // also the template the bridge's field allowlist fills in.
            Self::Worktree
            | Self::AgentHooksRequired
            | Self::DeleteWorktree
            | Self::RenameWorktree
            | Self::SidebarSpaceEditor
            | Self::GitCommit
            | Self::GitFileDiff
            | Self::PortlessSetup
            | Self::DiscoverGhostex
            | Self::ExportTranscriptResult
            | Self::MissingProjectFolder => serde_json::json!({
                "modal": self.modal_id(),
                "type": "open",
            }),
        }
    }
}

pub(crate) fn gpui_app_modal_kind_for_hotkey_action_id(
    action_id: &str,
) -> Option<GpuiAppModalKind> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-23:04:
    `runGhostexHotkeyAction` needs an explicit app-modal allowlist after shell, pane, sidebar, focus, and action-slot routes have run. Map the separate Quick Access command/session entry actions and legacy sidebar modal ids here without treating every unknown hotkey id as a modal candidate.
    */
    match action_id {
        "openSettings" => Some(GpuiAppModalKind::Settings),
        "openHotkeys" => Some(GpuiAppModalKind::Hotkeys),
        "openCommandPalette" => Some(GpuiAppModalKind::CommandPalette),
        "openFindPrompts" => Some(GpuiAppModalKind::FindPrompts),
        "openSessionSearchPalette" => Some(GpuiAppModalKind::PreviousSessions),
        "openPreviousSessions" => Some(GpuiAppModalKind::PreviousSessions),
        "agentsHub" | "openAgentsHub" => Some(GpuiAppModalKind::AgentsHub),
        "configureAgents" => Some(GpuiAppModalKind::ConfigureAgents),
        "actions" | "configureActions" => Some(GpuiAppModalKind::ConfigureActions),
        "openTargets" => Some(GpuiAppModalKind::OpenTargets),
        /*
        CDXC:SettingsExtensionsTab 2026-08-30:
        `openExtensions` is deliberately absent: the Extensions surface is a
        Settings tab now, so the shell route in `run_ghostex_hotkey_action`
        opens Settings with `initialTab: "extensions"` before this app-modal
        allowlist is consulted.
        */
        _ => None,
    }
}

fn extension_modal_id(id: ExtensionId) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static MODAL_IDS: OnceLock<Mutex<HashMap<ExtensionId, &'static str>>> = OnceLock::new();
    let mut modal_ids = MODAL_IDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("extension modal id cache lock poisoned");
    if let Some(modal_id) = modal_ids.get(&id) {
        return modal_id;
    }
    let modal_id = Box::leak(format!("extension:{}", id.as_str()).into_boxed_str());
    modal_ids.insert(id, modal_id);
    modal_id
}

fn extension_modal_window_size(id: ExtensionId) -> Size<Pixels> {
    const DEFAULT_WIDTH: f32 = 1120.0;
    const DEFAULT_HEIGHT: f32 = 850.0;
    const MAX_WIDTH: f32 = 1400.0;
    const MAX_HEIGHT: f32 = 900.0;

    let manifest_path = crate::shared_settings::ghostex_storage_paths()
        .extensions_dir()
        .join("installed")
        .join(id.as_str())
        .join("ghostex-extension.json");
    let modal = std::fs::read(manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|manifest| manifest.get("modal").cloned());
    let dimension = |name: &str, default: f32, cap: f32| {
        modal
            .as_ref()
            .and_then(|modal| modal.get(name))
            .and_then(serde_json::Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| value.min(cap))
            .unwrap_or(default)
    };
    size(
        px(dimension("width", DEFAULT_WIDTH, MAX_WIDTH)),
        px(dimension("height", DEFAULT_HEIGHT, MAX_HEIGHT)),
    )
}
