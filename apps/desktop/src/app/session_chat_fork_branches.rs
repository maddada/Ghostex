//! The chat branch switcher's `selectForkBranch` host action.
//!
//! CDXC:SessionFork 2026-09-18 SEE-ALSO:
//! GPUI chat's strip is `apps/desktop/src/app/native_chat/fork_branches.rs` and the rows come from
//! `packages/gx-chat-core/src/menus/picker/fork_branches.rs`. This action must keep waking a
//! stopped branch before focusing it.

use crate::*;
use ghostex_gx_core::MachineId;
use std::time::Duration;

impl GhostexGpuiApp {
    /*
    CDXC:SessionFork 2026-09-03:
    A fork's ancestor is usually STOPPED: forking kills the source's provider and Previous Sessions
    hides the row. A stopped row is not in the live presentation, so focusing it alone finds no
    session and the click does nothing. Wake it first: `/api/wakeSession` has no lifecycle guard,
    respawns the provider with the row's saved agent resume command, marks the SAME registry row
    running, and broadcasts its presentation delta before it answers, so the follow-up focus
    resolves. Reviving in place keeps the family edge intact; a Previous-Sessions-style restore
    would create a new row and remove the parent both leaves point at.
    */
    pub(crate) fn select_session_chat_fork_branch(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let field = |name: &str| {
            message
                .get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let (Some(project_id), Some(branch_session_id)) = (field("projectId"), field("sessionId"))
        else {
            return;
        };
        // The family is derived by the daemon that owns this chat, so a remote session's branches
        // live on that same machine and take its tunnel.
        // A Cloud Box start names the box's machine (`cloudbox:<boxId>`) the new session is on.
        // CDXC:RemoteMachines 2026-11-04 WHY: a coordinator's worker on another computer than the
        // coordinator's names that computer's gxserver (`serverId`, server/coordinator_open_http.rs),
        // which may be this computer itself when the coordinator runs elsewhere. Without a machine
        // for it the click must not open a session of the same ids on the coordinator's computer.
        let on_server = match field("serverId") {
            Some(server_id) => match self.machine_for_server_id(&server_id) {
                Some(machine) => Some(machine),
                None => {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not open that worker",
                        "Its computer isn't connected. Connect it in Settings › Remote and try again.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let remote_machine_id = match (
            on_server,
            field("machineId"),
            self.workspace_terminal_key_for_shell_session(session_id),
        ) {
            (Some(MachineId::Local), _, _) => None,
            (Some(MachineId::Remote(machine_id)), _, _) => Some(machine_id),
            (None, Some(machine_id), _) => Some(machine_id),
            (None, None, Some(GpuiWorkspaceTerminalSessionKey::Remote(key))) => {
                Some(key.remote_machine_id)
            }
            _ => None,
        };
        let sidebar_session_id = match &remote_machine_id {
            Some(machine_id) => {
                gpui_remote_scoped_session_id(machine_id, &project_id, &branch_session_id)
            }
            None => gpui_combined_presentation_session_id(&project_id, &branch_session_id),
        };
        if field("lifecycleState").as_deref() != Some("stopped") {
            self.focus_session_chat_fork_branch(&sidebar_session_id, cx);
            return;
        }
        let remote = remote_machine_id
            .as_deref()
            .and_then(|machine_id| self.gpui_remote_gxserver_request_target(machine_id));
        if remote_machine_id.is_some() && remote.is_none() {
            self.report_session_chat_fork_branch_failure(cx);
            return;
        }
        let params = serde_json::json!({
            "projectId": project_id,
            "sessionId": branch_session_id,
        });
        cx.spawn(async move |this, cx| {
            let woken = cx
                .background_executor()
                .spawn(async move {
                    let response = match remote {
                        Some(target) => gpui_remote_gxserver_post_typed_operation(
                            &target,
                            "/api/wakeSession",
                            &params,
                            Duration::from_secs(60),
                        ),
                        None => gxserver_post_typed_operation(
                            "/api/wakeSession",
                            &params,
                            Duration::from_secs(60),
                        ),
                    };
                    matches!(response, Ok((status, _)) if (200..300).contains(&status))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if woken {
                    this.focus_session_chat_fork_branch(&sidebar_session_id, cx);
                } else {
                    this.report_session_chat_fork_branch_failure(cx);
                }
            });
        })
        .detach();
    }

    /// The same route the Find Prompts modal takes: the Rust store owns focus (gx_store/activation_focus.rs) and the reveal
    /// expands and scrolls the containers holding the row.
    fn focus_session_chat_fork_branch(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.dispatch_gpui_command_palette_session_focus(sidebar_session_id, cx) {
            // In the window the focus went to (workspace_windows/session_routing.rs).
            let row_id = sidebar_session_id.to_string();
            self.run_in_session_window(sidebar_session_id, cx, move |app, cx| {
                app.reveal_sidebar_session(&row_id, cx)
            });
        } else {
            self.report_session_chat_fork_branch_failure(cx);
        }
    }

    /// The machine whose gxserver is `server_id`: this computer, or a remote machine the store
    /// holds rows of, else a connected computer the registration named
    /// (remote_conn/connected_computers.rs).
    fn machine_for_server_id(&self, server_id: &str) -> Option<MachineId> {
        self.gx_store
            .core
            .presentation()
            .machines()
            .find(|(_, entry)| {
                entry
                    .loaded()
                    .is_some_and(|loaded| loaded.server_id == server_id)
            })
            .map(|(machine, _)| machine.clone())
            .or_else(|| {
                crate::app::remote_conn::connected_computers::gpui_remote_machine_for_server_id(
                    server_id,
                )
                .map(MachineId::Remote)
            })
    }

    fn report_session_chat_fork_branch_failure(&mut self, cx: &mut gpui::Context<Self>) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Could not open that branch",
            "The session could not be resumed. Try again from the sidebar.",
            cx,
        );
    }
}
