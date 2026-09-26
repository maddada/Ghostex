// C1 wave-3 extraction: the gpui Action-derive structs, the actions! macro block, and their scope enums moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

gpui::actions!(
    ghostex_gpui,
    [
        OpenCommandPane,
        PasteIntoFocusedTerminal,
        FindInFocusedTerminal,
        OpenBrowserHistory,
        ReloadFocusedBrowser,
        FindNextInFocusedBrowser,
        FindPreviousInFocusedBrowser,
        ZoomInFocusedSurface,
        ZoomOutFocusedSurface,
        ResetFocusedSurfaceZoom,
        TitlebarDropdownCancel,
        SleepInactiveSessionsFromTitlebar,
        StartGpuiGxserverFromTitlebar,
        StopGpuiGxserverFromTitlebar,
        RestartGpuiGxserverFromTitlebar,
        OpenGpuiPortlessSetupModalFromTitlebar,
        CloseFocusedSurface,
        CloseFocusedSurfaceMenuOnly,
        ToggleGpuiSidebarCollapsed,
        ToggleViewPanel,
        SleepFocusedSession,
        WakeFocusedSession,
        NewTerminalTab,
        SplitFocusedTerminalRight,
        SplitFocusedTerminalDown,
        NewBrowserTab,
        ToggleAgentsFocusMode,
        MergeAllTabs,
        SwitchAgentsWorkarea,
        SwitchSourceWorkarea,
        SwitchBrowserWorkarea,
        SwitchKanbanWorkarea,
        SwitchManageWorkarea,
        OpenGpuiSettingsModal,
        OpenGpuiExtensionsModal,
        OpenGpuiAccountsModal,
        OpenGpuiHotkeysModal,
        OpenGpuiCommandPaletteModal,
        OpenGpuiPreviousSessionsModal,
        OpenGpuiAgentsHubModal,
        OpenGpuiConfigureAgentsModal,
        OpenGpuiConfigureActionsModal,
        OpenGpuiOpenTargetsModal,
        ConfigureGpuiTitlebarActions,
        StopGpuiKeepAwake,
        OpenGpuiPowerSettingsModal,
        SleepGpuiPetOverlay,
        GoToGhostexFromGpuiPetOverlay,
        GpuiKeepAwakeMenuLabel,
        FocusWorkspaceLeft,
        FocusWorkspaceRight,
        FocusWorkspaceUp,
        FocusWorkspaceDown,
        AboutGhostexGpui,
        CheckForGhostexGpuiUpdates,
        HideGhostexGpui,
        HideGhostexGpuiOthers,
        ShowAllGhostexGpuiApps,
        RestartGhostexGpui,
        QuitGhostexGpui,
        QuitGhostexGpuiAndBackgroundServices,
        MinimizeGhostexGpuiWindow,
        ZoomGhostexGpuiWindow,
        GpuiEditMenuCut,
        GpuiEditMenuCopy,
        GpuiEditMenuPaste,
        GpuiEditMenuSelectAll
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NewTerminalTabInPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SplitPaneRightWithNewTerminal {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SplitPaneBelowWithNewTerminal {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RotateAgentsPanesForPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct AppendFullWidthTerminalRowForPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleFocusModeForPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct MergeAllTabsForPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CloseAgentsPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SelectAgentsWorkspaceTab {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CloseAgentsWorkspaceTab {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CloseAgentsWorkspaceTabsByScope {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
    pub(crate) scope: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SleepAgentsWorkspaceTabsByScope {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
    pub(crate) scope: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RenameAgentsWorkspaceTab {
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct FocusAgentsWorkspaceTab {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ForkAgentsWorkspaceTab {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ReloadAgentsWorkspaceTab {
    pub(crate) pane_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentsWorkspaceTabCloseScope {
    Close,
    CloseLeft,
    CloseOthers,
    CloseRight,
}

impl AgentsWorkspaceTabCloseScope {
    pub(crate) fn action_value(self) -> u8 {
        match self {
            Self::Close => 0,
            Self::CloseLeft => 1,
            Self::CloseOthers => 2,
            Self::CloseRight => 3,
        }
    }

    pub(crate) fn from_action_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Close),
            1 => Some(Self::CloseLeft),
            2 => Some(Self::CloseOthers),
            3 => Some(Self::CloseRight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentsWorkspaceTabSleepScope {
    Sleep,
    SleepLeft,
    SleepOthers,
    SleepRight,
}

impl AgentsWorkspaceTabSleepScope {
    pub(crate) fn action_value(self) -> u8 {
        match self {
            Self::Sleep => 0,
            Self::SleepLeft => 1,
            Self::SleepOthers => 2,
            Self::SleepRight => 3,
        }
    }

    pub(crate) fn from_action_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Sleep),
            1 => Some(Self::SleepLeft),
            2 => Some(Self::SleepOthers),
            3 => Some(Self::SleepRight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenBrowserPaneInExternalBrowser {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SetBrowserPageAppearance {
    pub(crate) appearance: crate::cef::BrowserPageAppearance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NewBrowserTabInPane {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SplitBrowserPaneRightWithBrowserTab {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SplitBrowserPaneBelowWithBrowserTab {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SelectBrowserTabInPane {
    pub(crate) pane_id: u64,
    pub(crate) tab_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CloseBrowserTabInPane {
    pub(crate) pane_id: u64,
    pub(crate) tab_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SleepBrowserTabInPane {
    pub(crate) pane_id: u64,
    pub(crate) tab_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunBrowserFeedbackTool;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleBrowserDevTools;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ResetBrowserZoom;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenGpuiWorkspaceInTarget {
    pub(crate) target_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SelectGpuiTitlebarMode {
    pub(crate) mode_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ReloadGpuiTitlebarView {
    pub(crate) mode_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SleepGpuiTitlebarView {
    pub(crate) mode_index: u64,
}

/// The view panel's tab strip. Every row identifies its view by `TitlebarMode::switcher_index`, the
/// same id the older view actions use, so a menu row and a hotkey name the same view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenGpuiViewTab {
    pub(crate) mode_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenNewBrowserTabFromViewMenu;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CloseGpuiViewTab {
    pub(crate) mode_index: u64,
}

/// Pin or unpin a tab of the view panel's strip. `browser_tab_id` names one of the Browser's pages;
/// without it the row is the view `mode_index` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleGpuiViewStripTabPinned {
    pub(crate) mode_index: u64,
    pub(crate) browser_tab_id: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct PopOutGpuiViewTab {
    pub(crate) mode_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleGpuiViewPanelMaximized;

/// "Show in this Project" and "Show in this Space": one override each, ticked when the view is
/// currently shown there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleGpuiViewProjectScope {
    pub(crate) mode_index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleGpuiViewSpaceScope {
    pub(crate) mode_index: u64,
    pub(crate) space_key: String,
}

/// `Hidden here ▸`: clear the override that hides this view in this project, so it comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ShowGpuiHiddenViewHere {
    pub(crate) mode_index: u64,
}

/// "Choose where it's shown…": Settings, on the Extensions page, with this view's scope editor open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenGpuiViewScopeSettings {
    pub(crate) mode_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunGpuiTitlebarAction {
    pub(crate) action_index: u64,
}

/// One row of the titlebar's ⋯ menu, by its position in `GpuiTitlebarMoreMenuItem::ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenGpuiTitlebarMoreMenuItem {
    pub(crate) item_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunGpuiTitlebarGitMenuAction {
    pub(crate) row_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CopyGpuiTitlebarGitBranch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenGpuiTitlebarGitCommitScreen;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunGpuiTitlebarGitRemoteSync;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunGpuiTitlebarTipsHeaderAction {
    pub(crate) action_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunGpuiTitlebarTip {
    pub(crate) tip_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunGpuiTitlebarHelpQuestion {
    pub(crate) question_index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct FocusGpuiTitlebarResourceSession {
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct OpenGpuiTitlebarResourceUrl {
    pub(crate) url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct StartGpuiKeepAwakePeriod {
    pub(crate) duration_minutes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SelectBrowserProfile {
    pub(crate) pane_id: u64,
    pub(crate) profile_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CreateBrowserProfile {
    pub(crate) pane_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct CloseCommandPaneTabsByScope {
    pub(crate) group_id: u64,
    pub(crate) session_id: u64,
    pub(crate) scope: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct SleepCommandPaneTabsByScope {
    pub(crate) group_id: u64,
    pub(crate) session_id: u64,
    pub(crate) scope: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RenameCommandPaneTab {
    pub(crate) group_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct DelayedSendCommandPaneTab {
    pub(crate) group_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleCloseAfterDoneCommandPaneTab {
    pub(crate) group_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct FocusCommandPaneTab {
    pub(crate) group_id: u64,
    pub(crate) session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneTabCloseScope {
    Close,
    CloseLeft,
    CloseOthers,
    CloseRight,
}

impl CommandPaneTabCloseScope {
    pub(crate) fn action_value(self) -> u8 {
        match self {
            Self::Close => 0,
            Self::CloseLeft => 1,
            Self::CloseOthers => 2,
            Self::CloseRight => 3,
        }
    }

    pub(crate) fn from_action_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Close),
            1 => Some(Self::CloseLeft),
            2 => Some(Self::CloseOthers),
            3 => Some(Self::CloseRight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneTabSleepScope {
    Sleep,
    SleepLeft,
    SleepOthers,
    SleepRight,
}

impl CommandPaneTabSleepScope {
    pub(crate) fn action_value(self) -> u8 {
        match self {
            Self::Sleep => 0,
            Self::SleepLeft => 1,
            Self::SleepOthers => 2,
            Self::SleepRight => 3,
        }
    }

    pub(crate) fn from_action_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Sleep),
            1 => Some(Self::SleepLeft),
            2 => Some(Self::SleepOthers),
            3 => Some(Self::SleepRight),
            _ => None,
        }
    }
}
