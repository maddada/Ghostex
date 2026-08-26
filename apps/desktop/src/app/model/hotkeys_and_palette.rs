// C1 wave-3 re-cluster: focused-pane and command-palette hotkey action resolution, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn focused_command_pane_create_split_hotkey_source(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUIFocusedCommandHotkeys 2026-06-26-06:47:
    Cmd+T/Cmd+D command-placeholder creation is a native responder path only while the Commands panel is visibly expanded. Require expanded command-pane focus plus a live focused source session before allocating command tabs or splits, so stale/collapsed command focus no-ops instead of creating hidden command sessions; clicked command-panel creation keeps its explicit hidden-open route.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some((group_id, session_id))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandPaneFocusedSessionHotkeyAction {
    Rename,
    DelayedSend,
    CloseAfterDone,
    Sleep,
    Wake,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiFocusedPaneHotkeyAction {
    CreateSession,
    OpenCommandsPanel,
    OpenBrowserPane,
    SplitRight,
    SplitDown,
    MergeAllTabs,
    RotatePanesClockwise,
    RuntimeNoOp(GpuiFocusedPaneRuntimeAction),
    CommandSession(GpuiCommandPaneFocusedSessionHotkeyAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiFocusedPaneRuntimeAction {
    ForkSession,
    ReloadSession,
    PopOutPane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandPaletteTabCycleHotkeyAction {
    Previous,
    Next,
}

impl GpuiCommandPaletteTabCycleHotkeyAction {
    pub(crate) fn reverse(self) -> bool {
        matches!(self, Self::Previous)
    }
}

pub(crate) fn gpui_command_palette_tab_cycle_hotkey_action(
    action_id: &str,
) -> Option<GpuiCommandPaletteTabCycleHotkeyAction> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-07:32:
    Shared command-palette tab-cycle rows post `focusPreviousSession` and `focusNextSession` through `runGhostexHotkeyAction`. GPUI maps only those exact ids to the `cycle_focused_tab(reverse)` direction so command, Agents, and Browser focus keep Ctrl-Shift-Tab/Ctrl-Tab parity; numbered session-slot rows are intentionally excluded because SidebarApp owns rendered slot order.
    */
    match action_id {
        "focusPreviousSession" => Some(GpuiCommandPaletteTabCycleHotkeyAction::Previous),
        "focusNextSession" => Some(GpuiCommandPaletteTabCycleHotkeyAction::Next),
        _ => None,
    }
}

pub(crate) fn gpui_command_palette_sidebar_slot_hotkey_action_id(action_id: &str) -> Option<&str> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-23:20:
    Numbered `focusSessionSlot1` through `focusSessionSlot9` rows are rendered-sidebar slot commands, not Rust tab-cycle commands. Delegate only those exact action ids to SidebarApp so its DOM slot ownership resolves focus, while `focusPreviousSession`/`focusNextSession` stay on GPUI tab-cycle routing and jump-to-project ids cannot loop back through native.
    */
    match action_id {
        "focusSessionSlot1" | "focusSessionSlot2" | "focusSessionSlot3" | "focusSessionSlot4"
        | "focusSessionSlot5" | "focusSessionSlot6" | "focusSessionSlot7" | "focusSessionSlot8"
        | "focusSessionSlot9" => Some(action_id),
        _ => None,
    }
}

pub(crate) fn gpui_command_palette_project_slot_hotkey_number(action_id: &str) -> Option<u8> {
    /*
    CDXC:GPUIProjectHotkeys 2026-06-26-23:42:
    Project slot hotkeys are rendered-sidebar project commands. GPUI must delegate `jumpToProject1` through `jumpToProject9` to SidebarApp because Rust does not own the rendered project row order and must avoid the `nativeHotkey` bounce path that SidebarApp forwards back to native.
    */
    match action_id {
        "jumpToProject1" => Some(1),
        "jumpToProject2" => Some(2),
        "jumpToProject3" => Some(3),
        "jumpToProject4" => Some(4),
        "jumpToProject5" => Some(5),
        "jumpToProject6" => Some(6),
        "jumpToProject7" => Some(7),
        "jumpToProject8" => Some(8),
        "jumpToProject9" => Some(9),
        _ => None,
    }
}

pub(crate) fn gpui_command_pane_focused_session_hotkey_action(
    action_id: &str,
) -> Option<GpuiCommandPaneFocusedSessionHotkeyAction> {
    /*
    CDXC:GPUICommandFocusedSessionActions 2026-06-25-15:01:
    GPUI handles command-pane focused Close After Done, Sleep, Wake, and Close action ids from the shared command-palette hotkey bridge. Other hotkey ids must continue to use their existing modal or shell handlers instead of being swallowed by command-pane lifecycle code.

    Delayed Send is a real focused-session action in GPUI. Route it through the
    command-session branch so the command palette and configured hotkey open
    the existing timer modal for the exact focused command terminal.
    */
    match action_id {
        "renameActiveSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Rename),
        "delayedSend" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::DelayedSend),
        "closeAfterDone" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::CloseAfterDone),
        "sleepFocusedSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Sleep),
        "wakeFocusedSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Wake),
        "closeFocusedSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Close),
        _ => None,
    }
}

pub(crate) fn gpui_focused_pane_hotkey_action(
    action_id: &str,
) -> Option<GpuiFocusedPaneHotkeyAction> {
    /*
    CDXC:GPUICommandPalette 2026-06-25-17:32:
    The shared command palette posts focused-pane commands through `runGhostexHotkeyAction`. GPUI must route supported pane actions through the same shell helpers as direct keybindings so command-pane focus can split commands, non-command focus can open Browser, and Agents-only merge can no-op exactly like the native focused-pane dispatcher without fabricating fork/reload/pop-out runtime behavior.

    CDXC:GPUICommandPalette 2026-06-27-05:30:
    Native focused-pane Fork, Reload, and Pop Out actions still enter the focused-pane dispatcher, then command terminals consume them through the command-panel titlebar branch's default no-op because command sessions do not own those runtime semantics. GPUI should consume those ids explicitly as runtime no-ops so they cannot fall through to modal/sidebar fallback routes, while still not inventing fake command clone, reload, or pop-out behavior.

    CDXC:GPUICommandPalette 2026-06-26-06:47:
    `openBrowserPane` remains a recognized focused-pane command-palette id, but command-terminal focus decides at execution time whether it no-ops through the native command-panel titlebar branch instead of creating a Browser tab.

    CDXC:GPUIFocusedPaneRotation 2026-06-26-06:56:
    `rotatePanesClockwise` must enter the GPUI focused-pane bridge instead of falling through to modal routing. Command-pane, Browser, and project-editor focus no-op by policy; active Agents-pane focus is the only admitted execution target once the WorkspaceModel pure rotation helper is available.

    CDXC:GPUICommandPalette 2026-06-26-07:15:
    `openCommandsPanel` is the shared command-palette, sidebar-header, and F12
    route for the Commands panel. Hidden panels open and focus; a visible pane
    that is not already focused becomes active; the same hotkey minimizes an
    already-focused pane.

    CDXC:GPUICommandPalette 2026-06-26-07:24:
    `createSession` from the command palette must reuse the focused Cmd+T hotkey helper. That preserves command-pane visible-source gating and Agents-pane placeholder targeting instead of adding a separate fallback that could create a session in the wrong surface.
    */
    match action_id {
        "createSession" => Some(GpuiFocusedPaneHotkeyAction::CreateSession),
        "openCommandsPanel" => Some(GpuiFocusedPaneHotkeyAction::OpenCommandsPanel),
        "openBrowserPane" => Some(GpuiFocusedPaneHotkeyAction::OpenBrowserPane),
        "splitMore" => Some(GpuiFocusedPaneHotkeyAction::SplitRight),
        "splitMoreDown" => Some(GpuiFocusedPaneHotkeyAction::SplitDown),
        "mergeAllTabs" => Some(GpuiFocusedPaneHotkeyAction::MergeAllTabs),
        "rotatePanesClockwise" => Some(GpuiFocusedPaneHotkeyAction::RotatePanesClockwise),
        "forkSession" => Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(
            GpuiFocusedPaneRuntimeAction::ForkSession,
        )),
        "reloadSession" => Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(
            GpuiFocusedPaneRuntimeAction::ReloadSession,
        )),
        "popOutPane" => Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(
            GpuiFocusedPaneRuntimeAction::PopOutPane,
        )),
        _ => gpui_command_pane_focused_session_hotkey_action(action_id)
            .map(GpuiFocusedPaneHotkeyAction::CommandSession),
    }
}

pub(crate) fn gpui_command_palette_switch_workarea_hotkey_mode(
    action_id: &str,
) -> Option<TitlebarMode> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-07:24:
    Shared command-palette workarea rows post ordinary hotkey ids through `runGhostexHotkeyAction`. GPUI must translate only those exact switch ids to titlebar modes, with the shared GitHub row targeting the current Browser workarea field, then execute through the same titlebar availability and focus route as Option+1..5.
    */
    match action_id {
        "switchAgentsView" => Some(TitlebarMode::Agents),
        "switchSourceView" => Some(TitlebarMode::Source),
        "switchGitHubView" => Some(TitlebarMode::Browser),
        "switchKanbanView" => Some(TitlebarMode::Kanban),
        "switchManageView" => Some(TitlebarMode::Manage),
        _ => None,
    }
}

pub(crate) fn gpui_source_workarea_allowed_configured_hotkey_action_id(action_id: &str) -> bool {
    gpui_workarea_switch_hotkey_action_id(action_id)
        || matches!(
            action_id,
            "focusLeft"
                | "focusRight"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "openCommandsPanel"
                | "toggleCompanionPane"
        )
}

pub(crate) fn gpui_command_palette_action_slot_index(action_id: &str) -> Option<usize> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-10:04:
    Shared Start Action 1-5 command-palette rows post positional `runActionSlot*` ids. Keep this mapper exact and zero-based so GPUI reuses the configured titlebar Actions list order without parsing or transporting private action payload data.
    */
    match action_id {
        "runActionSlot1" => Some(0),
        "runActionSlot2" => Some(1),
        "runActionSlot3" => Some(2),
        "runActionSlot4" => Some(3),
        "runActionSlot5" => Some(4),
        _ => None,
    }
}

pub(crate) fn gpui_command_palette_adjacent_group_focus_direction(
    action_id: &str,
) -> Option<WorkspaceFocusDirection> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-10:04:
    Shared adjacent-group focus rows post `focusPreviousGroup` and `focusNextGroup`, which are render-order commands rather than spatial arrows. Map only those exact ids to the existing previous/next workspace traversal so GPUI mirrors native focusAdjacentGroup without inventing numbered group slots, project jumps, or runtime fallbacks.
    */
    match action_id {
        "focusPreviousGroup" => Some(WorkspaceFocusDirection::Left),
        "focusNextGroup" => Some(WorkspaceFocusDirection::Right),
        _ => None,
    }
}

pub(crate) fn gpui_command_palette_adjacent_group_focus_source_allowed(
    shell_focus: ShellFocusTarget,
) -> bool {
    /*
    CDXC:GPUICommandPalette 2026-06-26-10:04:
    Adjacent-group focus is scoped to the command/Agents render-order model. Browser and project-editor focus do not enter this route, because doing so would turn a shared command-palette row into a cross-workarea project jump.
    */
    matches!(
        shell_focus,
        ShellFocusTarget::AgentsPane(_) | ShellFocusTarget::CommandPane
    )
}

pub(crate) fn gpui_focused_pane_open_browser_hotkey_should_open(
    shell_focus: ShellFocusTarget,
) -> bool {
    /*
    CDXC:GPUICommandPalette 2026-06-26-06:47:
    Native `runFocusedPaneHotkeyAction("openBrowserPane")` dispatches through `handleNativeTerminalTitleBarAction`; a live command terminal hits the command-panel branch and default-returns. GPUI should preserve that no-op for CommandPane focus while keeping Browser creation for Agents, Browser, and project-editor focus.
    */
    !matches!(shell_focus, ShellFocusTarget::CommandPane)
}

pub(crate) fn gpui_focused_pane_rotate_agents_hotkey_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
) -> Option<WorkspacePaneId> {
    /*
    CDXC:GPUIFocusedPaneRotation 2026-06-26-06:56:
    Native focused-pane rotation runs only for the active workspace pane group. GPUI must preserve command-terminal default-return behavior and keep Browser/project-editor focus inert, so only `active_mode == Agents` plus `ShellFocusTarget::AgentsPane(_)` may reach the future workspace rotation mutation.
    */
    match shell_focus {
        ShellFocusTarget::AgentsPane(pane_id) if active_mode == TitlebarMode::Agents => {
            Some(pane_id)
        }
        ShellFocusTarget::CommandPane
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_)
        | ShellFocusTarget::AgentsPane(_) => None,
    }
}

pub(crate) fn apply_rotate_agents_panes_hotkey_model(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &mut WorkspaceModel,
) -> Option<WorkspacePaneId> {
    /*
    CDXC:GPUIFocusedPaneRotation 2026-06-26-06:56:
    The focused-pane rotate hotkey is a thin app-route over the pure Agents workspace rotation model. Keep command/Browser/project-editor focus as no-ops before mutating the workspace, and return the post-rotation focused pane so the app shell can restore focus and clear runtime-only drag/metrics state.
    */
    let _pane_id = gpui_focused_pane_rotate_agents_hotkey_target(active_mode, shell_focus)?;
    if !agents_workspace.rotate_panes_clockwise() {
        return None;
    }
    Some(agents_workspace.focused_pane)
}
