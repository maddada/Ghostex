// C1 wave-3 re-cluster: the GpuiAppModalKind enum (every native app-modal window's id, title, size, and open-message shape) and its hotkey-action lookup, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAppModalKind {
    AddProject,
    Settings,
    Hotkeys,
    MissingProjectFolder,
    CommandPalette,
    PreviousSessions,
    RecentProjects,
    DaemonSessions,
    PinnedPrompts,
    StashedPrompts,
    ScratchPad,
    AgentsHub,
    DelayedSend,
    RenameSession,
    ExportTranscriptResult,
    ConfigureAgents,
    ConfigureActions,
    OpenTargets,
    FirstLaunchSetup,
    WatchGhostexVideo,
    RemoteGxserverInstall,
    RemoteProjectPicker,
    Worktree,
    DeleteWorktree,
    RenameWorktree,
    GitCommit,
    GitFileDiff,
    PortlessSetup,
    DiscoverGhostex,
    UpdateAvailable,
}


impl GpuiAppModalKind {
    pub(crate) fn from_modal_id(value: &str) -> Option<Self> {
        match value {
            "addProject" => Some(Self::AddProject),
            "settings" => Some(Self::Settings),
            "hotkeys" => Some(Self::Hotkeys),
            "missingProjectFolder" => Some(Self::MissingProjectFolder),
            "commandPalette" => Some(Self::CommandPalette),
            "previousSessions" => Some(Self::PreviousSessions),
            "recentProjects" => Some(Self::RecentProjects),
            "daemonSessions" => Some(Self::DaemonSessions),
            "pinnedPrompts" => Some(Self::PinnedPrompts),
            "stashedPrompts" => Some(Self::StashedPrompts),
            "scratchPad" => Some(Self::ScratchPad),
            "agentsHub" => Some(Self::AgentsHub),
            "delayedSend" => Some(Self::DelayedSend),
            "renameSession" => Some(Self::RenameSession),
            "exportTranscriptResult" => Some(Self::ExportTranscriptResult),
            "configureAgents" => Some(Self::ConfigureAgents),
            "configureActions" => Some(Self::ConfigureActions),
            "openTargets" => Some(Self::OpenTargets),
            "firstLaunchSetup" | "tipsAndTricks" => Some(Self::FirstLaunchSetup),
            "watchGhostexVideo" => Some(Self::WatchGhostexVideo),
            "remoteGxserverInstall" => Some(Self::RemoteGxserverInstall),
            "remoteProjectPicker" => Some(Self::RemoteProjectPicker),
            "worktree" => Some(Self::Worktree),
            "deleteWorktree" => Some(Self::DeleteWorktree),
            "renameWorktree" => Some(Self::RenameWorktree),
            "gitCommit" => Some(Self::GitCommit),
            "gitFileDiff" => Some(Self::GitFileDiff),
            "portlessSetup" => Some(Self::PortlessSetup),
            "discoverGhostex" => Some(Self::DiscoverGhostex),
            "updateAvailable" => Some(Self::UpdateAvailable),
            _ => None,
        }
    }

    pub(crate) fn modal_id(self) -> &'static str {
        match self {
            Self::AddProject => "addProject",
            Self::Settings => "settings",
            Self::Hotkeys => "hotkeys",
            Self::MissingProjectFolder => "missingProjectFolder",
            Self::CommandPalette => "commandPalette",
            Self::PreviousSessions => "previousSessions",
            Self::RecentProjects => "recentProjects",
            Self::DaemonSessions => "daemonSessions",
            Self::PinnedPrompts => "pinnedPrompts",
            Self::StashedPrompts => "stashedPrompts",
            Self::ScratchPad => "scratchPad",
            Self::AgentsHub => "agentsHub",
            Self::DelayedSend => "delayedSend",
            Self::RenameSession => "renameSession",
            Self::ExportTranscriptResult => "exportTranscriptResult",
            Self::ConfigureAgents => "configureAgents",
            Self::ConfigureActions => "configureActions",
            Self::OpenTargets => "openTargets",
            Self::FirstLaunchSetup => "firstLaunchSetup",
            Self::WatchGhostexVideo => "watchGhostexVideo",
            Self::RemoteGxserverInstall => "remoteGxserverInstall",
            Self::RemoteProjectPicker => "remoteProjectPicker",
            Self::Worktree => "worktree",
            Self::DeleteWorktree => "deleteWorktree",
            Self::RenameWorktree => "renameWorktree",
            Self::GitCommit => "gitCommit",
            Self::GitFileDiff => "gitFileDiff",
            Self::PortlessSetup => "portlessSetup",
            Self::DiscoverGhostex => "discoverGhostex",
            Self::UpdateAvailable => "updateAvailable",
        }
    }

    pub(crate) fn window_title(self) -> &'static str {
        match self {
            Self::AddProject => "Ghostex Add Project",
            Self::Settings => "Ghostex Settings",
            Self::Hotkeys => "Ghostex Hotkeys",
            Self::MissingProjectFolder => "Ghostex Project Folder Missing",
            Self::CommandPalette
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts => "Ghostex Quick Access",
            Self::DaemonSessions => "Ghostex Running Sessions",
            Self::PinnedPrompts => "Ghostex Pinned Prompts",
            Self::ScratchPad => "Ghostex Scratch Pad",
            Self::AgentsHub => "Ghostex Agents Hub",
            Self::DelayedSend => "Ghostex Session Automations",
            Self::RenameSession => "Ghostex Rename Session",
            Self::ExportTranscriptResult => "Ghostex Export Transcript",
            Self::ConfigureAgents => "Ghostex Configure Agents",
            Self::ConfigureActions => "Ghostex Actions",
            Self::OpenTargets => "Ghostex Open Targets",
            Self::FirstLaunchSetup => "Ghostex Tips",
            Self::WatchGhostexVideo => "Ghostex Tutorial Video",
            Self::RemoteGxserverInstall => "Ghostex Remote Setup",
            Self::RemoteProjectPicker => "Ghostex Remote Project",
            Self::Worktree => "Ghostex Add Worktree",
            Self::DeleteWorktree => "Ghostex Delete Worktree",
            Self::RenameWorktree => "Ghostex Rename Worktree",
            Self::GitCommit => "Ghostex Commit Changes",
            Self::GitFileDiff => "Ghostex File Diff",
            Self::PortlessSetup => "Ghostex Portless Setup",
            Self::DiscoverGhostex => "Discover Ghostex",
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
            Self::RenameSession => size(
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
            Self::PinnedPrompts => size(
                px(APP_MODAL_HOST_COMPACT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_WINDOW_HEIGHT),
            ),
            Self::DaemonSessions => size(
                px(APP_MODAL_HOST_DAEMON_SESSIONS_WINDOW_WIDTH),
                px(APP_MODAL_HOST_WINDOW_HEIGHT),
            ),
            Self::ScratchPad => size(
                px(APP_MODAL_HOST_SCRATCH_PAD_WINDOW_WIDTH),
                px(APP_MODAL_HOST_SCRATCH_PAD_WINDOW_HEIGHT),
            ),
            /*
            CDXC:GPUIAppModalSizes 2026-07-26-07:20:
            Settings, Hotkeys, Configure Agents, Configure Actions, and Open Targets all render the one tabbed Settings dialog in the modal host, so they must keep the full Settings frame even though their legacy standalone stylesheets are narrower.
            */
            Self::Settings
            | Self::Hotkeys
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
            Self::PortlessSetup => size(
                px(APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_WIDTH),
                px(APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_HEIGHT),
            ),
            Self::UpdateAvailable => size(
                px(APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_HEIGHT),
            ),
            Self::DiscoverGhostex => size(px(1120.0), px(850.0)),
            Self::RemoteGxserverInstall => size(
                px(APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_WIDTH),
                px(APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_HEIGHT),
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
        */
        false
    }

    pub(crate) fn window_min_size(self) -> Size<Pixels> {
        self.window_size()
    }

    pub(crate) fn uses_react_modal_host(self) -> bool {
        self != Self::WatchGhostexVideo
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
                | Self::PinnedPrompts
                | Self::ScratchPad
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
            | Self::ConfigureAgents
            | Self::ConfigureActions
            | Self::OpenTargets
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::DaemonSessions
            | Self::PinnedPrompts
            | Self::StashedPrompts
            | Self::ScratchPad
            | Self::AgentsHub
            | Self::DelayedSend
            | Self::RenameSession
            | Self::WatchGhostexVideo => serde_json::json!({
                "modal": self.modal_id(),
                "type": "open",
            }),
            // CDXC:GPUIFirstLaunchTutorialVideo 2026-08-19: the setup modal's
            // first page plays the tutorial, and it cannot embed YouTube from
            // the file:// modal host, so it is handed the app-served player
            // page instead.
            Self::FirstLaunchSetup => serde_json::json!({
                "modal": self.modal_id(),
                "tutorialVideoEmbedUrl": cef::app_served_resource_url(
                    GPUI_TUTORIAL_VIDEO_PLAYER_RESOURCE_PATH,
                ),
                "type": "open",
            }),
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
            // open message is the menu-path shape.
            Self::Worktree
            | Self::DeleteWorktree
            | Self::RenameWorktree
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


pub(crate) fn gpui_app_modal_kind_for_hotkey_action_id(action_id: &str) -> Option<GpuiAppModalKind> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-23:04:
    `runGhostexHotkeyAction` needs an explicit app-modal allowlist after shell, pane, sidebar, focus, and action-slot routes have run. Map the separate Quick Access command/session entry actions and legacy sidebar modal ids here without treating every unknown hotkey id as a modal candidate.
    */
    match action_id {
        "openSettings" => Some(GpuiAppModalKind::Settings),
        "openHotkeys" => Some(GpuiAppModalKind::Hotkeys),
        "openCommandPalette" => Some(GpuiAppModalKind::CommandPalette),
        "openSessionSearchPalette" => Some(GpuiAppModalKind::PreviousSessions),
        "openPreviousSessions" => Some(GpuiAppModalKind::PreviousSessions),
        "daemonSessions" | "openDaemonSessions" => Some(GpuiAppModalKind::DaemonSessions),
        "pinnedPrompts" | "openPinnedPrompts" => Some(GpuiAppModalKind::PinnedPrompts),
        "scratchPad" | "openScratchPad" => Some(GpuiAppModalKind::ScratchPad),
        "agentsHub" | "openAgentsHub" => Some(GpuiAppModalKind::AgentsHub),
        "configureAgents" => Some(GpuiAppModalKind::ConfigureAgents),
        "actions" | "configureActions" => Some(GpuiAppModalKind::ConfigureActions),
        "openTargets" => Some(GpuiAppModalKind::OpenTargets),
        _ => None,
    }
}
