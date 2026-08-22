// C1 wave-3 extraction: a chunk (1/6, in original file order) of the remaining plain value-type enums/structs/small helper fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


#[derive(Clone, Copy)]
pub(crate) struct GpuiNativeTitlebarTip {
    pub(crate) body: &'static str,
    pub(crate) icon_path: &'static str,
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
}


#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceProcess {
    pub(crate) command: String,
    pub(crate) cpu: f64,
    pub(crate) pid: u32,
    pub(crate) ppid: u32,
    pub(crate) memory_mb: f64,
    pub(crate) system_pid: u32,
}


/// One TCP listener sampled for the titlebar Resources "Dev Servers" section.
#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceServer {
    pub(crate) label: String,
    pub(crate) pid: u32,
    pub(crate) port: u16,
    pub(crate) url: String,
}


#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceRow {
    pub(crate) action: GpuiNativeResourceAction,
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) children: Vec<GpuiNativeResourceChild>,
    pub(crate) cpu: f64,
    pub(crate) detail: String,
    pub(crate) icon_path: &'static str,
    pub(crate) label: String,
    pub(crate) memory_mb: f64,
    pub(crate) pids: Vec<u32>,
    pub(crate) session_id: Option<String>,
    pub(crate) url: Option<String>,
}


#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceChild {
    pub(crate) cpu: f64,
    pub(crate) label: String,
    pub(crate) memory_mb: f64,
    pub(crate) pid: u32,
}


#[derive(Clone, Debug)]
pub(crate) enum GpuiNativeResourceAction {
    Browser(BrowserTabId),
    Code,
    None,
    Orphan,
    Server,
    Session,
}


#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiNativeResourcesSnapshot {
    pub(crate) browser_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) code_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) inactive_terminal_sleep_count: usize,
    pub(crate) orphan_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) persistent_session_mode: bool,
    pub(crate) project_label: String,
    pub(crate) server_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) session_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) sleep_all_session_count: usize,
    pub(crate) total_cpu: f64,
    pub(crate) total_memory_mb: f64,
}


/// Fixed selector set for titlebar Git menu rows. Menu selections dispatch
/// only one of these validated selectors back into the sidebar runtime's
/// `runSidebarGitAction` path; labels, branch text, and reasons from the
/// renderer never become action payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarGitMenuActionId {
    Commit,
    Push,
    Pr,
    SyncMain,
    SyncRemote,
    MultiRelease,
    Release,
}


impl GpuiTitlebarGitMenuActionId {
    pub(crate) fn from_selector(value: &str) -> Option<Self> {
        match value {
            "commit" => Some(Self::Commit),
            "push" => Some(Self::Push),
            "pr" => Some(Self::Pr),
            "syncMain" => Some(Self::SyncMain),
            "syncRemote" => Some(Self::SyncRemote),
            "multiRelease" => Some(Self::MultiRelease),
            "release" => Some(Self::Release),
            _ => None,
        }
    }

    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::Pr => "pr",
            Self::SyncMain => "syncMain",
            Self::SyncRemote => "syncRemote",
            Self::MultiRelease => "multiRelease",
            Self::Release => "release",
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarGitMenuRow {
    pub(crate) action: GpuiTitlebarGitMenuActionId,
    pub(crate) disabled: bool,
    pub(crate) label: String,
    pub(crate) primary: bool,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarGitMenuState {
    pub(crate) additions: u64,
    pub(crate) ahead_count: u64,
    pub(crate) behind_count: u64,
    pub(crate) branch: Option<String>,
    pub(crate) deletions: u64,
    pub(crate) has_working_tree_changes: bool,
    pub(crate) is_busy: bool,
    pub(crate) is_repo: bool,
    pub(crate) primary_action: GpuiTitlebarGitMenuActionId,
    pub(crate) rows: Vec<GpuiTitlebarGitMenuRow>,
    pub(crate) sync_remote_disabled: bool,
}


pub(crate) fn gpui_titlebar_git_menu_state_from_payload(payload: &str) -> Option<GpuiTitlebarGitMenuState> {
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(serde_json::Value::as_str)
        != Some(GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE)
        || object.get("version").and_then(serde_json::Value::as_u64)
            != Some(GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION)
    {
        return None;
    }
    let rows_value = object.get("rows").and_then(serde_json::Value::as_array)?;
    if rows_value.len() > GPUI_TITLEBAR_GIT_MENU_MAX_ROWS {
        return None;
    }
    let mut rows = Vec::with_capacity(rows_value.len());
    for row_value in rows_value {
        let row = row_value.as_object()?;
        let action = GpuiTitlebarGitMenuActionId::from_selector(
            row.get("action").and_then(serde_json::Value::as_str)?,
        )?;
        let label = bounded_gpui_titlebar_git_menu_text(
            row.get("label").and_then(serde_json::Value::as_str)?,
            GPUI_TITLEBAR_GIT_MENU_ROW_LABEL_MAX_CHARS,
        )?;
        rows.push(GpuiTitlebarGitMenuRow {
            action,
            disabled: row.get("disabled").and_then(serde_json::Value::as_bool) == Some(true),
            label,
            primary: row.get("primary").and_then(serde_json::Value::as_bool) == Some(true),
        });
    }
    let branch = match object.get("branch") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(branch)) => {
            bounded_gpui_titlebar_git_menu_text(branch, GPUI_TITLEBAR_GIT_MENU_BRANCH_MAX_CHARS)
        }
        Some(_) => return None,
    };
    Some(GpuiTitlebarGitMenuState {
        additions: object
            .get("additions")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        ahead_count: object
            .get("aheadCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        behind_count: object
            .get("behindCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        branch,
        deletions: object
            .get("deletions")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        has_working_tree_changes: object
            .get("hasWorkingTreeChanges")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        is_busy: object.get("isBusy").and_then(serde_json::Value::as_bool) == Some(true),
        is_repo: object.get("isRepo").and_then(serde_json::Value::as_bool) == Some(true),
        primary_action: object
            .get("primaryAction")
            .and_then(serde_json::Value::as_str)
            .and_then(GpuiTitlebarGitMenuActionId::from_selector)?,
        rows,
        sync_remote_disabled: object
            .get("syncRemoteDisabled")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    })
}


pub(crate) fn bounded_gpui_titlebar_git_menu_text(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(max_chars).collect())
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BrowserProfileId(pub(crate) u64);


impl BrowserProfileId {
    pub(crate) fn default_profile() -> Self {
        Self(BROWSER_PROFILE_DEFAULT_ID)
    }

    pub(crate) fn display_label(self) -> String {
        format!("Profile {}", self.0)
    }

    pub(crate) fn display_number(self) -> Option<u64> {
        (self != Self::default_profile()).then_some(self.0)
    }

    pub(crate) fn cef_profile_string(self) -> String {
        if self == Self::default_profile() {
            BROWSER_PROFILE_DEFAULT_CEF_ID.to_string()
        } else {
            format!("profile-{}", self.0)
        }
    }
}


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


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TitlebarMode {
    Agents,
    Source,
    Browser,
    Kanban,
    Automate,
    Manage,
}


impl TitlebarMode {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "agents" => Some(Self::Agents),
            "source" => Some(Self::Source),
            "browser" => Some(Self::Browser),
            "kanban" => Some(Self::Kanban),
            "automate" => Some(Self::Automate),
            "manage" => Some(Self::Manage),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Source => "source",
            Self::Browser => "browser",
            Self::Kanban => "kanban",
            Self::Automate => "automate",
            Self::Manage => "manage",
        }
    }

    pub(crate) fn display_label(self) -> &'static str {
        match self {
            Self::Agents => "Agents",
            Self::Source => "Code",
            Self::Browser => "Browser",
            Self::Kanban => "Kanban",
            Self::Automate => "Automate",
            Self::Manage => "Docs",
        }
    }

    pub(crate) fn is_project_editor_mode(self) -> bool {
        matches!(
            self,
            Self::Source | Self::Browser | Self::Kanban | Self::Automate | Self::Manage
        )
    }

    pub(crate) fn project_editor_order(self) -> u64 {
        match self {
            Self::Source => 0,
            Self::Browser => 1,
            Self::Kanban => 2,
            Self::Automate => 3,
            Self::Manage => 4,
            Self::Agents => 5,
        }
    }

    pub(crate) fn switcher_index(self) -> u64 {
        match self {
            Self::Agents => 0,
            Self::Source => 1,
            Self::Browser => 2,
            Self::Kanban => 3,
            Self::Automate => 4,
            Self::Manage => 5,
        }
    }

    pub(crate) fn from_switcher_index(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::Agents),
            1 => Some(Self::Source),
            2 => Some(Self::Browser),
            3 => Some(Self::Kanban),
            4 => Some(Self::Automate),
            5 => Some(Self::Manage),
            _ => None,
        }
    }

    pub(crate) fn placeholder_message(self) -> &'static str {
        match self {
            Self::Agents => "",
            Self::Source => "Source is unavailable for the current project context.",
            Self::Browser => "",
            Self::Kanban => "Kanban is unavailable for the current project context.",
            Self::Automate => "Automate is unavailable for the current project context.",
            Self::Manage => "Docs is unavailable for the current project context.",
        }
    }
}


/*
CDXC:GPUIProjectEditorPlaceholders 2026-06-28-17:09:
Source, Kanban, Automate, and Docs neutral placeholders are unavailable/loading/error surfaces only. Real Source/Kanban/Automate/Docs replacement is owned by the direct runtime URL plus normal-layout CefSurface gate, so placeholder rendering must not create CEF views, start code-server, run file operations, synthesize fallback URLs, persist private details, or add WKWebView/WebKit paths.
*/
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProjectEditorPlaceholderSignature {
    pub(crate) mode: TitlebarMode,
    pub(crate) title: Option<String>,
    pub(crate) message: String,
    pub(crate) actions: Vec<ProjectEditorPlaceholderAction>,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectEditorPlaceholderAction {
    HideCodeViewTab,
    InstallSourceComponent,
    RetrySourceLoad,
}


impl ProjectEditorPlaceholderSignature {
    pub(crate) fn for_mode(mode: TitlebarMode) -> Option<Self> {
        if matches!(mode, TitlebarMode::Agents | TitlebarMode::Browser) {
            return None;
        }

        Some(Self {
            mode,
            title: None,
            message: mode.placeholder_message().to_string(),
            actions: Vec::new(),
        })
    }

    pub(crate) fn for_source_code_server_launch_state(
        state: SourceCodeServerRuntimeLaunchState,
        loading_elapsed: Option<Duration>,
    ) -> Option<Self> {
        let signature = Self::for_mode(TitlebarMode::Source)?;
        let (title, message, actions) = match state {
            SourceCodeServerRuntimeLaunchState::Launching
                if loading_elapsed.is_some_and(|elapsed| {
                    elapsed < SOURCE_CODE_SERVER_LOADING_PLACEHOLDER_DELAY
                }) =>
            {
                (None, "".to_string(), Vec::new())
            }
            SourceCodeServerRuntimeLaunchState::Launching => (
                Some("Loading source...".to_string()),
                "".to_string(),
                Vec::new(),
            ),
            _ => return Some(signature),
        };

        Some(Self {
            title,
            message,
            actions,
            ..signature
        })
    }
}


/*
CDXC:GPUIProjectEditorSleepingPlaceholders 2026-06-28-17:09:
Selected sleeping/restored project-editor modes remain real layout participants with neutral text-only shell surfaces. Surface activation expresses wake intent for shell state; Browser hides existing CEF while sleeping, and Source/Kanban/Automate/Docs must not mount or replace runtime surfaces until their awake direct CEF gates permit it.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectEditorSleepingPlaceholderSignature {
    pub(crate) mode: TitlebarMode,
    pub(crate) title: &'static str,
    pub(crate) message: &'static str,
}


impl ProjectEditorSleepingPlaceholderSignature {
    pub(crate) fn for_mode(mode: TitlebarMode) -> Option<Self> {
        /*
        CDXC:GPUIProjectEditorSleepingPlaceholder 2026-06-28-17:09:
        Sleeping/restored Source, Browser, Kanban, Automate, and Docs visible copy is private-detail-free shell state. It must not include project/session/URL details, create CEF views, mount bridges, replace placeholders, or introduce WKWebView/WebKit paths.
        */
        let (title, message) = match mode {
            TitlebarMode::Source => (
                "Source is sleeping",
                "Source shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Browser => (
                "Browser is sleeping",
                "Browser shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Kanban => (
                "Kanban is sleeping",
                "Kanban shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Automate => (
                "Automate is sleeping",
                "Automate shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Manage => (
                "Docs is sleeping",
                "Docs shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Agents => return None,
        };

        Some(Self {
            mode,
            title,
            message,
        })
    }
}


#[derive(Clone, Copy)]
pub(crate) struct SidebarDragState {
    pub(crate) start_x: f32,
    pub(crate) start_width: f32,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarSide {
    Left,
    Right,
}


impl GpuiSidebarSide {
    pub(crate) fn from_settings_value(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}


/*
CDXC:GPUICommandPaneSide 2026-08-16:
Command pane placement is placement-only shell state sourced from shared
Settings (`commandsPanelSide`). Bottom keeps the historical pinned/collapsed
layout; Right renders the pinned pane as a workspace column with a vertical
resize rail. The collapsed footer strip stays at the bottom on both sides so
the pane remains discoverable from the same place.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandPaneSide {
    Bottom,
    Right,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarBodyChromePart {
    Sidebar,
    Divider,
    Workspace,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TitlebarModeSwitcherItem {
    pub(crate) mode: TitlebarMode,
    pub(crate) is_available: bool,
    pub(crate) disabled_reason: Option<&'static str>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarExitFocusControlSignature {
    pub(crate) label: &'static str,
    pub(crate) styled_as_active_mode_tab: bool,
    pub(crate) clears_agents_focus_mode: bool,
}


pub(crate) fn gpui_titlebar_exit_focus_control_signature(
    agents_focus_mode_active: bool,
) -> Option<GpuiTitlebarExitFocusControlSignature> {
    /*
    CDXC:GPUITitlebarFocusExit 2026-06-27-02:05:
    The titlebar Exit Focus affordance is visible only while the Agents workspace is in pane Focus mode, and it must reuse active mode-tab chrome instead of a separate outlined or icon-button skin. Activating it clears Agents focus mode through the workspace model without changing command-pane focus mode, project-editor focus, terminal content, paths, commands, or renderer state.
    */
    agents_focus_mode_active.then_some(GpuiTitlebarExitFocusControlSignature {
        label: "Exit focus",
        styled_as_active_mode_tab: true,
        clears_agents_focus_mode: true,
    })
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTabSession {
    pub(crate) activity: AgentTerminalActivity,
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) key: GpuiWorkspaceTerminalSessionKey,
    pub(crate) kind: AgentsWorkspaceSessionKind,
    pub(crate) is_generating_first_prompt_title: bool,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) title: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalFocusMessage {
    pub(crate) force_remount: bool,
    pub(crate) placement_target_session_id: Option<String>,
    pub(crate) preferred_interface: GpuiPreferredAgentInterface,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuiPreferredAgentInterface {
    Chat,
    #[default]
    Terminal,
}


impl GpuiPreferredAgentInterface {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(Self::Chat),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarCreateProjectAgentMessage {
    pub(crate) agent_id: String,
    pub(crate) preferred_interface: GpuiPreferredAgentInterface,
    pub(crate) project_id: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarCreateProjectTerminalMessage {
    pub(crate) project_id: String,
}


/// True for sidebar bridge events that act on per-project runtime state, so
/// they must not run ahead of a project switch that is still queued behind the
/// settle window. The listed pass-through events are project-agnostic status,
/// telemetry, and compatibility no-ops; flushing on those would defeat the
/// debounce because they arrive on every presentation publish.
pub(crate) fn gpui_sidebar_bridge_event_must_follow_pending_project_switch(
    event: &cef::SidebarBridgeEvent,
) -> bool {
    !matches!(
        event,
        cef::SidebarBridgeEvent::ActiveProjectContext(_)
            | cef::SidebarBridgeEvent::GxserverPresentationFocusState(_)
            | cef::SidebarBridgeEvent::WorkspaceTerminalFocus(_)
            | cef::SidebarBridgeEvent::SessionCompletionSound(_)
            | cef::SidebarBridgeEvent::SessionStatusIndicators(_)
            | cef::SidebarBridgeEvent::PetOverlayState(_)
            | cef::SidebarBridgeEvent::TitlebarGitMenuState(_)
            | cef::SidebarBridgeEvent::ProjectBoardConversationResponse(_)
            | cef::SidebarBridgeEvent::SourceWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::BrowserWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ProjectWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ManageFileWorkareaOperationRequest(_)
    )
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiBrowserRendererOpenReuse {
    Exact,
    None,
    Similar,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarOpenBrowserUrlMessage {
    pub(crate) url: String,
    pub(crate) reuse: GpuiBrowserRendererOpenReuse,
    pub(crate) from_quick_header: bool,
    pub(crate) project_id: Option<String>,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarBrowserTabFocusMessage {
    pub(crate) project_id: String,
    pub(crate) tab_id: BrowserTabId,
}


/*
CDXC:GPUISidebarRename 2026-07-28:
`command` is a fixed selector, not renderer-provided command text: it may only
be the literal "rename" (default), "name" (Pi), or "title" (Hermes Agent), and
Rust alone turns it into terminal input.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceTerminalRenameCommandKind {
    Name,
    Rename,
    Title,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalRenameCommandMessage {
    pub(crate) command: GpuiWorkspaceTerminalRenameCommandKind,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) title: String,
}


/*
CDXC:GPUIWorkspaceRenameCommand 2026-07-29:
Rename delivery selects the target tab first, so its Ghostty surface may still
be mounting when the command arrives. The bounded timer below re-validates the
same exact target until the surface mounts; it never retargets or falls back.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceRenameCommandDelivery {
    Delivered,
    SurfaceNotMounted,
    TargetInvalid,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalEnterMessage {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GpuiLocalWorkspaceSessionKey {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandTerminalCreateInput {
    pub(crate) command_id: Option<String>,
    pub(crate) command_title: Option<String>,
    pub(crate) cwd: String,
    pub(crate) project_id: String,
    pub(crate) startup_text: Option<String>,
    pub(crate) title: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandTerminalCreateInputResolution {
    Ready(GpuiCommandTerminalCreateInput),
    NotReady,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandTerminalAttachPlan {
    pub(crate) attach_command: String,
    pub(crate) command_id: Option<String>,
    pub(crate) initial_input: Option<String>,
    pub(crate) key: GpuiLocalWorkspaceSessionKey,
    pub(crate) title: String,
    pub(crate) working_directory: Option<String>,
    pub(crate) zmx_name: Option<String>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceAttachIntent {
    Attach,
    Wake,
}


impl GpuiLocalWorkspaceAttachIntent {
    pub(crate) fn rpc_path(self) -> &'static str {
        match self {
            Self::Attach => "/api/attachSessionMetadata",
            Self::Wake => "/api/wakeSession",
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceAttachOrigin {
    SidebarFocus,
    SurfacedRestore,
    WakeRecovery,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiWorkspaceTerminalRenameCommandTarget {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) needs_tab_selection: bool,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalLifecycleResultMessage {
    pub(crate) ok: bool,
    pub(crate) request_id: u64,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceLifecycleAction {
    Close,
    Sleep,
    Wake,
}


impl GpuiLocalWorkspaceLifecycleAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Sleep => "sleep",
            Self::Wake => "wake",
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceLifecycleMutationKind {
    DirectClose,
    ScopedClose,
    DirectSleep,
    DirectWake,
    ScopedSleep,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiLocalWorkspaceLifecycleRequest {
    pub(crate) action: GpuiLocalWorkspaceLifecycleAction,
    pub(crate) confirmed_close_slot_id: Option<AgentsTerminalBodyMountSlotId>,
    pub(crate) mutation_kind: GpuiLocalWorkspaceLifecycleMutationKind,
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) replacement_shell_session_id: Option<TerminalSessionId>,
    pub(crate) shell_session_id: TerminalSessionId,
}


pub(crate) fn gpui_local_workspace_lifecycle_request_is_pending(
    requests: &HashMap<u64, GpuiLocalWorkspaceLifecycleRequest>,
    request: &GpuiLocalWorkspaceLifecycleRequest,
) -> bool {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-27-00:33:
    Pending mapped Sleep/Wake requests must de-dupe only exact native mutations. Direct/scoped Sleep, replacement focus, and pane origin carry different macOS tab semantics, so session/action-only de-dupe can apply the wrong UX when a second request races an async SidebarApp ack. Close is local-first and never enters this pending set.
    */
    requests.values().any(|pending| pending == request)
}


pub(crate) fn gpui_workspace_terminal_rename_command_target_from_model(
    workspace: &WorkspaceModel,
    local_workspace_session_mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    key: &GpuiLocalWorkspaceSessionKey,
) -> Option<GpuiWorkspaceTerminalRenameCommandTarget> {
    /*
    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    Rename-command target selection is model-only until the final Ghostty owner check: require an existing local gxserver mapping, a Running Agents shell session, and a pane that owns that tab. Sleeping, mounting, restored, popped-out, stale, unmapped, command-pane, and fallback-focused terminals must not become rename targets.
    */
    let shell_session_id = local_workspace_session_mappings.get(key).copied()?;
    if !workspace.session(shell_session_id).is_some_and(|session| {
        session.presentation_state == TerminalSessionPresentationState::Running
    }) {
        return None;
    }
    let pane_id = workspace.pane_id_for_session(shell_session_id)?;
    let active_session_id = workspace
        .find_leaf(pane_id)
        .and_then(|leaf| leaf.tab_group.active_session_id());
    Some(GpuiWorkspaceTerminalRenameCommandTarget {
        pane_id,
        shell_session_id,
        slot_id: AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        },
        needs_tab_selection: active_session_id != Some(shell_session_id),
    })
}


impl From<&GpuiSidebarWorkspaceTerminalFocusMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalFocusMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}


impl From<&GpuiSidebarWorkspaceTerminalRenameCommandMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalRenameCommandMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}


impl From<&GpuiSidebarWorkspaceTerminalEnterMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalEnterMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}


/*
Agents Hub Source opens are process-local navigation intent. Keep the validated
file and its containing workspace only until the matching owned Source surface
is ready, then hand the file to code-server IPC. Never persist or log either
path, and never accept a path that was not present in the current Hub catalog.
*/
pub(crate) struct PendingSourceFileOpen {
    pub(crate) file_path: PathBuf,
    pub(crate) project_path: PathBuf,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TerminalSessionId(pub(crate) u64);


/*
CDXC:GPUITerminalRuntimeIdentity 2026-06-22-23:24:
Phase 3 separates process-lifetime Agents terminal runtime identity from durable shell `TerminalSessionId` and pane/body mount slots. Runtime ids bind Ghostty owners to the current app process only; they are not user-facing titles, not logs, not shell-state fields, and restored shell sessions intentionally receive fresh runtime ids.
*/
pub(crate) struct AgentsTerminalRuntimeSessionRegistry {
    pub(crate) runtime_ids_by_shell_session: HashMap<TerminalSessionId, AgentsTerminalRuntimeSessionId>,
    pub(crate) next_runtime_session_id: u64,
}


/*
CDXC:GPUIAgentsTerminalRuntimePerProject 2026-08-05:
Inactive project workspaces keep their live composited terminal owners beside
their parked shell models. The entities own the local shell/SSH attach clients,
so dropping them during a project or machine switch forces a fresh zmx attach
even though the persisted tab still says Running. Runtime ids, terminal
entities, OSC state, and close-confirm intent stay process-local and return only
to the exact project that parked them; none of this state is serialized.
*/
#[derive(Default)]
pub(crate) struct ParkedAgentsTerminalRuntime {
    pub(crate) runtime_sessions: AgentsTerminalRuntimeSessionRegistry,
    pub(crate) gpui_engine_terminals:
        HashMap<TerminalSessionId, terminal_gpui_engine::GpuiEngineTerminalRecord>,
    pub(crate) runtime_osc_states: HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
    pub(crate) gpui_engine_close_confirms: HashSet<AgentsTerminalBodyMountSlotId>,
}


impl Default for AgentsTerminalRuntimeSessionRegistry {
    fn default() -> Self {
        Self {
            runtime_ids_by_shell_session: HashMap::new(),
            next_runtime_session_id: 1,
        }
    }
}


impl AgentsTerminalRuntimeSessionRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reconcile_with_workspace(&mut self, workspace: &WorkspaceModel) {
        let shell_session_ids = workspace.terminal_session_ids();
        let current_shell_session_ids = shell_session_ids.iter().copied().collect::<HashSet<_>>();
        self.runtime_ids_by_shell_session
            .retain(|session_id, _| current_shell_session_ids.contains(session_id));

        for session_id in shell_session_ids {
            self.ensure_runtime_session_id(session_id);
        }
    }

    pub(crate) fn runtime_session_id_for_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        self.runtime_ids_by_shell_session.get(&session_id).copied()
    }

    pub(crate) fn ensure_runtime_session_id(
        &mut self,
        session_id: TerminalSessionId,
    ) -> AgentsTerminalRuntimeSessionId {
        if let Some(runtime_session_id) = self.runtime_ids_by_shell_session.get(&session_id) {
            return *runtime_session_id;
        }

        let runtime_session_id = self.allocate_runtime_session_id();
        self.runtime_ids_by_shell_session
            .insert(session_id, runtime_session_id);
        runtime_session_id
    }

    pub(crate) fn rotate_runtime_session_id_for_shell_session(
        &mut self,
        session_id: TerminalSessionId,
    ) -> AgentsTerminalRuntimeSessionId {
        /*
        CDXC:GPUITerminalStartupRetryIdentity 2026-06-23-18:19:
        Explicit failed-startup retry is a new process-local runtime attempt for the same durable shell session. Rotate only the runtime id so the retry startup candidate, launch plan, and completion intent cannot reuse stale attempt identity while the shell `TerminalSessionId`, tab, title, and persisted state remain unchanged.
        */
        let runtime_session_id = self.allocate_runtime_session_id();
        self.runtime_ids_by_shell_session
            .insert(session_id, runtime_session_id);
        runtime_session_id
    }

    pub(crate) fn allocate_runtime_session_id(&mut self) -> AgentsTerminalRuntimeSessionId {
        let runtime_session_id = AgentsTerminalRuntimeSessionId(self.next_runtime_session_id);
        self.next_runtime_session_id += 1;
        runtime_session_id
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupBodySlotId {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupBodyGeometry {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}


#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupLaunchPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}


/*
CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
Sleeping wake and popped-out reattach are runtime-owner moves, not startup attempts. Parked owner geometry and reattach plans stay process-local, require the same durable shell session, process-local runtime id, pane/session slot, and current body bounds, and must not create launch payloads, startup hosts, fallback surfaces, logs, shell-state fields, or fake Running state.
*/
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalParkedOwnerBodyGeometry {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalParkedOwnerReattachPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) parked_mount_slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) current_mount_slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}


#[cfg(target_os = "macos")]
impl AgentsTerminalParkedOwnerReattachPlan {
    pub(crate) fn attachment_plan(self) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
        terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
            host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
                self.current_mount_slot_id,
            ),
            slot_id: self.current_mount_slot_id,
            bounds: self.bounds,
        }
    }
}


#[cfg(target_os = "macos")]
pub(crate) struct AgentsTerminalParkedRuntimeOwner {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) mount_slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) host_native_view: terminal_native_view::AppOwnedTerminalHostNativeView,
    pub(crate) surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner,
}


#[cfg(target_os = "macos")]
impl AgentsTerminalParkedRuntimeOwner {
    pub(crate) fn new(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        shell_session_id: TerminalSessionId,
        mount_slot_id: AgentsTerminalBodyMountSlotId,
        host_native_view: terminal_native_view::AppOwnedTerminalHostNativeView,
        surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner,
    ) -> Self {
        Self {
            runtime_session_id,
            shell_session_id,
            mount_slot_id,
            host_native_view,
            surface_owner,
        }
    }

    pub(crate) fn matches_identity(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        shell_session_id: TerminalSessionId,
        mount_slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.runtime_session_id == runtime_session_id
            && self.shell_session_id == shell_session_id
            && self.mount_slot_id == mount_slot_id
            && self.surface_owner.mount_slot_id() == mount_slot_id
            && self.surface_owner.runtime_session_id() == runtime_session_id
    }

    pub(crate) fn can_reattach_with_plan(&self, plan: AgentsTerminalParkedOwnerReattachPlan) -> bool {
        self.matches_identity(
            plan.runtime_session_id,
            plan.shell_session_id,
            plan.parked_mount_slot_id,
        ) && self
            .host_native_view
            .can_move_to_running_attachment_plan(plan.attachment_plan())
            && self
                .surface_owner
                .can_move_to_mount_slot(plan.runtime_session_id)
    }

    pub(crate) fn into_running_owners(
        self,
        plan: AgentsTerminalParkedOwnerReattachPlan,
    ) -> (
        terminal_native_view::AppOwnedTerminalHostNativeView,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    ) {
        (
            self.host_native_view
                .into_rekeyed_running_host_native_view(plan.attachment_plan()),
            self.surface_owner
                .into_rekeyed_surface_owner(plan.current_mount_slot_id, plan.runtime_session_id),
        )
    }
}


/*
CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
Sleeping command terminals should park the existing AppKit host and Ghostty surface owner, not free and recreate them on wake. The parked owner is process-local and exact command group/session keyed; it must never infer ownership from titles, command text, cwd/env, terminal output, focus fallback, shell-state JSON, logs, or Agents runtime maps.
*/
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct CommandTerminalParkedOwnerReattachPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) parked_mount_slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) current_mount_slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) bounds: Bounds<Pixels>,
}


#[cfg(target_os = "macos")]
impl CommandTerminalParkedOwnerReattachPlan {
    pub(crate) fn attachment_plan(
        self,
    ) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan<CommandTerminalBodyMountSlotId>
    {
        terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
            host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
                self.current_mount_slot_id,
            ),
            slot_id: self.current_mount_slot_id,
            bounds: self.bounds,
        }
    }
}


#[cfg(target_os = "macos")]
pub(crate) struct CommandTerminalParkedRuntimeOwner {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) mount_slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) host_native_view:
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    pub(crate) surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
}


#[cfg(target_os = "macos")]
impl CommandTerminalParkedRuntimeOwner {
    pub(crate) fn new(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        mount_slot_id: CommandTerminalBodyMountSlotId,
        host_native_view: terminal_native_view::AppOwnedTerminalHostNativeView<
            CommandTerminalBodyMountSlotId,
        >,
        surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner<
            CommandTerminalBodyMountSlotId,
        >,
    ) -> Self {
        Self {
            runtime_session_id,
            mount_slot_id,
            host_native_view,
            surface_owner,
        }
    }

    pub(crate) fn matches_identity(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        mount_slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.runtime_session_id == runtime_session_id
            && self.mount_slot_id == mount_slot_id
            && self.surface_owner.mount_slot_id() == mount_slot_id
            && self.surface_owner.runtime_session_id() == runtime_session_id
    }

    pub(crate) fn can_reattach_with_plan(&self, plan: CommandTerminalParkedOwnerReattachPlan) -> bool {
        self.matches_identity(plan.runtime_session_id, plan.parked_mount_slot_id)
            && plan.parked_mount_slot_id == plan.current_mount_slot_id
            && self
                .host_native_view
                .can_rekey_to_running_attachment_plan(plan.attachment_plan())
            && self
                .surface_owner
                .can_rekey_to_mount_slot(plan.current_mount_slot_id, plan.runtime_session_id)
    }

    pub(crate) fn into_running_owners(
        self,
        plan: CommandTerminalParkedOwnerReattachPlan,
    ) -> (
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    ) {
        (
            self.host_native_view
                .into_rekeyed_running_host_native_view(plan.attachment_plan()),
            self.surface_owner
                .into_rekeyed_surface_owner(plan.current_mount_slot_id, plan.runtime_session_id),
        )
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupHostPreservationKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupRecord {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_geometry_available: bool,
}


impl AgentsTerminalStartupRecord {
    pub(crate) fn startup_body_slot_id(self) -> AgentsTerminalStartupBodySlotId {
        AgentsTerminalStartupBodySlotId {
            pane_id: self.pane_id,
            session_id: self.shell_session_id,
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupCompletionIntent {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}


impl AgentsTerminalStartupCompletionIntent {
    pub(crate) fn from_record(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        record: AgentsTerminalStartupRecord,
    ) -> Self {
        Self {
            runtime_session_id,
            shell_session_id: record.shell_session_id,
            startup_body_slot_id: record.startup_body_slot_id(),
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupReadinessSignalPreparation {
    pub(crate) completion_intent: AgentsTerminalStartupCompletionIntent,
    pub(crate) surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
}


impl AgentsTerminalStartupReadinessSignalPreparation {
    pub(crate) fn new(
        completion_intent: AgentsTerminalStartupCompletionIntent,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<Self> {
        (completion_intent.startup_body_slot_id == startup_body_slot_id
            && surface_metadata.indicates_ready_metadata())
        .then_some(Self {
            completion_intent,
            surface_metadata,
        })
    }
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupReadinessHandoffPlan {
    pub(crate) completion_intent: AgentsTerminalStartupCompletionIntent,
    pub(crate) startup_launch_plan: AgentsTerminalStartupLaunchPlan,
    pub(crate) mount_slot_id: AgentsTerminalBodyMountSlotId,
}


#[cfg(target_os = "macos")]
impl AgentsTerminalStartupReadinessHandoffPlan {
    pub(crate) fn runtime_session_id(self) -> AgentsTerminalRuntimeSessionId {
        self.completion_intent.runtime_session_id
    }

    pub(crate) fn startup_body_slot_id(self) -> AgentsTerminalStartupBodySlotId {
        self.completion_intent.startup_body_slot_id
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AgentsTerminalStartupCompletionSignal {
    Ready {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
    Failed {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
}


impl AgentsTerminalStartupCompletionSignal {
    pub(crate) fn completion_intent(self) -> AgentsTerminalStartupCompletionIntent {
        match self {
            Self::Ready { completion_intent } | Self::Failed { completion_intent } => {
                completion_intent
            }
        }
    }

    pub(crate) fn into_startup_result(self) -> AgentsTerminalStartupResult {
        match self {
            Self::Ready { completion_intent } => {
                AgentsTerminalStartupResult::Ready { completion_intent }
            }
            Self::Failed { completion_intent } => {
                AgentsTerminalStartupResult::Failed { completion_intent }
            }
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AgentsTerminalStartupResult {
    Ready {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
    Failed {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
}


impl AgentsTerminalStartupResult {
    pub(crate) fn completion_intent(self) -> AgentsTerminalStartupCompletionIntent {
        match self {
            Self::Ready { completion_intent } | Self::Failed { completion_intent } => {
                completion_intent
            }
        }
    }

    pub(crate) fn runtime_session_id(self) -> AgentsTerminalRuntimeSessionId {
        self.completion_intent().runtime_session_id
    }

    pub(crate) fn terminal_presentation_state(self) -> TerminalSessionPresentationState {
        match self {
            Self::Ready { .. } => TerminalSessionPresentationState::Running,
            Self::Failed { .. } => TerminalSessionPresentationState::StartupFailed,
        }
    }
}


/*
CDXC:GPUITerminalStartupBoundary 2026-06-22-23:50:
Agents terminal startup is a runtime-only boundary keyed by process-local runtime session id, not by durable shell session id or body mount slot id. Visible selected Mounting tabs may become pending startup records, but this layer does not launch a process, infer success, persist runtime ids, log commands, store cwd/env/stdout/stderr, or create fallback surfaces.

CDXC:GPUITerminalStartupBoundary 2026-06-22-23:50:
Startup results are intentionally enum-only. Ready may promote the same current Mounting shell session to Running; Failed preserves the tab as a safe failed-startup placeholder with no raw error string, command text, path, environment, terminal content, or process details in shell state.

CDXC:GPUITerminalStartupGeometry 2026-06-23-00:10:
Visible selected Mounting Agents bodies need runtime-only startup geometry for future launch preparation, but the startup slot id stays separate from the Running-only libghostty body mount slot so geometry alone never creates Running host state, a Ghostty surface, process, or Running transition.

CDXC:GPUITerminalStartupLaunchPlan 2026-06-23-00:22:
Phase 3 startup launch plans are a runtime-only readiness boundary for visible selected Mounting Agents bodies after exact body geometry exists. Plans may carry only runtime id, shell id, pane id, startup body slot id, bounds, and scale; they must not carry cwd, command, env, terminal content, stdout/stderr, process ids, logs, persisted fields, Ghostty hosts, Ghostty surfaces, Running mount slots, or Ready/Failed transitions by themselves.

CDXC:GPUITerminalStartupHostLifetime 2026-06-23-03:23:
A render-start geometry reset must not churn an already-created hidden startup host/config while the same pending Mounting tab remains current. Preserve only hosts previously created from a launch plan and only by matching runtime id plus `AgentsTerminalStartupBodySlotId`; pending records without prior geometry must not create hosts.

CDXC:GPUITerminalStartupCompletion 2026-06-23-03:51:
GPUI has no exposed GhosttyKit tty/pid or terminalReady-equivalent signal yet, so startup completion is a runtime-only intent plus explicit signal boundary. A current Mounting tab may advertise an exact runtime/session/startup-slot intent, but the producer returns no Ready/Failed result without a real signal and must never infer success from hidden startup host or Ghostty surface creation.

CDXC:GPUITerminalStartupLaunchPayloadSource 2026-06-23-04:00:
Startup config preparation now has a runtime-only launch-payload source boundary, but GPUI does not currently populate it because no explicit app startup state carries cwd, command, env vars, initial input, or wait-after-command. The empty source keeps startup requests inert until a future explicit producer is wired, and invalid future payloads must prune the startup boundary instead of falling back to inferred values.

CDXC:GPUITerminalStartupReadiness 2026-06-23-04:13:
Ghostty startup surface metadata is now a real runtime-only readiness input, but it may only create a handoff plan for the exact current startup completion intent when Ghostty reports a tty name and foreground process while the process has not exited. Promotion may proceed only when startup host/surface ownership can be moved into the Running path without dropping or recreating the process, and this layer must not create Failed, persist ids, log metadata, or expose raw tty names/process ids.

CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:38:
Startup-owned Ghostty metadata is also the real runtime failure input. A process-exited snapshot may produce only the existing Failed result for the exact current runtime/session/startup-slot intent while the shell tab is still the visible selected Mounting body and the startup surface owner identity still matches; cleanup must drop the startup Ghostty surface before the hidden host and must not create Running maps, fallback success, logs, raw process details, paths, commands, env, or terminal content.
*/
pub(crate) struct AgentsTerminalStartupCoordinator {
    pub(crate) pending_startups_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
    pub(crate) startup_launch_plans_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan>,
    pub(crate) startup_completion_intents_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent>,
    pub(crate) startup_readiness_signal_preparations_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupReadinessSignalPreparation>,
}


impl Default for AgentsTerminalStartupCoordinator {
    fn default() -> Self {
        Self {
            pending_startups_by_runtime_session: HashMap::new(),
            startup_launch_plans_by_runtime_session: HashMap::new(),
            startup_completion_intents_by_runtime_session: HashMap::new(),
            startup_readiness_signal_preparations_by_runtime_session: HashMap::new(),
        }
    }
}


impl AgentsTerminalStartupCoordinator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_visible_mounting_startup_candidates(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
        startup_body_geometries: &HashMap<
            AgentsTerminalStartupBodySlotId,
            AgentsTerminalStartupBodyGeometry,
        >,
    ) {
        let current_candidates = if agents_workspace_visible {
            workspace.visible_selected_mounting_startup_candidates()
        } else {
            Vec::new()
        };
        let current_startup_body_slot_ids = current_candidates
            .iter()
            .map(|candidate| candidate.startup_body_slot_id())
            .collect::<HashSet<_>>();

        self.pending_startups_by_runtime_session
            .retain(|runtime_session_id, record| {
                agents_workspace_visible
                    && current_startup_body_slot_ids.contains(&record.startup_body_slot_id())
                    && workspace
                        .session(record.shell_session_id)
                        .is_some_and(|session| {
                            session.presentation_state == TerminalSessionPresentationState::Mounting
                                && runtime_sessions
                                    .runtime_session_id_for_shell_session(record.shell_session_id)
                                    == Some(*runtime_session_id)
                        })
            });
        let pending_runtime_session_ids = self
            .pending_startups_by_runtime_session
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        self.startup_launch_plans_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });
        self.startup_completion_intents_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });
        self.startup_readiness_signal_preparations_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });

        if !agents_workspace_visible {
            return;
        }

        for mut candidate in current_candidates {
            let runtime_session_id =
                runtime_sessions.ensure_runtime_session_id(candidate.shell_session_id);
            candidate.startup_body_geometry_available =
                startup_body_geometries.contains_key(&candidate.startup_body_slot_id());
            self.pending_startups_by_runtime_session
                .insert(runtime_session_id, candidate);
        }
        self.sync_startup_completion_intents(agents_workspace_visible, workspace, runtime_sessions);
    }

    pub(crate) fn sync_startup_launch_plans(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        startup_body_geometries: &HashMap<
            AgentsTerminalStartupBodySlotId,
            AgentsTerminalStartupBodyGeometry,
        >,
    ) {
        self.startup_launch_plans_by_runtime_session = derive_agents_terminal_startup_launch_plans(
            agents_workspace_visible,
            workspace,
            runtime_sessions,
            startup_body_geometries,
            &self.pending_startups_by_runtime_session,
        );
        self.sync_startup_completion_intents(agents_workspace_visible, workspace, runtime_sessions);
    }

    pub(crate) fn sync_startup_completion_intents(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) {
        self.startup_completion_intents_by_runtime_session =
            derive_agents_terminal_startup_completion_intents(
                agents_workspace_visible,
                workspace,
                runtime_sessions,
                &self.pending_startups_by_runtime_session,
            );
        self.startup_readiness_signal_preparations_by_runtime_session
            .retain(|runtime_session_id, preparation| {
                self.startup_completion_intents_by_runtime_session
                    .get(runtime_session_id)
                    .copied()
                    == Some(preparation.completion_intent)
            });
    }

    pub(crate) fn sync_startup_readiness_signal_preparations(
        &mut self,
        metadata_snapshots: impl IntoIterator<
            Item = (
                AgentsTerminalRuntimeSessionId,
                AgentsTerminalStartupBodySlotId,
                terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
            ),
        >,
    ) {
        self.startup_readiness_signal_preparations_by_runtime_session = metadata_snapshots
            .into_iter()
            .filter_map(
                |(runtime_session_id, startup_body_slot_id, surface_metadata)| {
                    self.prepare_startup_readiness_signal(
                        runtime_session_id,
                        startup_body_slot_id,
                        surface_metadata,
                    )
                    .map(|preparation| (runtime_session_id, preparation))
                },
            )
            .collect();
    }

    pub(crate) fn prepare_startup_readiness_signal(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<AgentsTerminalStartupReadinessSignalPreparation> {
        let completion_intent = self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;

        (completion_intent.runtime_session_id == runtime_session_id)
            .then_some(())
            .and_then(|_| {
                AgentsTerminalStartupReadinessSignalPreparation::new(
                    completion_intent,
                    startup_body_slot_id,
                    surface_metadata,
                )
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_readiness_handoff_plans(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) -> Vec<AgentsTerminalStartupReadinessHandoffPlan> {
        let mut plans = self
            .startup_readiness_signal_preparations_by_runtime_session
            .keys()
            .copied()
            .filter_map(|runtime_session_id| {
                self.startup_readiness_handoff_plan_for_runtime_session(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    runtime_session_id,
                )
            })
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            (
                plan.startup_body_slot_id().pane_id.0,
                plan.startup_body_slot_id().session_id.0,
                plan.runtime_session_id().0,
            )
        });
        plans
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_readiness_handoff_plan_for_runtime_session(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<AgentsTerminalStartupReadinessHandoffPlan> {
        /*
        CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
        A ready metadata snapshot may promote only the exact current Mounting body it was prepared for. Match runtime id, shell session id, startup body slot id, visible selected Mounting state, current launch plan, and the future Running mount slot before any owner map can move.
        */
        if !agents_workspace_visible {
            return None;
        }

        let preparation = self
            .startup_readiness_signal_preparations_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let completion_intent = preparation.completion_intent;
        if completion_intent.runtime_session_id != runtime_session_id
            || !preparation.surface_metadata.indicates_ready_metadata()
            || self
                .startup_completion_intents_by_runtime_session
                .get(&runtime_session_id)
                .copied()
                != Some(completion_intent)
        {
            return None;
        }

        let record = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let startup_launch_plan = self
            .startup_launch_plans_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let startup_body_slot_id = record.startup_body_slot_id();
        if record.shell_session_id != completion_intent.shell_session_id
            || startup_body_slot_id != completion_intent.startup_body_slot_id
            || startup_launch_plan.runtime_session_id != runtime_session_id
            || startup_launch_plan.shell_session_id != record.shell_session_id
            || startup_launch_plan.pane_id != record.pane_id
            || startup_launch_plan.startup_body_slot_id != startup_body_slot_id
            || runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            || !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id)
            || !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
        {
            return None;
        }

        let mount_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id: startup_body_slot_id.pane_id,
            session_id: completion_intent.shell_session_id,
        };
        (mount_slot_id.session_id == startup_body_slot_id.session_id
            && mount_slot_id.pane_id == startup_launch_plan.pane_id)
            .then_some(AgentsTerminalStartupReadinessHandoffPlan {
                completion_intent,
                startup_launch_plan,
                mount_slot_id,
            })
    }

    #[allow(dead_code)]
    pub(crate) fn produce_startup_result_from_runtime_signal(
        &self,
        signal: Option<AgentsTerminalStartupCompletionSignal>,
    ) -> Option<AgentsTerminalStartupResult> {
        let signal = signal?;
        let completion_intent = signal.completion_intent();
        self.startup_completion_intents_by_runtime_session
            .get(&completion_intent.runtime_session_id)
            .copied()
            .is_some_and(|current_intent| current_intent == completion_intent)
            .then_some(signal.into_startup_result())
    }

    pub(crate) fn produce_failed_startup_result_from_surface_metadata(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<AgentsTerminalStartupResult> {
        if !agents_workspace_visible || !surface_metadata.process_exited() {
            return None;
        }

        let completion_intent = self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        if completion_intent.runtime_session_id != runtime_session_id
            || completion_intent.startup_body_slot_id != startup_body_slot_id
        {
            return None;
        }

        let record = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        if record.shell_session_id != completion_intent.shell_session_id
            || record.startup_body_slot_id() != startup_body_slot_id
            || runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            || !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id)
            || !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
        {
            return None;
        }

        Some(AgentsTerminalStartupResult::Failed { completion_intent })
    }

    pub(crate) fn apply_startup_result(
        &mut self,
        workspace: &mut WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        result: AgentsTerminalStartupResult,
    ) -> bool {
        let runtime_session_id = result.runtime_session_id();
        let completion_intent = result.completion_intent();
        let Some(record) = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()
        else {
            return false;
        };

        if self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()
            != Some(completion_intent)
            || record.shell_session_id != completion_intent.shell_session_id
            || record.startup_body_slot_id() != completion_intent.startup_body_slot_id
        {
            return false;
        }

        if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
            != Some(runtime_session_id)
        {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
            return false;
        }

        if !workspace.is_current_terminal_startup_body_slot(completion_intent.startup_body_slot_id)
        {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
            return false;
        }

        let changed = workspace.transition_terminal_session_presentation_state(
            record.shell_session_id,
            TerminalSessionPresentationState::Mounting,
            result.terminal_presentation_state(),
        );
        if changed || !workspace.session_is_mounting(record.shell_session_id) {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
        }
        changed
    }

    // Consumed by the macOS hidden-host startup path and by the non-macOS
    // GPUI-engine startup path, so it stays ungated.
    pub(crate) fn startup_launch_plans(&self) -> Vec<AgentsTerminalStartupLaunchPlan> {
        let mut plans = self
            .startup_launch_plans_by_runtime_session
            .values()
            .copied()
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            (
                plan.startup_body_slot_id.pane_id.0,
                plan.startup_body_slot_id.session_id.0,
                plan.runtime_session_id.0,
            )
        });
        plans
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_host_preservation_keys(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) -> Vec<AgentsTerminalStartupHostPreservationKey> {
        let mut keys = derive_agents_terminal_startup_host_preservation_keys(
            agents_workspace_visible,
            workspace,
            runtime_sessions,
            &self.pending_startups_by_runtime_session,
        );
        keys.sort_by_key(|key| {
            (
                key.startup_body_slot_id.pane_id.0,
                key.startup_body_slot_id.session_id.0,
                key.runtime_session_id.0,
            )
        });
        keys
    }
}


pub(crate) fn derive_agents_terminal_startup_launch_plans(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    startup_body_geometries: &HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan> {
    if !agents_workspace_visible {
        return HashMap::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            let geometry = startup_body_geometries
                .get(&startup_body_slot_id)
                .copied()?;
            Some((
                runtime_session_id,
                AgentsTerminalStartupLaunchPlan {
                    runtime_session_id,
                    shell_session_id: record.shell_session_id,
                    pane_id: record.pane_id,
                    startup_body_slot_id,
                    bounds: geometry.bounds,
                    scale_factor: geometry.scale_factor,
                },
            ))
        })
        .collect()
}


pub(crate) fn derive_agents_terminal_startup_host_preservation_keys(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> Vec<AgentsTerminalStartupHostPreservationKey> {
    if !agents_workspace_visible {
        return Vec::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            Some(AgentsTerminalStartupHostPreservationKey {
                runtime_session_id,
                startup_body_slot_id,
            })
        })
        .collect()
}


pub(crate) fn derive_agents_terminal_startup_completion_intents(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent> {
    if !agents_workspace_visible {
        return HashMap::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            Some((
                runtime_session_id,
                AgentsTerminalStartupCompletionIntent::from_record(runtime_session_id, record),
            ))
        })
        .collect()
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WorkspacePaneId(pub(crate) u64);


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceSplitId(pub(crate) u64);


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceSplitAxis {
    Horizontal,
    Vertical,
}


impl WorkspaceSplitAxis {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceDropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceDropTarget {
    TabStrip(usize),
    PaneBody(WorkspaceDropZone),
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceDropFeedback {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) target: WorkspaceDropTarget,
}


#[derive(Clone, Copy)]
pub(crate) struct WorkspaceCloseFocusBounds {
    pub(crate) left: f32,
    pub(crate) right: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}


pub(crate) struct WorkspaceCloseFocusRect {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) bounds: WorkspaceCloseFocusBounds,
    pub(crate) center_x: f32,
    pub(crate) center_y: f32,
    pub(crate) path: Vec<usize>,
    pub(crate) has_tabs: bool,
}


#[derive(Clone)]
pub(crate) struct DraggedWorkspaceTab {
    pub(crate) source_pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
    pub(crate) title: String,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) tab_status: AgentTerminalTabStatus,
    pub(crate) agent_icon: Option<&'static str>,
}


pub(crate) struct WorkspaceTabDragPreview {
    pub(crate) title: String,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) tab_status: AgentTerminalTabStatus,
    pub(crate) agent_icon: Option<&'static str>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandSessionId(pub(crate) u64);


pub(crate) fn gpui_command_session_external_id(session_id: CommandSessionId) -> String {
    /*
    CDXC:GPUICommandPaneBridge 2026-06-27-07:05:
    SidebarApp and shared app-modal command-session payloads use canonical `G{u64}` bridge ids so GPUI matches native local session id shape. Keep `CommandSessionId` numeric inside the Rust model and shell/layout persistence, and do not emit legacy numeric strings across the external command-pane bridge.
    */
    format!("G{}", session_id.0)
}


pub(crate) fn gpui_command_session_id_from_external_id(value: &str) -> Option<CommandSessionId> {
    /*
    CDXC:GPUICommandPaneBridge 2026-06-27-07:05:
    External command-pane ids are accepted only as uppercase `G` plus a positive decimal integer. Reject raw numeric strings, lowercase prefixes, empty ids, malformed suffixes, and zero instead of falling back to legacy numeric parsing.
    */
    let numeric = value.strip_prefix('G')?;
    if numeric.is_empty() || !numeric.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let id = numeric.parse::<u64>().ok()?;
    (id > 0).then_some(CommandSessionId(id))
}


/// Agents workspace sessions cross the app-modal bridge as `GW{u64}` ids so
/// Delayed Send payloads cannot collide with `G{u64}` command-pane ids or
/// gxserver session ids.
pub(crate) fn gpui_agents_session_external_id(session_id: TerminalSessionId) -> String {
    format!("GW{}", session_id.0)
}


pub(crate) fn gpui_agents_session_id_from_external_id(value: &str) -> Option<TerminalSessionId> {
    let numeric = value.strip_prefix("GW")?;
    if numeric.is_empty() || !numeric.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let id = numeric.parse::<u64>().ok()?;
    (id > 0).then_some(TerminalSessionId(id))
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandPaneGroupId(pub(crate) u64);


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneHoverTab {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceHoverTab {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserHoverTab {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) tab_id: BrowserTabId,
}


/*
CDXC:GPUICommandPaneResize 2026-06-25-13:19:
Resize hover affordance is runtime-only chrome owned by the exact rail under the pointer: the command-panel rail or one command split rail. Do not persist it into command-pane layout state or infer it from drag state.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneResizeHoverTarget {
    PanelRail,
    Split(CommandPaneSplitId),
}


pub(crate) fn clear_command_resize_hover_state_fields(
    hovering: &mut Option<CommandPaneResizeHoverTarget>,
    visible: &mut Option<CommandPaneResizeHoverTarget>,
    epoch: &mut u64,
) -> bool {
    /*
    CDXC:GPUICommandPaneResize 2026-06-27-03:12:
    Native command-panel resize cursors are refreshed when the rail gesture ends, resets, or disappears. GPUI mirrors that by explicitly clearing runtime resize hover chrome and invalidating delayed hover timers whenever command resize ownership ends, without persisting or mutating layout state.
    */
    if hovering.is_none() && visible.is_none() {
        return false;
    }

    *hovering = None;
    *visible = None;
    *epoch = epoch.wrapping_add(1);
    true
}


pub(crate) fn clear_command_resize_hover_state_fields_if_command_pane_hidden(
    command_pane_has_sessions: bool,
    hovering: &mut Option<CommandPaneResizeHoverTarget>,
    visible: &mut Option<CommandPaneResizeHoverTarget>,
    epoch: &mut u64,
) -> bool {
    /*
    CDXC:GPUICommandPaneResize 2026-06-27-03:16:
    Final command-tab removal hides the command panel just like explicit minimize/collapse. Clear runtime resize hover chrome only after the command pane becomes empty so ordinary tab close, scoped close, confirmed close, and process-exit cleanup keep hover affordances while the panel remains visible.

    CDXC:GPUICommandPaneResize 2026-06-27-07:23:
    User/direct command-tab closes, sidebar Action clears, and scoped tab closes can remove the final command session without going through the explicit minimize control. They must invalidate resize-hover cursor chrome at the successful model-removal boundary, while non-final closes preserve hover because the panel still has a live rail.
    */
    if command_pane_has_sessions {
        return false;
    }

    clear_command_resize_hover_state_fields(hovering, visible, epoch)
}


/*
CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
The command sticky active-tab proxy is runtime-only tab-strip navigation chrome. It appears at the edge where the selected command tab is clipped and never enters command-pane persistence or tab identity state.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneStickyActiveTabEdge {
    Leading,
    Trailing,
}


impl CommandPaneStickyActiveTabEdge {
    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            CommandPaneStickyActiveTabEdge::Leading => "leading",
            CommandPaneStickyActiveTabEdge::Trailing => "trailing",
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandPaneSplitId(pub(crate) u64);


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BrowserTabId(pub(crate) u64);


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BrowserPaneId(pub(crate) u64);


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BrowserSplitId(pub(crate) u64);


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaneMode {
    Pinned,
    Floating,
    Collapsed,
}


impl CommandPaneMode {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "pinned" => Some(Self::Pinned),
            "floating" => Some(Self::Floating),
            "collapsed" => Some(Self::Collapsed),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::Floating => "floating",
            Self::Collapsed => "collapsed",
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneNewCommandControlPlacement {
    FixedActionCluster,
    InlineTabRun,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneBottomReservationChrome {
    PlainChrome,
    CollapsedStrip,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandPaneWorkspaceBottomReservation {
    pub(crate) chrome: CommandPaneBottomReservationChrome,
    pub(crate) height: f32,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CommandPaneWorkspaceLayoutPlan {
    Hidden,
    Pinned {
        panel_height: f32,
    },
    PinnedRight {
        panel_width: f32,
    },
    Floating {
        panel_height: f32,
        bottom_reservation: CommandPaneWorkspaceBottomReservation,
    },
    Collapsed {
        bottom_reservation: CommandPaneWorkspaceBottomReservation,
    },
}


pub(crate) fn agents_workspace_tab_context_close_scope_label(
    scope: AgentsWorkspaceTabCloseScope,
) -> &'static str {
    match scope {
        AgentsWorkspaceTabCloseScope::Close => "Close Tab",
        AgentsWorkspaceTabCloseScope::CloseLeft => "Close Left",
        AgentsWorkspaceTabCloseScope::CloseOthers => "Close Other Tabs",
        AgentsWorkspaceTabCloseScope::CloseRight => "Close Right",
    }
}


pub(crate) fn agents_workspace_tab_context_sleep_scope_label(
    scope: AgentsWorkspaceTabSleepScope,
) -> &'static str {
    match scope {
        AgentsWorkspaceTabSleepScope::Sleep => "Sleep",
        AgentsWorkspaceTabSleepScope::SleepLeft => "Sleep Left",
        AgentsWorkspaceTabSleepScope::SleepOthers => "Sleep Other Tabs",
        AgentsWorkspaceTabSleepScope::SleepRight => "Sleep Right",
    }
}


pub(crate) fn agents_workspace_tab_context_focus_label() -> &'static str {
    "Focus"
}


pub(crate) fn agents_workspace_tab_context_scoped_close_order() -> [AgentsWorkspaceTabCloseScope; 3] {
    /*
    CDXC:GPUIAgentsTabContextMenu 2026-06-26-06:57:
    Native workspace tab right-click menus omit direct Close Tab and order scoped close rows as Close Right, Close Left, then Close Other Tabs. GPUI Agents menus use the same row set while direct close remains owned by inline tab chrome and middle-click gestures.
    */
    [
        AgentsWorkspaceTabCloseScope::CloseRight,
        AgentsWorkspaceTabCloseScope::CloseLeft,
        AgentsWorkspaceTabCloseScope::CloseOthers,
    ]
}


pub(crate) fn agents_workspace_tab_context_sleep_order(
    clicked_tab_is_sleeping: bool,
) -> Vec<AgentsWorkspaceTabSleepScope> {
    /*
    CDXC:GPUIAgentsTabContextMenu 2026-06-26-06:57:
    Native workspace tab menus show direct Sleep only for awake clicked tabs, then always show Sleep Right, Sleep Left, and Sleep Other Tabs before the close group. Empty sibling scopes remain action rows and no-op in the pane-local resolver.
    */
    let mut scopes = Vec::with_capacity(4);
    if !clicked_tab_is_sleeping {
        scopes.push(AgentsWorkspaceTabSleepScope::Sleep);
    }
    scopes.extend([
        AgentsWorkspaceTabSleepScope::SleepRight,
        AgentsWorkspaceTabSleepScope::SleepLeft,
        AgentsWorkspaceTabSleepScope::SleepOthers,
    ]);
    scopes
}


pub(crate) fn command_pane_tab_context_close_scope_label(scope: CommandPaneTabCloseScope) -> &'static str {
    match scope {
        CommandPaneTabCloseScope::Close => "Close Tab",
        CommandPaneTabCloseScope::CloseLeft => "Close Left",
        CommandPaneTabCloseScope::CloseOthers => "Close Other Tabs",
        CommandPaneTabCloseScope::CloseRight => "Close Right",
    }
}


pub(crate) fn command_pane_tab_context_sleep_scope_label(scope: CommandPaneTabSleepScope) -> &'static str {
    match scope {
        CommandPaneTabSleepScope::Sleep => "Sleep",
        CommandPaneTabSleepScope::SleepLeft => "Sleep Left",
        CommandPaneTabSleepScope::SleepOthers => "Sleep Other Tabs",
        CommandPaneTabSleepScope::SleepRight => "Sleep Right",
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneTabSessionAction {
    Rename,
    DelayedSend,
    CloseAfterDone,
}


pub(crate) fn command_pane_tab_context_focus_label() -> &'static str {
    "Focus"
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneTabContextFocusPolicy {
    SelectAndFocus,
    SelectExpandWakeAndFocus,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneScopedTabMutationFocusPolicy {
    FocusCommandPane,
    PreserveCurrentFocus,
}


pub(crate) fn command_pane_tab_context_session_action_focus_policy(
    action: CommandPaneTabSessionAction,
) -> CommandPaneTabContextFocusPolicy {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-18:33:
    Retained clicked-tab action handlers focus the clicked terminal before dispatch. GPUI mirrors native dispatch for Rename and Close After Done by selecting/focusing the clicked command tab without expanding hidden chrome, while Delayed Send still expands and wakes because it must target a visible mounted command body for the later Return.

    CDXC:GPUICommandTabContextMenu 2026-06-27-01:49:
    Keep these dispatch policies for existing non-menu action handlers, but do not use them as evidence that command-tab right-click should expose Rename Session, Delayed Send, or Close After Done rows.
    */
    match action {
        CommandPaneTabSessionAction::Rename | CommandPaneTabSessionAction::CloseAfterDone => {
            CommandPaneTabContextFocusPolicy::SelectAndFocus
        }
        CommandPaneTabSessionAction::DelayedSend => {
            CommandPaneTabContextFocusPolicy::SelectExpandWakeAndFocus
        }
    }
}


pub(crate) fn command_pane_tab_context_scoped_lifecycle_focus_policy()
-> CommandPaneScopedTabMutationFocusPolicy {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-18:38:
    Native scoped Sleep/Close context-menu rows do not focus the clicked terminal before dispatch; only primary tab actions route through `nativeTabContextMenuAction`. Preserve GPUI shell focus for scoped lifecycle rows while direct/focused command close and focused Sleep keep their existing focus ownership.
    */
    CommandPaneScopedTabMutationFocusPolicy::PreserveCurrentFocus
}


pub(crate) fn command_pane_tab_context_runtime_action_count(
    _command_pane: &CommandPaneModel,
    _group_id: CommandPaneGroupId,
    _session_id: CommandSessionId,
) -> usize {
    /*
    CDXC:GPUICommandTabRuntimeActions 2026-06-25-21:59:
    Fork Session, Reload Session, and Pop Out Pane must stay absent from GPUI command-tab context menus until they can dispatch to real command-pane runtime semantics. Current command Ghostty surfaces support mount, focus, input, close/confirm, sleep parking, and action timers only; there is no command-session clone, live embedded reload, or popped-out command-owner transfer path. Do not add disabled rows, fallback toasts, shell-only duplicates, surface drops, or placeholder menu actions.

    CDXC:GPUICommandTabRuntimeActions 2026-06-28-15:12:
    GPUI tests are intentionally absent, so preserve this as the production row-count policy instead of retaining unused runtime-action enums or test-gated assertion helpers.
    */
    0
}


pub(crate) fn command_pane_tab_tooltip(title: &str, delayed_send_remaining_label: Option<&str>) -> String {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-17:57:
    Native command tabs keep the normal title tooltip but append "Delayed Send in <remaining>" while a live timer label exists. Use only the visible title plus the runtime countdown label already allowed for tab/body/sidebar timer chrome; do not read command text, terminal content, shell-state JSON, paths, stdout/stderr, or persisted placeholder flags.
    */
    let title = if title.trim().is_empty() {
        COMMAND_PANE_DEFAULT_SESSION_TITLE
    } else {
        title
    };
    delayed_send_remaining_label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| format!("{title}\nDelayed Send in {label}"))
        .unwrap_or_else(|| title.to_string())
}


pub(crate) fn command_pane_tab_context_scoped_close_order() -> [CommandPaneTabCloseScope; 3] {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-14:07:
    Native tab-button context menus order scoped close rows as Close Right, Close Left, then Close Other Tabs. Command-role GPUI menus should match those labels and order while preserving the existing clicked-group close resolution.
    */
    [
        CommandPaneTabCloseScope::CloseRight,
        CommandPaneTabCloseScope::CloseLeft,
        CommandPaneTabCloseScope::CloseOthers,
    ]
}


pub(crate) fn command_pane_tab_context_sleep_order(
    clicked_tab_is_sleeping: bool,
) -> Vec<CommandPaneTabSleepScope> {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-14:27:
    Native tab menus show direct Sleep only for awake clicked tabs, then always show Sleep Right, Sleep Left, and Sleep Other Tabs before the close group. Keep the row order separate from close ordering so sleeping command tabs remain visible while not offering a redundant direct Sleep action.
    */
    let mut scopes = Vec::with_capacity(4);
    if !clicked_tab_is_sleeping {
        scopes.push(CommandPaneTabSleepScope::Sleep);
    }
    scopes.extend([
        CommandPaneTabSleepScope::SleepRight,
        CommandPaneTabSleepScope::SleepLeft,
        CommandPaneTabSleepScope::SleepOthers,
    ]);
    scopes
}


pub(crate) fn command_pane_new_command_control_placement() -> CommandPaneNewCommandControlPlacement {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:13:
    macOS command-pane chrome keeps New Terminal inline with the tab run, while the fixed right action cluster is reserved for panel actions such as Pin/Unpin and Minimize/Expand. GPUI should not render New Terminal in that fixed cluster.
    */
    CommandPaneNewCommandControlPlacement::InlineTabRun
}


pub(crate) fn command_pane_tab_add_tooltip() -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:23:
    Native command chrome names the inline plus action New Terminal, even inside the command panel. Keep GPUI's visible tooltip aligned with the macOS tab-add button while the internal model still creates a command-terminal placeholder.
    */
    "New Terminal"
}


pub(crate) fn command_pane_tab_add_icon_path() -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:54:
    Native command-pane New Terminal chrome is the tab-strip add button, not the generic `.newTerminal` titlebar action button. It uses plus symbol chrome with the New Terminal tooltip, so GPUI should render a plus icon rather than the terminal action symbol here.
    */
    COMMAND_ICON_PLUS
}


pub(crate) fn command_pane_panel_mode_controls_visible(expanded_chrome: bool) -> bool {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:05:
    Hidden/collapsed command-panel chrome mirrors macOS expand-only panel actions, so Pin/Unpin is visible only in expanded command-pane titlebars. Keep New Terminal as inline tab-run chrome and Expand in the collapsed strip for existing GPUI collapsed-strip creation/open behavior, but do not expose panel mode mutation while hidden.
    */
    COMMAND_PANE_FLOATING_MODE_ENABLED && expanded_chrome
}


pub(crate) fn command_pane_mode_for_current_release(mode: CommandPaneMode) -> CommandPaneMode {
    if COMMAND_PANE_FLOATING_MODE_ENABLED || mode != CommandPaneMode::Floating {
        mode
    } else {
        CommandPaneMode::Pinned
    }
}


pub(crate) fn command_pane_panel_pin_icon_path(mode: CommandPaneMode) -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:40:
    macOS command-panel mode chrome uses pin and pin.slash symbols for Pin/Unpin Commands Panel. GPUI should not expose raw P/U fallback letters when SVG chrome is available.
    */
    match mode {
        CommandPaneMode::Pinned => COMMAND_ICON_PIN_SLASH,
        CommandPaneMode::Floating | CommandPaneMode::Collapsed => COMMAND_ICON_PIN,
    }
}


pub(crate) fn command_pane_panel_visibility_icon_path(expanded: bool) -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:40:
    macOS command-panel visibility chrome uses chevron.down for Minimize Commands Panel and chevron.up for Expand Commands Panel. Keep GPUI on symbol chrome instead of visible v/^ fallback text.
    */
    if expanded {
        COMMAND_ICON_CHEVRON_DOWN
    } else {
        COMMAND_ICON_CHEVRON_UP
    }
}


pub(crate) fn command_pane_bottom_reservation_chrome(
    mode: CommandPaneMode,
) -> Option<CommandPaneBottomReservationChrome> {
    /*
    CDXC:GPUICommandPaneFloating 2026-06-25-18:19:
    Native floating command panels reserve `collapsedCommandsPanelHeight` as a plain black `CommandsPanelChromeView` while the expanded floating panel owns the actual tabs and controls. Render the interactive collapsed strip only for collapsed command-pane mode so floating mode does not duplicate command tabs below the panel.
    */
    match mode {
        CommandPaneMode::Pinned => None,
        CommandPaneMode::Floating => Some(CommandPaneBottomReservationChrome::PlainChrome),
        CommandPaneMode::Collapsed => Some(CommandPaneBottomReservationChrome::CollapsedStrip),
    }
}


pub(crate) fn command_pane_workspace_layout_plan(
    mode: CommandPaneMode,
    has_sessions: bool,
    content_height: f32,
    height_ratio: f32,
    side: GpuiCommandPaneSide,
    content_width: f32,
    width_ratio: f32,
) -> CommandPaneWorkspaceLayoutPlan {
    /*
    CDXC:GPUICommandPaneLayout 2026-06-27-08:32:
    Native TerminalWorkspaceView lays command panels out from session presence and mode: pinned reserves the full command-panel height and pushes the workspace up, floating expanded overlays the panel while reserving only collapsedCommandsPanelHeight as plain bottom chrome, and collapsed renders only the interactive collapsed strip.

    CDXC:GPUICommandPaneLayout 2026-06-27-15:00:
    The command-pane chrome belongs to live command sessions. Once the final command session closes, hide the entire bottom strip so the active workspace reclaims its full height; reopening a command pane still follows the existing session-creation path.

    CDXC:GPUICommandPaneSide 2026-08-16:
    `commandsPanelSide: "right"` only changes the pinned placement: the expanded pane becomes a workspace column sized by `widthRatio`. Floating (release-disabled) and the collapsed footer strip keep their bottom layout so the pane is discoverable from the same place on both sides.
    */
    if !has_sessions {
        return CommandPaneWorkspaceLayoutPlan::Hidden;
    }

    if side == GpuiCommandPaneSide::Right && mode == CommandPaneMode::Pinned {
        return CommandPaneWorkspaceLayoutPlan::PinnedRight {
            panel_width: command_pane_width_for_ratio(width_ratio, content_width),
        };
    }

    let bottom_reservation = command_pane_bottom_reservation_chrome(mode).map(|chrome| {
        CommandPaneWorkspaceBottomReservation {
            chrome,
            height: COMMAND_PANE_STRIP_HEIGHT,
        }
    });

    match mode {
        CommandPaneMode::Pinned => CommandPaneWorkspaceLayoutPlan::Pinned {
            panel_height: command_pane_height_for_ratio(height_ratio, content_height),
        },
        CommandPaneMode::Floating => CommandPaneWorkspaceLayoutPlan::Floating {
            panel_height: command_pane_floating_height_for_ratio(height_ratio, content_height),
            bottom_reservation: bottom_reservation
                .expect("floating command panes reserve plain bottom chrome"),
        },
        CommandPaneMode::Collapsed => CommandPaneWorkspaceLayoutPlan::Collapsed {
            bottom_reservation: bottom_reservation
                .expect("collapsed command panes reserve collapsed bottom chrome"),
        },
    }
}


pub(crate) fn command_pane_control_trailing_padding(expanded_chrome: bool) -> f32 {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:47:
    Expanded and collapsed command action clusters keep their rightmost button flush inside the surrounding command chrome. The collapsed strip supplies its separate outer right margin.
    */
    if expanded_chrome {
        COMMAND_PANE_CONTROL_EXPANDED_TRAILING_PADDING
    } else {
        COMMAND_PANE_CONTROL_COLLAPSED_TRAILING_PADDING
    }
}


pub(crate) fn command_pane_tab_left_mouse_up_selects(
    pending_click: Option<CommandPanePendingTabClick>,
    target: CommandPanePendingTabClick,
    command_tab_drag_active: bool,
) -> bool {
    /*
    CDXC:GPUICommandTabSelection 2026-06-25-19:14:
    Left-click command tab activation is a mouse-up commit after a matching mouse-down token. Once GPUI starts a command-tab drag, the pending token is canceled and mouse-up must not mutate the active command selection.
    */
    pending_click == Some(target) && !command_tab_drag_active
}


pub(crate) fn command_pane_tab_left_mouse_up_focuses(
    click_count: usize,
    pending_click: Option<CommandPanePendingTabClick>,
    target: CommandPanePendingTabClick,
    command_tab_drag_active: bool,
    command_pane: &CommandPaneModel,
) -> bool {
    /*
    CDXC:GPUICommandFocusMode 2026-06-25-21:50:
    Native command-pane tabs route double-click mouse-up to Focus instead of the normal selection request, but only for split owners that expose the Focus tab action. Keep the same pending-click and drag gates as single-click selection so double-click Focus cannot fire after drag start, stale mouse-up delivery, collapsed hidden-open tabs, or non-eligible command groups.
    */
    click_count >= 2
        && command_pane_tab_left_mouse_up_selects(pending_click, target, command_tab_drag_active)
        && command_pane.tab_context_allows_focus_mode(target.group_id, target.session_id)
}


pub(crate) fn command_pane_tab_left_mouse_up_finishes_drag(command_tab_drag_active: bool) -> bool {
    /*
    CDXC:GPUICommandTabSelection 2026-06-25-19:21:
    A left mouse-up delivered to a command tab still has to clear any active command-tab drag state after skipping click selection. Native AppKit's tab mouse-up ends the drag/click gesture through the same owner; GPUI must not rely only on the root mouse-up path because the tab handler consumes the event.
    */
    command_tab_drag_active
}

