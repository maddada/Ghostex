// C1 wave-3 re-cluster: command pane action-run session selection, completion, and control-focus dispatch, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneActionSessionSelectionKind {
    Created,
    Reused,
    ReusedActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneActionSessionSelection {
    pub(crate) kind: CommandPaneActionSessionSelectionKind,
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneActionRunCompletedTab {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarCommandRunState {
    Error,
    Running,
    Success,
}

impl GpuiSidebarCommandRunState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            GpuiSidebarCommandRunState::Error => "error",
            GpuiSidebarCommandRunState::Running => "running",
            GpuiSidebarCommandRunState::Success => "success",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSidebarCommandRunFeedbackState {
    pub(crate) active_run_ids: Vec<String>,
    pub(crate) status: Option<GpuiSidebarCommandRunState>,
}

impl GpuiSidebarCommandRunFeedbackState {
    pub(crate) fn apply_run_state(&mut self, run_id: &str, state: GpuiSidebarCommandRunState) {
        match state {
            GpuiSidebarCommandRunState::Running => {
                if !self
                    .active_run_ids
                    .iter()
                    .any(|active_run_id| active_run_id == run_id)
                {
                    self.active_run_ids.push(run_id.to_string());
                }
                self.status = Some(GpuiSidebarCommandRunState::Running);
            }
            GpuiSidebarCommandRunState::Success | GpuiSidebarCommandRunState::Error => {
                self.active_run_ids
                    .retain(|active_run_id| active_run_id != run_id);
                self.status = Some(if self.active_run_ids.is_empty() {
                    state
                } else {
                    GpuiSidebarCommandRunState::Running
                });
            }
        }
    }

    pub(crate) fn titlebar_action_run_mode_for_click(
        &self,
        action: &GpuiTitlebarAction,
    ) -> GpuiTitlebarActionRunMode {
        /*
        CDXC:Titlebar 2026-06-27-09:26:
        GPUI Rust titlebar Actions do not run through the React command-palette click handler, so they need the same sanitized feedback rule locally: a close-on-exit terminal Action reruns in Debug only after its previous run ended in error and no newer run is active. Store only run ids plus coarse state; never derive Debug from command text, URLs, paths, env, output, status files, terminal content, or logs.
        */
        if action.run_mode != GpuiTitlebarActionRunMode::Default {
            return action.run_mode;
        }
        if action.action_type == GpuiTitlebarActionType::Terminal
            && action.close_terminal_on_exit
            && self.status == Some(GpuiSidebarCommandRunState::Error)
            && self.active_run_ids.is_empty()
        {
            GpuiTitlebarActionRunMode::Debug
        } else {
            GpuiTitlebarActionRunMode::Default
        }
    }
}

pub(crate) fn gpui_titlebar_action_run_mode_for_click(
    action: &GpuiTitlebarAction,
    feedback: Option<&GpuiSidebarCommandRunFeedbackState>,
) -> GpuiTitlebarActionRunMode {
    if action.run_mode != GpuiTitlebarActionRunMode::Default {
        return action.run_mode;
    }
    feedback
        .map(|feedback| feedback.titlebar_action_run_mode_for_click(action))
        .unwrap_or(GpuiTitlebarActionRunMode::Default)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneActionRunCompletion {
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command_id: String,
    pub(crate) completed_tab: Option<CommandPaneActionRunCompletedTab>,
    pub(crate) exit_code: i32,
    pub(crate) play_completion_sound: bool,
    pub(crate) run_id: String,
}

impl CommandPaneActionRunCompletion {
    pub(crate) fn run_state(&self) -> GpuiSidebarCommandRunState {
        if self.exit_code == 0 {
            GpuiSidebarCommandRunState::Success
        } else {
            GpuiSidebarCommandRunState::Error
        }
    }

    pub(crate) fn should_play_completion_sound(&self) -> bool {
        self.exit_code != 0 || self.play_completion_sound
    }
}

pub(crate) fn gpui_command_pane_action_runtime_close_terminal_on_exit(
    _requested_close_terminal_on_exit: bool,
) -> bool {
    /*
    CDXC:CommandPane 2026-06-26-04:59:
    Native `runNativeSidebarCommand` forces command-pane Action close-on-exit off so each Action keeps a reusable command tab after completion. GPUI may still parse and save legacy `closeTerminalOnExit` fields, but every command-pane Action runtime boundary must normalize the requested value to false and must not close the completed Action tab.
    */
    false
}

pub(crate) fn gpui_command_pane_default_action_should_focus_command_pane() -> bool {
    /*
    CDXC:CommandPane 2026-06-27-01:45:
    Default terminal Actions mirror native `runNativeSidebarCommand` with `createCommandTerminal(..., { focusAfterCreate: false })`: select/open the Action command tab and publish running state without moving shell typing focus into CommandPane. Do not add fallback focus behavior here; focus remains explicit through command-pane controls, tab clicks, direct context actions, F12/open-panel routes, command hotkeys, and Debug Actions.

    CDXC:CommandPane 2026-06-27-01:45:
    The default Action focus policy is privacy-neutral and state-only: it must not inspect command text, cwd, project paths, terminal output, URLs, titles, env vars, or user content to decide focus. Native parity is unconditional for default terminal Actions.
    */
    false
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandPaneActionRunRefresh {
    pub(crate) changed: bool,
    pub(crate) completions: Vec<CommandPaneActionRunCompletion>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandTerminalProcessExitCleanup {
    pub(crate) changed: bool,
    pub(crate) completions: Vec<CommandPaneActionRunCompletion>,
}

#[derive(Clone, Copy)]
pub(crate) enum CommandPaneControlAction {
    NewCommandPlaceholder,
    TogglePinned,
    ToggleExpanded,
}

pub(crate) fn command_pane_control_action_selects_clicked_group_before_dispatch(
    action: CommandPaneControlAction,
) -> bool {
    /*
    CDXC:CommandPane 2026-06-25-22:01:
    Native command titlebar actions call through the clicked owner titlebar and focus that command session before dispatch. GPUI fixed command-panel controls must retarget the clicked command group for Pin/Unpin and Minimize instead of falling back to the previously focused group; New Terminal already carries explicit insertion targeting.
    */
    matches!(
        action,
        CommandPaneControlAction::TogglePinned | CommandPaneControlAction::ToggleExpanded
    )
}

pub(crate) fn command_pane_focus_clicked_control_group(
    command_pane: &mut CommandPaneModel,
    action: CommandPaneControlAction,
    target_group_id: Option<CommandPaneGroupId>,
) -> bool {
    if !command_pane_control_action_selects_clicked_group_before_dispatch(action) {
        return true;
    }

    let Some(group_id) = target_group_id else {
        return true;
    };

    command_pane.focus_group(group_id)
}

pub(crate) fn command_pane_control_action_focuses_command_pane(
    action: CommandPaneControlAction,
    was_expanded: bool,
    is_expanded_after: bool,
) -> bool {
    /*
    CDXC:FocusRouting 2026-06-25-18:27:
    Native command titlebar actions focus the command terminal before dispatch, including Pin/Unpin on already-expanded panels. GPUI command-pane controls use the same focus policy so action clicks update command-pane focus without clearing project-editor workspace mode.

    CDXC:Notifications 2026-06-25-19:58:
    Any titlebar control that focuses an existing command session should also acknowledge that session's Attention state, matching native command titlebar focus while leaving non-focusing Minimize and new idle command creation out of the acknowledgement path.
    */
    match action {
        CommandPaneControlAction::NewCommandPlaceholder => true,
        CommandPaneControlAction::TogglePinned => is_expanded_after,
        CommandPaneControlAction::ToggleExpanded => !was_expanded && is_expanded_after,
    }
}
