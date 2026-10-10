//! Open, focus-a-result, launch-a-result and close for the native Search by Prompt window.
//! SEE-ALSO: apps/desktop/src/app/window/find_prompts/ (the window),
//! apps/gpui-web/src/app/web_host/find_prompts.rs (the web build's twin of this file).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens Search by Prompt in the app appearance and theme colours, like the other native modals,
    /// with the Session Chat font the React page used.
    pub(crate) fn open_gpui_find_prompts_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
        let font_family = crate::app::window::find_prompts::palette::find_prompts_font_family(
            &gpui_session_chat_font_family_from_settings(settings.object()),
        );
        let glass = window_glass_active();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_find_prompts_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::FindPrompts,
            // The Settings frame (CDXC:AppModal 2026-10-01 in app/model/app_modal_kind.rs).
            APP_MODAL_HOST_SETTINGS_WINDOW_WIDTH,
            APP_MODAL_HOST_SETTINGS_WINDOW_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiFindPromptsModalWindow::new(host, light, glass, font_family, window, cx)
                })
            },
            cx,
        );
    }

    fn handle_gpui_find_prompts_modal_command(
        &mut self,
        command: FindPromptsModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            FindPromptsModalCommand::Close => self.close_gpui_find_prompts_modal(cx),
            FindPromptsModalCommand::FocusSession {
                project_id,
                session_id,
            } => {
                if project_id.is_empty() || session_id.is_empty() {
                    return;
                }
                self.close_gpui_find_prompts_modal(cx);
                // CDXC:PromptSearch 2026-09-10 WHY:
                // Direct native focus skipped the sidebar presentation update and project-switch coordination, so a result could attach into the outgoing workspace or leave its sidebar row hidden.
                // Use the modal session activation route, then the same reveal request as the titlebar button to expand and scroll the owning sidebar containers.
                let sidebar_session_id =
                    gpui_combined_presentation_session_id(&project_id, &session_id);
                if self.dispatch_gpui_command_palette_session_focus(&sidebar_session_id, cx) {
                    // In the window the focus went to (workspace_windows/session_routing.rs).
                    let row_id = sidebar_session_id.clone();
                    self.run_in_session_window(&sidebar_session_id, cx, move |app, cx| {
                        app.reveal_sidebar_session(&row_id, cx)
                    });
                } else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not open session",
                        "The sidebar is not ready. Try opening the search result again.",
                        cx,
                    );
                }
            }
            FindPromptsModalCommand::LaunchSession {
                command,
                cwd,
                title,
            } => {
                if command.is_empty() || cwd.is_empty() {
                    return;
                }
                self.close_gpui_find_prompts_modal(cx);
                self.dispatch_gpui_os_integration_command_message(
                    serde_json::json!({
                        "action": "createQuickTerminal",
                        "command": command,
                        "cwd": cwd,
                        "title": title,
                    }),
                    cx,
                );
            }
        }
    }

    pub(crate) fn close_gpui_find_prompts_modal(&mut self, cx: &mut gpui::Context<Self>) {
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::FindPrompts) {
            return;
        }
        let return_focus_target = self.app_modal_command_return_focus_target;
        self.remove_native_app_modal_window(cx);
        self.app_modal_command_return_focus_target = return_focus_target;
        self.restore_keyboard_focus_after_app_modal(cx);
    }
}
