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

    /// Handoff / Export writes a file on this computer's disk and shows its result dialog; the page has neither.
    pub(crate) fn receive_gpui_export_transcript_result(
        &mut self,
        _result: &Value,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
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

    /// The desktop's status item and pet read the same settings; the page has neither.
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
        self.web_open_session_in_work_area(session, terminal, cx);
    }

    /// Which surface a focused session opens on: a plain terminal has no chat to show; a known agent keeps the surface the page is on unless the focus asks for its chat; a session whose row has not arrived yet (a create) opens where the focus prefers.
    pub(crate) fn web_surface_is_terminal(
        &self,
        session: &SessionKey,
        preferred: crate::app::model::GpuiPreferredAgentInterface,
    ) -> bool {
        use crate::app::model::GpuiPreferredAgentInterface;
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
    /// The page has no command pane to give focus back to.
    pub(crate) fn restore_gpui_app_modal_command_return_focus_if_needed(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    /// Open In needs a folder on this computer; the page has none.
    pub(crate) fn active_project_open_in_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// The desktop's pet lives in its own window; the page has no pet.
    pub(crate) fn sleep_gpui_pet_overlay_from_context_menu(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Not available in the browser",
            "The pet runs in the Ghostex app.",
            cx,
        );
    }
}
