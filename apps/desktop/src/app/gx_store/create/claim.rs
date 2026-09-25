//! The doors F4's commands arrive through, answered in Rust before they reach the old runtime.
//!
//! CDXC:AgentLauncher 2026-09-25 WHY:
//! Every create and open the sidebar, the New Thread picker, Quick Access and Settings ask for used
//! to go to the QuickJS runtime through one of three doors: a wrapped `{type:'command', message}`
//! at the end of `dispatch_native_sidebar_ui`, the `onSidebarHostMessage` allowlist behind
//! `dispatch_gpui_sidebar_host_message`, or an app modal's `sidebarCommand`. A command answered here
//! returns `true` and must go no further, because the runtime would have performed it a second time. Each
//! type is answered at every door it can arrive through, since a port that closed one door and
//! left another would run the action twice or not at all.
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/actions.rs (`dispatch_native_sidebar_ui`),
//! apps/desktop/src/app/sidebar_dispatch.rs (`dispatch_gpui_sidebar_host_message`),
//! apps/desktop/src/app/delayed_send.rs (`handle_gpui_app_modal_sidebar_command`),
//! docs/2026-09-25/app-runtime-port/LEDGER.md (the F4 rows).

use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with F4's commands. Memory only.
#[derive(Default)]
pub(crate) struct CreateHost {
    pub(super) counters: CreateCounters,
    /// A browser open waiting for the project switch it has to follow (browser.rs).
    pub(super) pending_browser_open: Option<super::browser::PendingBrowserOpen>,
    /// Sessions a create here just made, whose attach is the create's own (focus_created.rs).
    pub(super) created_attaches: super::focus_created::CreatedAttaches,
    /// The Project Board's link availability answers, kept for their TTL (board.rs).
    pub(super) board_link_checks: super::board::LinkChecks,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CreateCounters {
    pub(super) browser_pane_opens: u64,
    pub(super) quick_browser_opens: u64,
    pub(super) find_prompts: u64,
    pub(super) project_removes: u64,
    pub(super) project_closes: u64,
    pub(super) terminal_creates: u64,
    pub(super) agent_launches: u64,
    pub(super) hook_dialogs: u64,
    pub(super) created_focuses: u64,
    /// Created sessions not placed into their user-made group because the groups document had
    /// not been read yet.
    pub(super) placements_unread: u64,
    pub(super) folder_picks: u64,
    pub(super) os_integration_commands: u64,
    pub(super) board_requests: u64,
}

impl GhostexGpuiApp {
    /// A wrapped sidebar command at the end of `dispatch_native_sidebar_ui`. Returns whether it was
    /// answered here.
    pub(crate) fn gx_store_run_sidebar_create(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        // New Group, Rename and Close Group (gx_store/workspace_groups/group_commands.rs).
        if self.gx_store_run_group_command(command, message, cx) {
            return true;
        }
        // Close Project carries the successor the store names from the list it draws
        // (sidebar_close_project.rs).
        if message.get("type").and_then(Value::as_str) == Some("closeWorkspaceProjectForGroup") {
            let command = self.gx_store_add_close_project_successor(command.clone());
            self.gx_store_close_project_for_group(&command["message"], cx);
            return true;
        }
        self.gx_store_answer_create_message(message, cx)
    }

    /// A message on the `onSidebarHostMessage` allowlist (`dispatch_gpui_sidebar_host_message`).
    /// Returns whether it was answered here.
    pub(crate) fn gx_store_claim_sidebar_host_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The host message door also made the launched agent the launcher's highlighted one.
        if message.get("type").and_then(Value::as_str) == Some("runSidebarAgent") {
            self.gx_store_run_sidebar_agent_message(message, cx);
            return true;
        }
        self.gx_store_answer_create_message(message, cx)
    }

    /// An app modal's `sidebarCommand` whose type `handle_gpui_app_modal_sidebar_command` has no
    /// arm of its own for. Returns whether it was answered here.
    ///
    /// Quick Access "Quick Browser Tab" and the command palette post `openBrowserChat` here, and the
    /// handler used to drop it at `_ => {}`: the row did nothing (ledger H010).
    pub(crate) fn gx_store_run_app_modal_create_command(
        &mut self,
        command_type: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command_type {
            "openBrowserChat" => {
                self.gx_store_open_quick_browser_tab(cx);
                true
            }
            // Quick Access "Quick Terminal" and the command palette, dropped here before
            // (ledger H009).
            "createChat" => {
                self.gx_store_create_quick_terminal(cx);
                true
            }
            _ => false,
        }
    }

    /// The one switch every door shares, on the runtime's own message shape.
    fn gx_store_answer_create_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match message.get("type").and_then(Value::as_str) {
            Some("openBrowserPaneInGroup") => {
                let group_id = message.get("groupId").and_then(Value::as_str);
                self.gx_store_open_browser_pane_in_group(group_id, cx);
                true
            }
            Some("openBrowserChat") => {
                self.gx_store_open_quick_browser_tab(cx);
                true
            }
            Some("searchPreviousSessionsByText") => {
                self.gx_store_open_find_prompts(cx);
                true
            }
            // The empty-sidebar double-click and the New Thread picker's Terminal name no group:
            // the active one takes it (`createSession(groupId = this.activeGroupId)`).
            Some("createSession") => {
                self.gx_store_create_terminal(None, cx).detach();
                true
            }
            Some("createSessionInGroup") => {
                let group_id = message.get("groupId").and_then(Value::as_str);
                self.gx_store_create_terminal(group_id, cx).detach();
                true
            }
            Some("createProjectTerminal") => {
                self.gx_store_create_project_terminal(message, cx);
                true
            }
            Some("createChat") => {
                self.gx_store_create_quick_terminal(cx);
                true
            }
            Some("runSidebarAgent") => {
                let Some(agent_id) = message.get("agentId").and_then(Value::as_str) else {
                    return true;
                };
                let group_id = message.get("groupId").and_then(Value::as_str);
                let account_id = message.get("accountId").and_then(Value::as_str);
                self.gx_store_request_agent_launch(agent_id, group_id, account_id, cx);
                true
            }
            Some("confirmAgentHookLaunch") => {
                self.gx_store_confirm_agent_hook_launch(message, cx);
                true
            }
            Some("updateCustomSessionTags") => {
                self.gx_store_update_custom_session_tags(message, cx);
                true
            }
            Some("removeProject") => {
                if let Some(project_id) = message.get("projectId").and_then(Value::as_str) {
                    self.gx_store_remove_project(project_id, cx);
                }
                true
            }
            Some("removeWorkspaceProjectForGroup") => {
                if let Some(group_id) = message.get("groupId").and_then(Value::as_str) {
                    self.gx_store_remove_project_for_group(group_id, cx);
                }
                true
            }
            _ => false,
        }
    }
}
