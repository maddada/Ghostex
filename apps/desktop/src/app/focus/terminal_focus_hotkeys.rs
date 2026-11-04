//! Unfocus Terminal and Focus Last Terminal: the hotkeys that take the keyboard out of a terminal,
//! so the normal hotkeys apply, and put it back in the terminal that last had it.
//!
//! CDXC:Hotkeys 2026-10-27 WHY:
//! The user asked for "a hotkey to unfocus the terminal and also another one refocus the last active terminal" next to the Terminal focused section of Settings > Hotkeys (CDXC:Hotkeys 2026-10-27 DECISION in packages/settings-catalog/src/hotkey_definitions.rs). Unfocusing parks the keyboard on the root's `root_action_focus_handle`, the same place a hidden terminal releases it (`sync_composited_terminal_keyboard_owner`), so the pane keeps its shell focus and every pane hotkey still targets it, but no terminal is on the key path and the normal bindings apply; it never moves the keyboard into a text field.

use gpui::{Focusable as _, Window};

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// Takes the keyboard out of the focused terminal. Returns false when no terminal had it.
    pub(crate) fn unfocus_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let owner = self.composited_terminal_keyboard_owner.take();
        let focused_target = owner.map(|(_, target)| target).or_else(|| {
            self.last_keyboard_terminal.filter(|target| {
                self.gpui_engine_terminal_view_for_target(*target)
                    .is_some_and(|view| view.read(cx).focus_handle(cx).is_focused(window))
            })
        });
        let Some(target) = focused_target else {
            return false;
        };
        self.last_keyboard_terminal = Some(target);
        #[cfg(target_os = "macos")]
        if let Some((root, _)) = owner {
            update_gpui_keyboard_router_composited_terminal_focus(
                root as *mut std::ffi::c_void,
                target,
                false,
                self.first_responder_target,
            );
        }
        window.focus(&self.root_action_focus_handle, cx);
        cx.notify();
        true
    }

    /// Puts the keyboard back in the terminal that last had it: its tab is selected in its pane,
    /// and a session no longer on screen comes to the focused pane, as a sidebar click brings it.
    /// Returns false when there is none or it has closed since.
    pub(crate) fn focus_last_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match self.last_keyboard_terminal {
            Some(GpuiEngineTerminalEventTarget::Agents(session_id)) => {
                let key = self
                    .local_workspace_session_mappings
                    .iter()
                    .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()));
                if let Some(key) = key
                    && self.focus_existing_gpui_local_workspace_terminal(&key, cx)
                {
                    return true;
                }
                let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
                    return false;
                };
                let pane_id = self.pull_workspace_session_into_focused_pane(pane_id, session_id);
                focus_existing_local_workspace_terminal_tab_model(
                    &mut self.agents_workspace,
                    &mut self.agents_terminal_runtime_sessions,
                    pane_id,
                    session_id,
                );
                self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
                self.request_agents_session_text_focus_handoff(
                    AgentsTerminalBodyMountSlotId {
                        pane_id,
                        session_id,
                    },
                    cx,
                );
                cx.notify();
                true
            }
            Some(GpuiEngineTerminalEventTarget::Command(session_id)) => {
                let Some(group_id) = command_pane_group_for_session(&self.command_pane, session_id)
                else {
                    return false;
                };
                if !self.select_command_pane_tab(group_id, session_id, true, window, cx) {
                    return false;
                }
                self.remember_current_non_command_focus();
                self.focus_shell_target_now(ShellFocusTarget::CommandPane, window, cx);
                cx.notify();
                true
            }
            None => false,
        }
    }
}
