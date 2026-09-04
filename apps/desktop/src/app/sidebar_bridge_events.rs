use gpui::Window;

use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn receive_sidebar_bridge_event(
        &mut self,
        event: cef::SidebarBridgeEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Navigation 2026-07-29:
        Backstop for the coalescer: a project-scoped sidebar command must never
        overtake a project switch that is still queued behind the settle
        window, or it would act on the outgoing project's runtime. Land the
        trailing switch first. Project-agnostic and high-frequency telemetry
        events pass through so they cannot defeat the debounce.
        */
        if !self.project_switch_pending_requests.is_empty()
            && gpui_sidebar_bridge_event_must_follow_pending_project_switch(&event)
        {
            self.flush_coalesced_project_switch_requests(window, cx);
        }
        match event {
            cef::SidebarBridgeEvent::ActiveProjectContext(payload) => {
                self.receive_sidebar_project_context_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::GxserverPresentationFocusState(payload) => {
                self.receive_sidebar_gxserver_presentation_focus_state_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::CreateProjectAgent(payload) => {
                self.receive_sidebar_create_project_agent_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::CreateProjectTerminal(payload) => {
                self.receive_sidebar_create_project_terminal_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalFocus(payload) => {
                self.receive_sidebar_workspace_terminal_focus_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalRenameCommand(payload) => {
                self.receive_sidebar_workspace_terminal_rename_command_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalEnter(payload) => {
                self.receive_sidebar_workspace_terminal_enter_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalLifecycleResult(payload) => {
                self.receive_sidebar_workspace_terminal_lifecycle_result_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::SourceWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::BrowserWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ProjectWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ManageFileWorkareaOperationRequest(_) => {
                /*
                CDXC:Workarea 2026-06-29-00:02:
                Legacy sidebar readiness/proof messages stay accepted as compatibility no-ops. Source, Kanban, Automate, and Manage mounting now follows only the current runtime URL gate plus owned CEF surface map, and first-party Kanban/Automate/Manage CEF requests still flow through the separate project-workarea bridge.
                */
            }
            cef::SidebarBridgeEvent::NativeProjectPathAction(payload) => {
                self.receive_sidebar_native_project_path_action_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::NativeAppShotPrompt(payload) => {
                self.receive_sidebar_native_app_shot_prompt_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::ResourcesSnapshotRequest(payload) => {
                self.receive_sidebar_resources_snapshot_request_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::SidebarCommandAction(payload) => {
                self.receive_sidebar_command_action_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::SidebarCommandRunEnd(payload) => {
                self.receive_sidebar_command_run_end_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::GhostexHotkeyAction(payload) => {
                self.receive_sidebar_ghostex_hotkey_action_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::SessionCompletionSound(payload) => {
                self.receive_sidebar_session_completion_sound_payload(&payload);
            }
            cef::SidebarBridgeEvent::SessionStatusIndicators(payload) => {
                self.receive_sidebar_session_status_indicators_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::PetOverlayState(payload) => {
                self.receive_sidebar_pet_overlay_state_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::GlobalActions(payload) => {
                self.receive_sidebar_global_actions_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::TitlebarGitMenuState(payload) => {
                self.receive_sidebar_titlebar_git_menu_state_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::OpenBrowserUrl(payload) => {
                self.receive_sidebar_open_browser_url_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::BrowserTabFocus(payload) => {
                self.receive_sidebar_browser_tab_focus_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::ProjectBoardConversationResponse(payload) => {
                self.receive_sidebar_project_board_conversation_response_payload(&payload, cx);
            }
        }
    }
}
