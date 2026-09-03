// C1 wave-3 re-cluster: focused command-pane close/sleep/wake/rename target resolution, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedCommandPaneCloseDecision {
    CloseCommandTab {
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    },
    InterceptNoOp,
    FallThroughToActiveMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusedSurfaceCloseDecision {
    CloseCommandTab {
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    },
    InterceptNoOp,
    CloseProjectEditorCompanionSession(TitlebarMode),
    CloseAgentsActiveTab,
    CloseBrowserActiveTab,
    NoOp,
}

pub(crate) fn focused_command_pane_close_decision(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> FocusedCommandPaneCloseDecision {
    /*
    CDXC:CommandPane 2026-06-26-05:33:
    Cmd-W should follow native command-panel responder ownership. Expanded live command focus closes awake command tabs, expanded live sleeping placeholders consume the shortcut without closing, and stale/collapsed command focus falls through to the active workspace or Browser close path instead of swallowing Cmd-W.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return FocusedCommandPaneCloseDecision::FallThroughToActiveMode;
    }

    let Some((group_id, session_id)) = command_pane.focused_group_active_session_id() else {
        return FocusedCommandPaneCloseDecision::FallThroughToActiveMode;
    };
    let Some(session) = command_pane.session(session_id) else {
        return FocusedCommandPaneCloseDecision::FallThroughToActiveMode;
    };
    if session.is_sleeping {
        FocusedCommandPaneCloseDecision::InterceptNoOp
    } else {
        FocusedCommandPaneCloseDecision::CloseCommandTab {
            group_id,
            session_id,
        }
    }
}

pub(crate) fn focused_surface_close_decision(
    shell_focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    command_pane: &CommandPaneModel,
) -> FocusedSurfaceCloseDecision {
    /*
    CDXC:FocusMode 2026-06-27-02:58:
    Cmd-W routing mirrors `native/sidebar/native-hotkey-source.test.ts`: expanded live command focus wins first, sleeping command placeholders consume the shortcut, and focused Source/Browser/Kanban/Automate/Manage main project-editor surfaces never inherit Browser tab or workspace-tab close behavior. Browser tabs remain closeable through BrowserSurface, exact BrowserPane focus, or stale/collapsed command fallthrough into Browser's active-surface policy.

    CDXC:FocusMode 2026-07-29-04:29:
    A focused companion is a session surface, so Cmd-W closes its exact focused terminal through the existing workspace lifecycle owner. Companion collapse remains exclusive to its visible titlebar control and must not substitute for session close.
    */
    match focused_command_pane_close_decision(shell_focus, command_pane) {
        FocusedCommandPaneCloseDecision::CloseCommandTab {
            group_id,
            session_id,
        } => {
            return FocusedSurfaceCloseDecision::CloseCommandTab {
                group_id,
                session_id,
            };
        }
        FocusedCommandPaneCloseDecision::InterceptNoOp => {
            return FocusedSurfaceCloseDecision::InterceptNoOp;
        }
        FocusedCommandPaneCloseDecision::FallThroughToActiveMode => {}
    }

    match shell_focus {
        ShellFocusTarget::ProjectEditorCompanion(mode)
            if active_mode == mode && mode.is_project_editor_mode() =>
        {
            FocusedSurfaceCloseDecision::CloseProjectEditorCompanionSession(mode)
        }
        ShellFocusTarget::ProjectEditorSurface(mode) if mode.is_project_editor_mode() => {
            FocusedSurfaceCloseDecision::NoOp
        }
        ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
            if active_mode == TitlebarMode::Browser =>
        {
            FocusedSurfaceCloseDecision::CloseBrowserActiveTab
        }
        ShellFocusTarget::AgentsPane(_) if active_mode == TitlebarMode::Agents => {
            FocusedSurfaceCloseDecision::CloseAgentsActiveTab
        }
        ShellFocusTarget::CommandPane => match active_mode {
            TitlebarMode::Agents => FocusedSurfaceCloseDecision::CloseAgentsActiveTab,
            TitlebarMode::Browser => FocusedSurfaceCloseDecision::CloseBrowserActiveTab,
            TitlebarMode::Source
            | TitlebarMode::Kanban
            | TitlebarMode::Automate
            | TitlebarMode::Manage
            | TitlebarMode::Extension(_) => FocusedSurfaceCloseDecision::NoOp,
        },
        ShellFocusTarget::AgentsPane(_)
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_) => FocusedSurfaceCloseDecision::NoOp,
    }
}

pub(crate) fn focused_command_pane_close_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:FocusMode 2026-06-25-15:05:
    Close Focused Session from the shared command-palette bridge should close the command terminal only when the command pane owns shell focus. Non-command focus remains out of this command-pane parity path so GPUI does not widen command-pane work into unrelated surface close behavior.

    CDXC:CommandPane 2026-06-25-17:37:
    Focused command close requires an expanded command pane with an active command tab, matching native live first-responder routing. Collapsed command strips can show tabs but do not own terminal typing focus and must not close from focused-session commands.

    CDXC:CommandPane 2026-06-25-18:24:
    Native Cmd-W over the Commands panel requires `commandPanelFocusedResponderSessionId`, which accepts active command terminals rather than sleeping placeholder-only command tabs. Keep focused command close out of sleeping command tabs; tab close buttons, middle-click, and context-menu close scopes still own explicit sleeping-tab close.
    */
    if let FocusedCommandPaneCloseDecision::CloseCommandTab {
        group_id,
        session_id,
    } = focused_command_pane_close_decision(shell_focus, command_pane)
    {
        Some((group_id, session_id))
    } else {
        None
    }
}

pub(crate) fn focused_command_pane_sleep_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:FocusMode 2026-06-25-14:56:
    Sleep Focused Session should target the active command terminal only when the command pane owns shell focus and is visibly expanded, matching native focused-session routing from AppKit first responder state. Collapsed strips, non-command focus, missing sessions, and already sleeping command tabs must no-op instead of mutating stale command state.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some_and(|session| !session.is_sleeping)
        .then_some((group_id, session_id))
}

pub(crate) fn focused_command_pane_rename_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:FocusMode 2026-06-25-16:33:
    Rename Active Session is a focused-session action in native command panes. In GPUI, route it only when the expanded command pane owns shell focus, then open the shared Rename Session modal for the active command tab without deriving titles from command text, paths, output, or persisted shell JSON.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }
    command_pane.focused_group_active_session_id()
}

pub(crate) fn focused_command_pane_wake_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:FocusMode 2026-06-25-15:01:
    Wake Focused Session is the inverse focused command-terminal lifecycle action. Resolve only the expanded command pane's active sleeping tab while it owns shell focus, matching native command-palette focused-session routing without waking non-command focus, running command tabs, or collapsed command strips.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some_and(|session| session.is_sleeping)
        .then_some((group_id, session_id))
}

pub(crate) fn command_terminal_surface_focus_states_for_slots(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
    mounted_slot_ids: &[CommandTerminalBodyMountSlotId],
) -> Vec<(CommandTerminalBodyMountSlotId, bool)> {
    let focused_slot_id = focused_command_terminal_surface_mount_slot(shell_focus, command_pane)
        .filter(|slot_id| mounted_slot_ids.contains(slot_id));

    mounted_slot_ids
        .iter()
        .copied()
        .map(|slot_id| (slot_id, Some(slot_id) == focused_slot_id))
        .collect()
}
