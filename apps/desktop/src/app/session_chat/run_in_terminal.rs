//! A chat's Run in terminal and Paste in terminal: the command a reply showed, run (Enter pressed)
//! or only typed, in a terminal of the Commands panel.
//!
//! CDXC:SessionChat 2026-10-24 DECISION: User: "please make clicking on the floating terminal button that we added add that command to a terminal in the commands panel not a new terminal session in the sidebar". Run and Paste in terminal type into the bottom Commands panel's terminal: its focused plain terminal tab when that is idle (not an Action's tab, not running something), else another idle plain tab, else a new tab; the panel opens and that tab takes focus. Supersedes the earlier new sidebar terminal session. The chat's own terminal never gets the command, where the agent would read it as a prompt. A session outside the project the Commands panel belongs to (the window's active project) keeps the new terminal session in its own folder (`gx_store/create/command_terminal.rs`).
//! SEE-ALSO: packages/gx-chat-core/src/transcript/run_command.rs decides which commands qualify and what runs; `ghostex run-in-terminal` (server/src/ghostex_cli/run_in_terminal.rs) types into the same Commands panel terminal for the phone.

use ghostex_gx_core::{MachineId, ProjectKey, SessionKey};
use gpui::Window;

use crate::app::model::*;
use crate::*;

/// Ctrl-U: the shell clears the line being typed at its prompt.
const CLEAR_PROMPT_LINE: &str = "\u{15}";

impl GhostexGpuiApp {
    pub(crate) fn run_session_chat_command_in_terminal(
        &mut self,
        session_id: TerminalSessionId,
        command: &str,
        submit: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let session = match self.workspace_terminal_key_for_shell_session(session_id) {
            Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                SessionKey::local(key.project_id, key.session_id)
            }
            Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => {
                SessionKey::remote(key.remote_machine_id, key.project_id, key.session_id)
            }
            None => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not run the command",
                    "This chat's session is not open.",
                    cx,
                );
                return;
            }
        };
        if self.command_pane_belongs_to_session_project(&session) {
            self.send_session_chat_command_to_command_pane(command, submit, window, cx);
            return;
        }
        let cwd = self
            .gx_store
            .core
            .presentation()
            .session(&session)
            .and_then(|row| row.cwd.clone());
        self.gx_store_create_command_terminal(
            ProjectKey {
                machine: session.machine,
                project_id: session.project_id,
            },
            cwd,
            command.to_string(),
            submit,
            cx,
        );
    }

    /// Whether the Commands panel shown now is the session's project's: the panel follows the
    /// window's active project, on this computer or a remote one.
    fn command_pane_belongs_to_session_project(&self, session: &SessionKey) -> bool {
        let remote = self.active_gpui_remote_project_reference();
        match &session.machine {
            MachineId::Local => {
                remote.is_none()
                    && gpui_active_project_id_from_snapshot(
                        self.latest_sidebar_project_snapshot.as_ref(),
                    ) == Some(session.project_id.as_str())
            }
            MachineId::Remote(machine_id) => remote.is_some_and(|remote| {
                remote.remote_machine_id == *machine_id && remote.project_id == session.project_id
            }),
        }
    }

    /// The Commands panel tab a command goes to: the focused group's active tab, else the active
    /// tab of another group, when it is a plain terminal of the bottom panel that is not busy.
    fn idle_command_pane_terminal(&self) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        let idle = |group_id: CommandPaneGroupId, session_id: CommandSessionId| {
            self.command_pane.dock_for_group(group_id) == Some(CommandPaneDock::Panel)
                && self
                    .command_pane
                    .session(session_id)
                    .is_some_and(|session| {
                        session.action_command_id.is_none()
                            && session.remote_action_session.is_none()
                            && session.activity != CommandTerminalActivity::Working
                            && !session.is_sleeping
                            && !session.delayed_send_active
                    })
        };
        self.command_pane
            .focused_group_active_session_id()
            .filter(|(group_id, session_id)| idle(*group_id, *session_id))
            .or_else(|| {
                self.command_pane
                    .group_order()
                    .into_iter()
                    .find_map(|group_id| {
                        let session_id = self
                            .command_pane
                            .find_leaf(group_id)?
                            .tab_group
                            .active_session_id()?;
                        idle(group_id, session_id).then_some((group_id, session_id))
                    })
            })
    }

    /// Types `command` into a terminal of the Commands panel, pressing Enter after it when
    /// `submit`, then opens the panel on that tab. A tab whose terminal is on screen is typed into
    /// directly; any other gets the text as its attach input, the way a rerun Action does
    /// (`titlebar/action_execution.rs`).
    fn send_session_chat_command_to_command_pane(
        &mut self,
        command: &str,
        submit: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.command_pane.is_expanded() {
            self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
        }
        let attach_input = if submit {
            format!("{command}\r")
        } else {
            command.to_string()
        };
        // A reused tab's prompt may hold half-typed text (a command pasted earlier and never run),
        // which Ctrl-U clears so the command is not appended to it.
        let reused_input = format!("{CLEAR_PROMPT_LINE}{attach_input}");
        let slot = match self.idle_command_pane_terminal() {
            Some((group_id, session_id)) => {
                self.command_pane
                    .select_session_in_group(group_id, session_id);
                let slot = CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                };
                /*
                CDXC:SessionChat 2026-11-18 WHY:
                User (Discord, Leech): "I cannot send code to the terminal.. it opens the terminal and nothing is pasted"; a second click worked. With the Commands panel closed, the tab's terminal is not on screen, so the command used to go out as the attach input of a fresh attach. The tab's terminal view is usually still alive behind the closed panel, and a tab that already has a view never takes an attach input, so the panel opened on that terminal with nothing typed. Any live view of this exact tab is typed into directly, shown or not; only a tab with no view takes the attach path.
                */
                let live_view = self
                    .command_gpui_engine_terminals
                    .get(&session_id)
                    .map(|record| record.view.clone());
                if let Some(view) = live_view {
                    view.update(cx, |view, cx| {
                        view.send_text_input(&format!("{CLEAR_PROMPT_LINE}{command}"), cx);
                        if submit {
                            view.send_return_key(cx);
                        }
                    });
                } else {
                    let title = self
                        .command_pane
                        .session(session_id)
                        .map(|session| session.title.clone())
                        .unwrap_or_else(|| COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string());
                    self.start_command_terminal_gxserver_attach_for_slot(
                        slot,
                        title,
                        Some(reused_input),
                        None,
                        None,
                        cx,
                    );
                }
                slot
            }
            None => {
                let target_group = self.command_pane.bottom_dock_target_group();
                let Some((group_id, session_id)) = self.command_pane.add_new_session(target_group)
                else {
                    return;
                };
                let slot = CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                };
                self.start_command_terminal_gxserver_attach_for_slot(
                    slot,
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                    Some(attach_input),
                    None,
                    None,
                    cx,
                );
                slot
            }
        };
        if !self.command_pane.is_expanded() {
            self.open_command_pane_from_shared_settings(window, cx);
        }
        self.command_pane
            .acknowledge_attention_for_focused_session_activation();
        self.focus_command_pane(cx);
        self.request_command_terminal_text_focus_handoff(slot);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }
}
