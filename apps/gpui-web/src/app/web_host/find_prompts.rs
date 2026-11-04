//! Search by Prompt in the page: the desktop's window (`window/find_prompts/`), opened, focused and
//! closed the way this build can. The web twin of apps/desktop/src/app/find_prompts_modal_lifecycle.rs.
use gpui::{AppContext as _, Context};

use crate::GhostexGpuiApp;
use crate::app::consts::{
    APP_MODAL_HOST_SETTINGS_WINDOW_HEIGHT, APP_MODAL_HOST_SETTINGS_WINDOW_WIDTH,
};
use crate::app::model::GpuiAppModalKind;
use crate::app::window::find_prompts::palette::find_prompts_font_family;
use crate::app::window::{FindPromptsModalCommand, GpuiFindPromptsModalWindow};

impl GhostexGpuiApp {
    pub(crate) fn open_gpui_find_prompts_modal(&mut self, cx: &mut Context<Self>) {
        let light = crate::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
        let font_family = find_prompts_font_family();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.web_handle_find_prompts_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::FindPrompts,
            // The Settings frame (CDXC:AppModal 2026-10-01 in app/model/app_modal_kind.rs).
            APP_MODAL_HOST_SETTINGS_WINDOW_WIDTH,
            APP_MODAL_HOST_SETTINGS_WINDOW_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiFindPromptsModalWindow::new(host, light, false, font_family, window, cx)
                })
            },
            cx,
        );
    }

    fn web_handle_find_prompts_command(
        &mut self,
        command: FindPromptsModalCommand,
        cx: &mut Context<Self>,
    ) {
        match command {
            FindPromptsModalCommand::Close => {
                self.remove_native_app_modal_window(cx);
            }
            FindPromptsModalCommand::FocusSession {
                project_id,
                session_id,
            } => {
                if project_id.is_empty() || session_id.is_empty() {
                    return;
                }
                self.remove_native_app_modal_window(cx);
                let session = ghostex_gx_core::SessionKey {
                    machine: ghostex_gx_core::MachineId::Local,
                    project_id,
                    session_id,
                };
                let preferred = match self.show_terminal {
                    true => crate::app::model::GpuiPreferredAgentInterface::Terminal,
                    false => crate::app::model::GpuiPreferredAgentInterface::Chat,
                };
                let terminal = self.web_surface_is_terminal(&session, preferred);
                self.web_open_session_in_work_area(session, terminal, cx);
            }
            // A resume or fork that no session owns starts a new terminal, which the page cannot run yet.
            FindPromptsModalCommand::LaunchSession { .. } => {
                self.remove_native_app_modal_window(cx);
                self.dispatch_gpui_workspace_action_toast(
                    "info",
                    "Not available in the browser",
                    "Resume this prompt from the Ghostex app.",
                    cx,
                );
            }
        }
    }
}
