//! Open, refresh and command plumbing for the dialog an extension modal opens in its place while
//! the optional web runtime is not installed or failed.
//! SEE-ALSO: apps/desktop/src/app/window/web_runtime_prompt_modal.rs (the dialog),
//! apps/desktop/src/app/cef_deferred_startup.rs (the held modal it opens once CEF runs),
//! apps/desktop/src/app/helpers/web_runtime.rs (CDXC:CefRuntime 2026-09-28).
use crate::app::helpers::web_runtime::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Shows the prompt dialog for `modal`, which is waiting for the web runtime.
    pub(crate) fn open_web_runtime_modal_prompt(
        &mut self,
        modal: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) {
        let content = self.web_runtime_modal_prompt_content(modal);
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_web_runtime_prompt_modal_command(command, cx);
        });
        self.open_native_app_modal(
            modal,
            WEB_RUNTIME_PROMPT_MODAL_WIDTH,
            WEB_RUNTIME_PROMPT_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiWebRuntimePromptModalWindow::new(content, palette, host, window, cx)
                })
            },
            cx,
        );
    }

    fn web_runtime_modal_prompt_content(
        &self,
        modal: GpuiAppModalKind,
    ) -> WebRuntimePromptModalContent {
        let name = match modal {
            GpuiAppModalKind::Extension(id) => self
                .extensions_snapshot
                .installed
                .get(id.as_str())
                .map(|extension| extension.title.clone())
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "This extension".to_string()),
            GpuiAppModalKind::VisualPage => "This page".to_string(),
            _ => "This window".to_string(),
        };
        match web_runtime_install_prompt(&name) {
            Some(prompt) => WebRuntimePromptModalContent {
                title: prompt
                    .title
                    .unwrap_or_else(|| "Web runtime not installed".to_string()),
                message: prompt.message,
                retry: prompt
                    .action
                    .map(|action| action == WebRuntimePromptAction::Retry),
            },
            None => WebRuntimePromptModalContent {
                title: format!("Opening {name}"),
                message: web_runtime_install_phase_label(
                    crate::component_store::ComponentStoreProgressPhase::Ready,
                )
                .to_string(),
                retry: None,
            },
        }
    }

    /// Keeps an open prompt dialog in step with the runtime's state.
    pub(crate) fn refresh_web_runtime_modal_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(kind) = self.gpui_app_modal_deferred_for_cef_kind() else {
            return;
        };
        let content = self.web_runtime_modal_prompt_content(kind);
        let _ = self.update_native_app_modal(
            kind,
            cx,
            |view: &mut GpuiWebRuntimePromptModalWindow, _window, cx| {
                view.set_content(content, cx);
            },
        );
    }

    fn handle_web_runtime_prompt_modal_command(
        &mut self,
        command: WebRuntimePromptModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            WebRuntimePromptModalCommand::Install => self.install_web_runtime(false, cx),
            WebRuntimePromptModalCommand::Retry => self.retry_web_runtime(cx),
            WebRuntimePromptModalCommand::Dismiss => {
                if let Some(kind) = self.gpui_app_modal_deferred_for_cef_kind() {
                    self.forget_gpui_app_modal_deferred_for_cef();
                    self.release_native_app_modal_window(kind, cx);
                }
            }
        }
    }
}
