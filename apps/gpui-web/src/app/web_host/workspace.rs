//! What a created, focused or activated session or project does to the work area. The desktop places it in its pane workspace (`focus_local_workspace_terminal_from_message`, `swap_agents_workspace_to_project_id` and friends); the page has one work area that shows the open session's chat or terminal, so these open the session there.
use ghostex_gx_core::{MachineId, ProjectKey, SessionKey};
use gpui::{AnyWindowHandle, Context, Window};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

impl GhostexGpuiApp {
    /// The page has one work area, whose project is the open session's; there is no workspace to swap.
    pub(crate) fn swap_agents_workspace_to_project_id(
        &mut self,
        _new_project_id: Option<String>,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    /// A project activation (a worktree the Git menu just made, a menu bar row on the desktop): the project's first drawn session opens, else its group is focused.
    pub(crate) fn dispatch_gpui_menu_bar_project_activation(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = ProjectKey::parse_workspace_project_id(project_id) else {
            return false;
        };
        let first = self
            .gx_store
            .sidebar_view()
            .group(&project.to_sidebar_group_id())
            .and_then(|group| group.core.sessions.first())
            .and_then(|session| session.row.key.clone());
        match first {
            Some(session) => self.web_open_session_in_work_area(session, false, cx),
            None => self.gx_store_handle(
                ghostex_gx_core::Event::Intent(ghostex_gx_core::Intent::FocusProject { project }),
                cx,
            ),
        }
        true
    }

    /// The active project, from the store's focus: the page's only notion of it.
    pub(crate) fn gpui_app_modal_active_project_id(&self) -> Option<String> {
        self.gx_store
            .core
            .focus()
            .active_project
            .as_ref()
            .filter(|project| project.machine == MachineId::Local)
            .map(|project| project.project_id.clone())
    }

    /// No tunnels in a page (`remote_conn/`).
    pub(crate) fn gpui_remote_gxserver_request_target(
        &self,
        _remote_machine_id: &str,
    ) -> Option<GpuiRemoteGxserverRequestTarget> {
        None
    }

    /// A message for the open app modal: the page's only modals are native dialogs.
    pub(crate) fn dispatch_open_gpui_app_modal_message(
        &mut self,
        message: Value,
        cx: &mut Context<Self>,
    ) {
        self.receive_native_app_modal_message(&message, cx);
    }

    /// Handoff / Export's Open Location: the file is on the computer, which a page can't open.
    pub(crate) fn reveal_gpui_exported_transcript(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Open Location is not available in the browser",
            "Copy the path, or open it from the Ghostex app.",
            cx,
        );
    }

    /// The page's agents arrive with the HUD (gx_store/host.rs), so there is nothing to refresh.
    pub(crate) fn refresh_gpui_new_thread_picker_agents(&mut self, _cx: &mut Context<Self>) {}

    /// A handoff's computers: a page reaches only the computer that serves it.
    pub(crate) fn gpui_add_project_dialog_machine_options(&self) -> Value {
        serde_json::json!([{ "label": "This computer", "machineId": "local" }])
    }

    /// The page reaches one computer, so a handoff there has nothing to clone.
    pub(crate) fn open_gpui_add_project_modal(
        &mut self,
        _message: &Value,
        _cx: &mut Context<Self>,
    ) {
    }

    /// Attaching images from disk to a worktree's first prompt needs a file picker the page does not have.
    pub(crate) fn handle_gpui_pick_worktree_images_message(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Not available in the browser",
            "Attaching images from disk needs the Ghostex app.",
            cx,
        );
    }

    /// A bot's Edit SOUL and Edit config open in the Code view, which the page does not draw.
    pub(crate) fn open_bot_file_in_code_view(
        &mut self,
        _group_id: String,
        _project_path: std::path::PathBuf,
        _file_path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Not available in the browser",
            "Editing a bot's files needs the Ghostex app.",
            cx,
        );
    }

    /// Runs `f` with the page's window, the way the desktop defers into its main window.
    pub(crate) fn defer_in_main_window(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let Some(handle): Option<AnyWindowHandle> = self.web_host.main_window else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |app, cx| f(app, window, cx));
            });
        })
        .detach();
    }
}

impl GhostexGpuiApp {
    /// The desktop's tab strip draws the Global Actions; the page has no tab strip.
    pub(crate) fn receive_sidebar_global_actions_payload(
        &mut self,
        _payload: &str,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The desktop refreshes its open Settings window; the page opens no Settings window.
    pub(crate) fn refresh_open_gpui_app_modal_sidebar_state_in_background(
        &mut self,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The desktop's status item reads the same settings; the page has none.
    pub(crate) fn gx_store_indicators_settings_changed(&mut self, _cx: &mut Context<Self>) {}
}

impl GhostexGpuiApp {
    /// The one workspace selection every desktop sidebar action ends in: the page opens the session in its work area, on the surface the message prefers.
    pub(crate) fn focus_local_workspace_terminal_from_message(
        &mut self,
        message: &crate::app::model::GpuiSidebarWorkspaceTerminalFocusMessage,
        cx: &mut Context<Self>,
    ) {
        let session = SessionKey::local(message.project_id.as_str(), message.session_id.as_str());
        let terminal = self.web_surface_is_terminal(&session, message.preferred_interface);
        self.web_open_session_in_work_area(session.clone(), terminal, cx);
        if !terminal {
            self.web_claim_session_chat_launch_draft(session, cx);
        }
    }

    /// Claims a staged first-input draft (`firstUserInputDraft` from Handoff / Export) into the
    /// session's chat box when the session opens in Chat (`request_session_chat_launch_draft` on
    /// the desktop).
    fn web_claim_session_chat_launch_draft(&mut self, session: SessionKey, cx: &mut Context<Self>) {
        let params = serde_json::json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let Ok(result) =
                crate::app::gx_store::gx_rpc(None, "/api/claimSessionChatLaunchDraft", params)
                    .await
            else {
                return;
            };
            let content = result
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if content.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if let Some((_, view)) = this.native_chats.get(&session).cloned() {
                    view.update(cx, |view, cx| view.append_prompt(&content, cx));
                }
            });
        })
        .detach();
    }

    /// Which surface a focused session opens on: a plain terminal has no chat to show; a known agent keeps the surface the page is on unless the focus asks for its chat; a session whose row has not arrived yet (a create) opens where the focus prefers.
    pub(crate) fn web_surface_is_terminal(
        &self,
        session: &SessionKey,
        preferred: crate::app::model::GpuiPreferredAgentInterface,
    ) -> bool {
        use crate::app::model::GpuiPreferredAgentInterface;
        if ghostex_gx_core::session_chat_view_unavailable(&self.gx_store.core, session) {
            return true;
        }
        let kind = self
            .gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
            .map(|row| row.kind.clone());
        match kind {
            Some(ghostex_gx_core::protocol::SessionKind::Terminal) => true,
            Some(_) => self.show_terminal && preferred != GpuiPreferredAgentInterface::Chat,
            None => preferred == GpuiPreferredAgentInterface::Terminal,
        }
    }

    /// Whether the store's focus names this session (the desktop's `focus_publish.rs`).
    pub(crate) fn gx_store_selection_names_local_session(
        &self,
        key: &crate::app::model::GpuiLocalWorkspaceSessionKey,
    ) -> bool {
        self.gx_store
            .core
            .focus()
            .focused_session
            .as_ref()
            .is_some_and(|session| {
                session.machine.is_local()
                    && session.project_id == key.project_id
                    && session.session_id == key.session_id
            })
    }

    /// A local selection (the desktop's `local_focus.rs`): the page has no burst to settle, so the store's focus moves at once.
    pub(crate) fn gx_store_select_local_session(
        &mut self,
        key: &crate::app::model::GpuiLocalWorkspaceSessionKey,
        _local_was_sleeping: bool,
        _local_runtime_missing: bool,
        cx: &mut Context<Self>,
    ) {
        self.gx_store_select_opened_session(
            &SessionKey::local(key.project_id.as_str(), key.session_id.as_str()),
            cx,
        );
    }

    /// The store's focus takes a session this page just opened (the desktop's `focus_publish.rs`).
    pub(crate) fn gx_store_select_opened_session(
        &mut self,
        session: &SessionKey,
        cx: &mut Context<Self>,
    ) {
        self.gx_store_handle(
            ghostex_gx_core::Event::Intent(ghostex_gx_core::Intent::FocusSession {
                session: session.clone(),
                visible: None,
            }),
            cx,
        );
    }

    /// The desktop acknowledges a session's attention once its pane settles; the page tracks no attention of its own, so the daemon's state is what the row shows.
    pub(crate) fn gx_store_queue_attention_acknowledge(
        &mut self,
        _key: crate::app::model::GpuiLocalWorkspaceSessionKey,
        _cx: &mut Context<Self>,
    ) {
    }
}

impl GhostexGpuiApp {
    /// The page has one pane and no keyboard handoff to re-run; the page keeps its own focus when Quick Access closes.
    pub(crate) fn restore_keyboard_focus_after_app_modal(&mut self, _cx: &mut Context<Self>) {}

    /// Open In needs a folder on this computer; the page has none.
    pub(crate) fn active_project_open_in_path(&self) -> Option<std::path::PathBuf> {
        None
    }
}
