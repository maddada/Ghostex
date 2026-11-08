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
    BrowserHistory,
    RecentProjects,
    StashedPrompts,
    /// Quick Access on its Files tab.
    QuickAccessFiles,
    AgentsHub,
    DelayedSend,
    RenameSession,
    SessionNote,
    ExportTranscriptResult,
    /// The message a session dropped on another project opens. Opened directly by the sidebar's
    /// drop, never by a bridge message.
    SessionHandoff,
    ConfigureAgents,
    ConfigureActions,
    OpenTargets,
    /// CDXC:Onboarding 2026-09-27 SEE-ALSO:
    /// `onboarding` is the five-panel modal that the automatic first run, Tips > Setup and Quick Access > Setup open (user decision, see modals/modal_window.rs `open_gpui_first_launch_setup_with_sidebar_state`). The older first-launch setup modal was deleted on 2026-09-27 (user: "delete old setup one not new one that's active").
    /// The guards that treat Onboarding as "setup in progress" (no-projects close guard, completion on close, `completeFirstLaunchSetup`) have a React twin, `isFirstLaunchSetupModalKind` in apps/desktop/views/modal-host/settings-routing.ts (deleted 2026-10-01), and the component contract is packages/core-ui/onboarding/contract.ts (deleted 2026-10-01).
    Onboarding,
    RemoteGxserverInstall,
    RemoteSetup,
    Worktree,
    DeleteWorktree,
    RenameWorktree,
    SidebarSpaceEditor,
    MermaidDiagram,
    MarkdownTable,
    /// An agent's published HTML page, floating over the chat (visual_page_modal_lifecycle.rs).
    VisualPage,
    Extension(ExtensionId),
    UpdateAvailable,
    /// Name, agent, goal and first request of a new coordinator (new_coordinator_modal_lifecycle.rs).
    NewCoordinator,
    /// Name, instructions and agent of a new GBot (new_gbot_modal_lifecycle.rs).
    NewGbot,
    /// Make an existing session a coordinator, with an optional goal (new_coordinator_modal_lifecycle.rs).
    MakeCoordinator,
    /// An orchestrator's Goal & Instructions or Memory editor (coordinator_details_modal_lifecycle.rs).
    CoordinatorDetails,
    /// Send Bug / Feedback, from the More options menu (feedback_modal_lifecycle.rs).
    Feedback,
    /// Create a Linear ticket and start work on it, from a work-mode project's menu
    /// (create_linear_ticket_modal_lifecycle.rs).
    CreateLinearTicket,
    /// Link a work-mode session to a PR, Linear issues, a Linear project or a GitHub issue, from
    /// the session's Link to submenu (work_link_picker_modal_lifecycle.rs).
    WorkLinkPicker,
    /// Link a PR, an issue or a Linear ticket to a session by hand, from its links list's +
    /// (add_session_link_modal_lifecycle.rs).
    AddSessionLink,
    /// "Paste potentially unsafe text?" for terminal paste protection (terminal_input/paste_and_shortcuts.rs).
    TerminalPasteConfirm,
    CloseSessionsConfirm,
    /// The one-time 11.0.1 security notice: pair phones and computers again
    /// (security_upgrade_modal_lifecycle.rs). Opened directly when gxserver says it is due.
    SecurityUpgrade,
}

impl GpuiAppModalKind {
    pub(crate) fn from_modal_id(value: &str) -> Option<Self> {
        match value {
            "addProject" => Some(Self::AddProject),
            "agentHooksRequired" => Some(Self::AgentHooksRequired),
            "securityUpgrade" => Some(Self::SecurityUpgrade),
            "settings" => Some(Self::Settings),
            "hotkeys" => Some(Self::Hotkeys),
            "missingProjectFolder" => Some(Self::MissingProjectFolder),
            "commandPalette" => Some(Self::CommandPalette),
            "findPrompts" => Some(Self::FindPrompts),
            "previousSessions" => Some(Self::PreviousSessions),
            "browserHistory" => Some(Self::BrowserHistory),
            "recentProjects" => Some(Self::RecentProjects),
            "stashedPrompts" => Some(Self::StashedPrompts),
            "quickAccessFiles" => Some(Self::QuickAccessFiles),
            "agentsHub" => Some(Self::AgentsHub),
            "delayedSend" => Some(Self::DelayedSend),
            "renameSession" => Some(Self::RenameSession),
            "sessionNote" => Some(Self::SessionNote),
            "exportTranscriptResult" => Some(Self::ExportTranscriptResult),
            "sessionHandoff" => Some(Self::SessionHandoff),
            "configureAgents" => Some(Self::ConfigureAgents),
            "configureActions" => Some(Self::ConfigureActions),
            "openTargets" => Some(Self::OpenTargets),
            "onboarding" => Some(Self::Onboarding),
            "remoteGxserverInstall" => Some(Self::RemoteGxserverInstall),
            "remoteSetup" => Some(Self::RemoteSetup),
            "worktree" => Some(Self::Worktree),
            "deleteWorktree" => Some(Self::DeleteWorktree),
            "renameWorktree" => Some(Self::RenameWorktree),
            "sidebarSpaceEditor" => Some(Self::SidebarSpaceEditor),
            "mermaidDiagram" => Some(Self::MermaidDiagram),
            "markdownTable" => Some(Self::MarkdownTable),
            "visualPage" => Some(Self::VisualPage),
            value if value.starts_with("extension:") => {
                ExtensionId::new(value.trim_start_matches("extension:")).map(Self::Extension)
            }
            "updateAvailable" => Some(Self::UpdateAvailable),
            "newCoordinator" => Some(Self::NewCoordinator),
            "newGbot" => Some(Self::NewGbot),
            "makeCoordinator" => Some(Self::MakeCoordinator),
            "coordinatorDetails" => Some(Self::CoordinatorDetails),
            "feedback" => Some(Self::Feedback),
            "createLinearTicket" => Some(Self::CreateLinearTicket),
            "workLinkPicker" => Some(Self::WorkLinkPicker),
            "addSessionLink" => Some(Self::AddSessionLink),
            "terminalPasteConfirm" => Some(Self::TerminalPasteConfirm),
            "closeSessionsConfirm" => Some(Self::CloseSessionsConfirm),
            _ => None,
        }
    }

    pub(crate) fn modal_id(self) -> &'static str {
        match self {
            Self::AddProject => "addProject",
            Self::AgentHooksRequired => "agentHooksRequired",
            Self::SecurityUpgrade => "securityUpgrade",
            Self::Settings => "settings",
            Self::Hotkeys => "hotkeys",
            Self::MissingProjectFolder => "missingProjectFolder",
            Self::CommandPalette => "commandPalette",
            Self::FindPrompts => "findPrompts",
            Self::PreviousSessions => "previousSessions",
            Self::BrowserHistory => "browserHistory",
            Self::RecentProjects => "recentProjects",
            Self::StashedPrompts => "stashedPrompts",
            Self::QuickAccessFiles => "quickAccessFiles",
            Self::AgentsHub => "agentsHub",
            Self::DelayedSend => "delayedSend",
            Self::RenameSession => "renameSession",
            Self::SessionNote => "sessionNote",
            Self::ExportTranscriptResult => "exportTranscriptResult",
            Self::SessionHandoff => "sessionHandoff",
            Self::ConfigureAgents => "configureAgents",
            Self::ConfigureActions => "configureActions",
            Self::OpenTargets => "openTargets",
            Self::Onboarding => "onboarding",
            Self::RemoteGxserverInstall => "remoteGxserverInstall",
            Self::RemoteSetup => "remoteSetup",
            Self::Worktree => "worktree",
            Self::DeleteWorktree => "deleteWorktree",
            Self::RenameWorktree => "renameWorktree",
            Self::SidebarSpaceEditor => "sidebarSpaceEditor",
            Self::MermaidDiagram => "mermaidDiagram",
            Self::MarkdownTable => "markdownTable",
            Self::VisualPage => "visualPage",
            Self::Extension(id) => extension_modal_id(id),
            Self::UpdateAvailable => "updateAvailable",
            Self::NewCoordinator => "newCoordinator",
            Self::NewGbot => "newGbot",
            Self::MakeCoordinator => "makeCoordinator",
            Self::CoordinatorDetails => "coordinatorDetails",
            Self::Feedback => "feedback",
            Self::CreateLinearTicket => "createLinearTicket",
            Self::WorkLinkPicker => "workLinkPicker",
            Self::AddSessionLink => "addSessionLink",
            Self::TerminalPasteConfirm => "terminalPasteConfirm",
            Self::CloseSessionsConfirm => "closeSessionsConfirm",
        }
    }

    pub(crate) fn window_title(self) -> &'static str {
        match self {
            Self::BrowserHistory => "Ghostex Browser History",
            Self::AddProject => "Ghostex Add Project",
            Self::AgentHooksRequired => "Ghostex Install Required Hooks",
            Self::SecurityUpgrade => "Ghostex Pair Your Devices Again",
            Self::Settings => "Ghostex Settings",
            Self::Hotkeys => "Ghostex Hotkeys",
            Self::MissingProjectFolder => "Ghostex Project Folder Missing",
            Self::CommandPalette
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts
            | Self::QuickAccessFiles => "Ghostex Quick Access",
            Self::FindPrompts => "Ghostex Search by Prompt",
            Self::AgentsHub => "Ghostex Agents Hub",
            Self::DelayedSend => "Ghostex Session Automations",
            Self::RenameSession => "Ghostex Rename Session",
            Self::SessionNote => "Ghostex Session Note",
            Self::ExportTranscriptResult => "Ghostex Handoff / Export",
            Self::SessionHandoff => "Ghostex Handoff",
            Self::ConfigureAgents => "Ghostex Configure Agents",
            Self::ConfigureActions => "Ghostex Actions",
            Self::OpenTargets => "Ghostex Open Targets",
            Self::Onboarding => "Welcome to Ghostex",
            Self::RemoteGxserverInstall => "Ghostex Remote Setup",
            Self::RemoteSetup => "Ghostex Mobile & Remote",
            Self::Worktree => "Ghostex Add Worktree",
            Self::DeleteWorktree => "Ghostex Delete Worktree",
            Self::RenameWorktree => "Ghostex Rename Worktree",
            Self::SidebarSpaceEditor => "Ghostex Space",
            Self::MermaidDiagram | Self::MarkdownTable | Self::VisualPage => "",
            Self::Extension(_) => "Ghostex Extension",
            Self::UpdateAvailable => "Ghostex Update",
            Self::NewCoordinator => "Ghostex New Coordinator",
            Self::NewGbot => "Ghostex New Bot",
            Self::MakeCoordinator => "Ghostex Make Coordinator",
            Self::CoordinatorDetails => "Ghostex Coordinator",
            Self::Feedback => "Ghostex Send Bug / Feedback",
            Self::CreateLinearTicket => "Ghostex Create Linear Ticket",
            Self::WorkLinkPicker => "Ghostex Link Session",
            Self::AddSessionLink => "Ghostex Add a Link",
            Self::TerminalPasteConfirm => "Ghostex Paste",
            Self::CloseSessionsConfirm => "Ghostex Close",
        }
    }

    pub(crate) fn window_size(self) -> Size<Pixels> {
        let base = self.base_window_size();
        let (width, height) = self.large_panel_size(f32::from(base.width), f32::from(base.height));
        size(px(width), px(height))
    }

    /// The large panel modals that open centred over the app: the Settings dialog (every entry),
    /// Search by Prompt, the Agents Hub, Commit Changes and File Diff. Quick Access, Browser
    /// History, the compact forms and confirmations, onboarding and the chat's diagram and table
    /// popups are not in this class.
    pub(crate) fn is_large_panel(self) -> bool {
        matches!(
            self,
            Self::Settings
                | Self::Hotkeys
                | Self::ConfigureAgents
                | Self::ConfigureActions
                | Self::OpenTargets
                | Self::FindPrompts
                | Self::AgentsHub
        )
    }

    /// The Settings dialog in every entry (Hotkeys, Configure Agents, Configure Actions and Open
    /// Targets render it too); the only large panels that open 20% bigger than the others.
    pub(crate) fn is_settings_dialog(self) -> bool {
        matches!(
            self,
            Self::Settings
                | Self::Hotkeys
                | Self::ConfigureAgents
                | Self::ConfigureActions
                | Self::OpenTargets
        )
    }

    /// CDXC:AppModal 2026-10-01 DECISION:
    /// User: Settings and Search by Prompt "open way too big. they should open centered in the page like the size that the quick access modal opens, not biggger", then "please make the settings and find by prompt modals to be bigger by 30% width and height" (2026-09-30), then "the bigger modals that pop up in the centre of the screen, like Find by Prompt and Settings: the size needs to be 10% bigger." Settings and Search by Prompt keep the Quick Access frame 30% wider and taller as their base (900 x 663), and every large panel modal opens 10% wider and taller than its base frame (Search by Prompt 990 x 729); the opener still shrinks it to fit the screen it opens on. This supersedes the 2026-09-30 opening size.
    ///
    /// CDXC:Settings 2026-10-14 DECISION:
    /// User: "please make settings for ghostex bigger by 20%". The Settings dialog (every entry that renders it, `is_settings_dialog`) opens another 20% wider and taller than the large panel size: 1188 x 875 instead of 990 x 729. Only the window grows, not the fonts or controls, and the other large panels keep the 10% step. This supersedes the 990 x 729 Settings size above.
    pub(crate) fn large_panel_size(self, width: f32, height: f32) -> (f32, f32) {
        if self.is_settings_dialog() {
            return crate::app::window::settings_dialog_modal_size(width, height);
        }
        if !self.is_large_panel() {
            return (width, height);
        }
        crate::app::window::large_panel_modal_size(width, height)
    }

    fn base_window_size(self) -> Size<Pixels> {
        match self {
            Self::BrowserHistory => size(
                px(APP_MODAL_HOST_COMMAND_PALETTE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_BROWSER_HISTORY_WINDOW_HEIGHT),
            ),
            /* All five Quick Access tabs share one stable child-window frame. */
            Self::CommandPalette
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts
            | Self::QuickAccessFiles => size(
                px(APP_MODAL_HOST_COMMAND_PALETTE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_PREVIOUS_SESSIONS_WINDOW_HEIGHT),
            ),
            Self::DelayedSend => size(
                px(APP_MODAL_HOST_DELAYED_SEND_WINDOW_WIDTH),
                px(APP_MODAL_HOST_DELAYED_SEND_WINDOW_HEIGHT),
            ),
            /*
            CDXC:SessionNotes 2026-08-24:
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
            Self::SessionHandoff => size(
                px(crate::app::window::SESSION_HANDOFF_MODAL_WIDTH),
                px(crate::app::window::SESSION_HANDOFF_MODAL_INITIAL_HEIGHT),
            ),
            Self::MissingProjectFolder => size(
                px(APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_WIDTH),
                px(APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_HEIGHT),
            ),
            Self::AgentHooksRequired => size(
                px(APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH),
                px(APP_MODAL_HOST_AGENT_HOOKS_REQUIRED_WINDOW_HEIGHT),
            ),
            Self::SecurityUpgrade => size(
                px(crate::app::window::SECURITY_UPGRADE_MODAL_WIDTH),
                px(crate::app::window::SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT),
            ),
            /*
            CDXC:AppModal 2026-07-26-07:20:
            Settings, Hotkeys, Configure Agents, Configure Actions, and Open Targets all render the one tabbed Settings dialog in the modal host, so they must keep the full Settings frame even though their legacy standalone stylesheets are narrower.
            */
            // The Settings and Search by Prompt base frame; they open 10% bigger, Settings another 20% (CDXC:AppModal 2026-10-01 and CDXC:Settings 2026-10-14 on `large_panel_size`).
            Self::Settings
            | Self::Hotkeys
            | Self::FindPrompts
            | Self::ConfigureAgents
            | Self::ConfigureActions
            | Self::OpenTargets => size(
                px(APP_MODAL_HOST_SETTINGS_WINDOW_WIDTH),
                px(APP_MODAL_HOST_SETTINGS_WINDOW_HEIGHT),
            ),
            Self::AgentsHub => size(
                px(APP_MODAL_HOST_WINDOW_WIDTH),
                px(APP_MODAL_HOST_WINDOW_HEIGHT),
            ),
            Self::AddProject => size(
                px(APP_MODAL_HOST_ADD_PROJECT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_ADD_PROJECT_WINDOW_HEIGHT),
            ),
            Self::Worktree => size(
                px(APP_MODAL_HOST_WORKTREE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_WORKTREE_WINDOW_HEIGHT),
            ),
            Self::DeleteWorktree => size(
                px(APP_MODAL_HOST_COMPACT_WINDOW_WIDTH),
                px(APP_MODAL_HOST_DELETE_WORKTREE_WINDOW_HEIGHT),
            ),
            /*
            CDXC:Worktrees 2026-08-09-18:40:
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
            CDXC:Spaces 2026-09-13 DECISION:
            User: make the New Space / Edit Space modal narrower, with every color swatch still on one line.
            This supersedes the 2026-08-28 compact (760px) width. Height stays 380px GPUI content; native titlebar and frame are outside these bounds.
            */
            Self::SidebarSpaceEditor => size(
                px(APP_MODAL_HOST_SIDEBAR_SPACE_EDITOR_WINDOW_WIDTH),
                px(APP_MODAL_HOST_SIDEBAR_SPACE_EDITOR_WINDOW_HEIGHT),
            ),
            Self::UpdateAvailable => size(
                px(APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_WIDTH),
                px(APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_HEIGHT),
            ),
            Self::NewCoordinator => size(
                px(crate::app::window::NEW_COORDINATOR_MODAL_WIDTH),
                px(crate::app::window::NEW_COORDINATOR_MODAL_INITIAL_HEIGHT),
            ),
            Self::MakeCoordinator => size(
                px(crate::app::window::NEW_COORDINATOR_MODAL_WIDTH),
                px(crate::app::window::MAKE_COORDINATOR_MODAL_INITIAL_HEIGHT),
            ),
            Self::CoordinatorDetails => size(
                px(crate::app::window::COORDINATOR_DETAILS_MODAL_WIDTH),
                px(crate::app::window::COORDINATOR_DETAILS_MODAL_INITIAL_HEIGHT),
            ),
            Self::NewGbot => size(
                px(crate::app::window::NEW_GBOT_MODAL_WIDTH),
                px(crate::app::window::NEW_GBOT_MODAL_INITIAL_HEIGHT),
            ),
            Self::Feedback => size(
                px(crate::app::window::FEEDBACK_MODAL_WIDTH),
                px(crate::app::window::FEEDBACK_MODAL_INITIAL_HEIGHT),
            ),
            Self::CreateLinearTicket => size(
                px(crate::app::window::CREATE_LINEAR_TICKET_MODAL_WIDTH),
                px(crate::app::window::CREATE_LINEAR_TICKET_MODAL_INITIAL_HEIGHT),
            ),
            Self::WorkLinkPicker => size(
                px(crate::app::window::WORK_LINK_PICKER_MODAL_WIDTH),
                px(crate::app::window::WORK_LINK_PICKER_MODAL_INITIAL_HEIGHT),
            ),
            Self::AddSessionLink => size(
                px(crate::app::window::ADD_SESSION_LINK_MODAL_WIDTH),
                px(crate::app::window::ADD_SESSION_LINK_MODAL_INITIAL_HEIGHT),
            ),
            Self::Extension(id) => extension_modal_window_size(id),
            // `TERMINAL_PASTE_CONFIRM_MODAL_WIDTH` / `_INITIAL_HEIGHT` in window/terminal_paste_confirm_modal.rs.
            Self::TerminalPasteConfirm => size(px(440.0), px(180.0)),
            Self::CloseSessionsConfirm => size(
                px(crate::app::window::CLOSE_SESSIONS_CONFIRM_MODAL_WIDTH),
                px(crate::app::window::CLOSE_SESSIONS_CONFIRM_MODAL_INITIAL_HEIGHT),
            ),
            Self::RemoteGxserverInstall => size(
                px(APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_WIDTH),
                px(APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_HEIGHT),
            ),
            Self::RemoteSetup => size(
                px(APP_MODAL_HOST_REMOTE_SETUP_WINDOW_WIDTH),
                px(APP_MODAL_HOST_REMOTE_SETUP_WINDOW_HEIGHT),
            ),
            // CDXC:Onboarding 2026-09-11 WHY:
            // The new onboarding renders a 1672x941 stage scaled to fit its window, so the frame keeps that aspect ratio at a size that still fits a 1440x900 screen with the menu bar and Dock.
            Self::Onboarding => size(px(1400.0), px(788.0)),
            // The floating previews open at a share of the app window instead
            // (`chat_preview_modal_size`, CDXC:SessionChat 2026-10-22, which supersedes the
            // 2026-09-06 1248 x 912 diagram size); these stay for the generic modal paths.
            Self::MermaidDiagram => size(px(1248.0), px(912.0)),
            Self::MarkdownTable => size(px(1248.0), px(912.0)),
            Self::VisualPage => size(px(1120.0), px(820.0)),
        }
    }

    pub(crate) fn window_size_for_open(self, _message: &serde_json::Value) -> Size<Pixels> {
        self.window_size()
    }

    /// CDXC:AppModal 2026-09-08 DECISION:
    /// User: use the terminal quick picker's borderless style for all app modals, including Settings, Quick Access, Add Project, Search by Prompt, Rename Session, and Delayed Send.
    /// This extends the earlier table and Mermaid popup decision. Clicking outside no longer closes a modal; the hover-only corner close button of `window/modal_window_frame.rs` is the close control instead (CDXC:AppModal 2026-09-30).
    pub(crate) fn has_titlebar(self) -> bool {
        false
    }

    pub(crate) fn is_resizable(self) -> bool {
        /*
        CDXC:AppModal 2026-06-27-09:57:
        Native command-pane Rename Session and Delayed Send are fixed-size child windows. GPUI must not apply a generic resizable minimum to these compact dialogs because Delayed Send is intentionally 470x365.

        CDXC:AppModal 2026-07-26-07:20:
        Every app-modal window is now fitted to its own dialog, so none of them are resizable. The React dialogs own their internal scrolling, and a resizable frame only ever produced dead space around a fixed-height form or a stretched compact dialog.

        CDXC:Extensions 2026-08-30:
        The Extensions browser was the last resizable app modal; it is a
        Settings tab now, so this is unconditionally false again.
        */
        false
    }

    pub(crate) fn window_min_size(self, open_message: &serde_json::Value) -> Size<Pixels> {
        self.window_size_for_open(open_message)
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
                | Self::Onboarding
                | Self::AgentsHub
                | Self::DelayedSend
                | Self::RenameSession
                // CDXC:Spaces 2026-09-14 DECISION:
                // User: the New Space / Edit Space modal must support light mode the same way Rename Session does.
                // The child window only learns the workspace theme from the sidebar hydrate, so this dialog has to receive that snapshot or it stays on the default dark palette.
                | Self::SidebarSpaceEditor
                // The export result dialog's agent picker renders the user's
                // configured agents, which only reach the modal host through
                // the sidebar-state snapshot.
                | Self::ExportTranscriptResult
                | Self::Worktree
                | Self::DeleteWorktree
                | Self::RenameWorktree
        )
    }

    pub(crate) fn open_message(self) -> serde_json::Value {
        match self {
            Self::CommandPalette => serde_json::json!({
                "initialQuery": "",
                "modal": self.modal_id(),
                "type": "open",
            }),
            Self::BrowserHistory
            | Self::Settings
            | Self::Hotkeys
            | Self::FindPrompts
            | Self::ConfigureAgents
            | Self::ConfigureActions
            | Self::OpenTargets
            | Self::PreviousSessions
            | Self::RecentProjects
            | Self::StashedPrompts
            | Self::QuickAccessFiles
            | Self::AgentsHub
            | Self::DelayedSend
            | Self::RenameSession
            | Self::SessionNote
            | Self::RemoteSetup
            | Self::Feedback
            | Self::NewGbot
            | Self::TerminalPasteConfirm
            | Self::CloseSessionsConfirm
            | Self::Onboarding => serde_json::json!({
                "modal": self.modal_id(),
                "type": "open",
            }),
            Self::MermaidDiagram | Self::MarkdownTable => serde_json::json!({
                "modal": self.modal_id(), "source": "", "type": "open",
            }),
            Self::VisualPage => serde_json::json!({
                "modal": self.modal_id(), "url": "", "type": "open",
            }),
            // A New Coordinator open names its project and a Make Coordinator open its session,
            // which only the sidebar knows; an orchestrator's editor opens with what gxserver read.
            Self::Extension(_)
            | Self::NewCoordinator
            | Self::MakeCoordinator
            | Self::CoordinatorDetails
            | Self::CreateLinearTicket
            | Self::WorkLinkPicker
            | Self::AddSessionLink => serde_json::Value::Null,
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
            // These modals are normally opened through bridge messages that
            // carry their full payload (worktree and diff drafts); the bare
            // open message is the menu-path shape. For the Space editor it is
            // also the template the bridge's field allowlist fills in.
            Self::Worktree
            | Self::AgentHooksRequired
            | Self::SecurityUpgrade
            | Self::DeleteWorktree
            | Self::RenameWorktree
            | Self::SidebarSpaceEditor
            | Self::ExportTranscriptResult
            | Self::SessionHandoff
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
    CDXC:CommandPalette 2026-06-26-23:04:
    `runGhostexHotkeyAction` needs an explicit app-modal allowlist after shell, pane, sidebar, focus, and action-slot routes have run. Map the separate Quick Access command/session entry actions and legacy sidebar modal ids here without treating every unknown hotkey id as a modal candidate.
    */
    match action_id {
        "openSettings" => Some(GpuiAppModalKind::Settings),
        "openHotkeys" => Some(GpuiAppModalKind::Hotkeys),
        "openCommandPalette" => Some(GpuiAppModalKind::CommandPalette),
        "openFindPrompts" => Some(GpuiAppModalKind::FindPrompts),
        "openSessionSearchPalette" => Some(GpuiAppModalKind::PreviousSessions),
        "openProjectSearchPalette" => Some(GpuiAppModalKind::RecentProjects),
        "openQuickAccessFiles" => Some(GpuiAppModalKind::QuickAccessFiles),
        "openPreviousSessions" => Some(GpuiAppModalKind::PreviousSessions),
        "agentsHub" | "openAgentsHub" => Some(GpuiAppModalKind::AgentsHub),
        "configureAgents" => Some(GpuiAppModalKind::ConfigureAgents),
        "actions" | "configureActions" => Some(GpuiAppModalKind::ConfigureActions),
        "openTargets" => Some(GpuiAppModalKind::OpenTargets),
        /*
        CDXC:Extensions 2026-08-30:
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
