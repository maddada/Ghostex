//! The chat branch switcher's `selectForkBranch` host action.
//!
//! CDXC:SessionFork 2026-09-18 SEE-ALSO:
//! GPUI chat's strip is `apps/desktop/src/app/native_chat/fork_branches.rs` and the rows come from
//! `packages/gx-chat-core/src/menus/picker/fork_branches.rs`. This action must keep waking a
//! stopped branch before focusing it.

use crate::*;
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
        let remote_machine_id = match self.workspace_terminal_key_for_shell_session(session_id) {
            Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => Some(key.remote_machine_id),
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

    /// The same route the Find Prompts modal takes: the sidebar runtime owns focus and the reveal
    /// expands and scrolls the containers holding the row.
    fn focus_session_chat_fork_branch(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.dispatch_gpui_command_palette_session_focus(sidebar_session_id, cx) {
            self.reveal_sidebar_session(sidebar_session_id, cx);
        } else {
            self.report_session_chat_fork_branch_failure(cx);
        }
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
