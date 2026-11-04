//! Open, open-a-visit and close for the native Browser History window.
//! SEE-ALSO: apps/desktop/src/app/window/browser_history_modal.rs (the window),
//! apps/desktop/src/app/browser_history.rs (`show_browser_history_popup`, the Cmd+Y / toolbar entry),
//! apps/desktop/src/browser_history.rs (the visit store).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the window for the `browserHistory` open message: `paneId` is the Browser pane a
    /// chosen visit opens in, `runtimeKey` the Browser runtime that pane belonged to.
    pub(crate) fn open_gpui_browser_history_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pane_id) = message
            .get("paneId")
            .and_then(serde_json::Value::as_u64)
            .map(BrowserPaneId)
        else {
            return;
        };
        let runtime_key = message
            .get("runtimeKey")
            .and_then(serde_json::Value::as_u64);
        let current_project_id = self.browser_tabs_project_id.clone().unwrap_or_default();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_browser_history_modal_command(pane_id, runtime_key, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::BrowserHistory,
            APP_MODAL_HOST_COMMAND_PALETTE_WINDOW_WIDTH,
            APP_MODAL_HOST_BROWSER_HISTORY_WINDOW_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiBrowserHistoryModalWindow::new(host, current_project_id, window, cx)
                })
            },
            cx,
        );
    }

    fn handle_gpui_browser_history_modal_command(
        &mut self,
        pane_id: BrowserPaneId,
        runtime_key: Option<u64>,
        command: BrowserHistoryModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            BrowserHistoryModalCommand::Close => self.close_gpui_browser_history_modal(cx),
            BrowserHistoryModalCommand::Open { id } => {
                cx.spawn(async move |this, cx| {
                    let result = crate::browser_history::get(id).await;
                    let _ = this.update_in(cx, |app, window, cx| {
                        app.open_gpui_browser_history_visit(
                            pane_id,
                            runtime_key,
                            result,
                            window,
                            cx,
                        );
                    });
                })
                .detach();
            }
        }
    }

    /// Opens a looked-up visit in a new tab of the pane the window was opened from, closing the
    /// window; a visit that is gone, or a store error, is shown in the window instead.
    fn open_gpui_browser_history_visit(
        &mut self,
        pane_id: BrowserPaneId,
        runtime_key: Option<u64>,
        result: Result<Option<crate::browser_history::HistoryPage>, String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::BrowserHistory;
        if self.native_app_modal_kind() != Some(kind)
            || runtime_key != Some(self.browser_tabs_runtime_key)
        {
            return;
        }
        let error = match result {
            Ok(Some(page)) => {
                let Some(url) = sanitize_browser_tab_url_for_state(&page.url) else {
                    return;
                };
                if !self.browser_tabs.focus_pane(pane_id) {
                    return;
                }
                self.close_gpui_browser_history_modal(cx);
                let source_tab = self.browser_tabs.active_tab().map(|tab| tab.id);
                self.open_browser_popup_tab(
                    url,
                    page.remote_machine_id,
                    cef::BrowserPopupPlacement::Selected,
                    source_tab,
                    window,
                    cx,
                );
                return;
            }
            Ok(None) => "This history entry is no longer available.".to_string(),
            Err(error) => error,
        };
        self.update_native_app_modal::<GpuiBrowserHistoryModalWindow, ()>(
            kind,
            cx,
            move |view, _window, cx| view.show_error(error, cx),
        );
    }

    pub(crate) fn close_gpui_browser_history_modal(&mut self, cx: &mut gpui::Context<Self>) {
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::BrowserHistory) {
            return;
        }
        let return_focus_target = self.app_modal_command_return_focus_target;
        self.remove_native_app_modal_window(cx);
        self.app_modal_command_return_focus_target = return_focus_target;
        self.restore_keyboard_focus_after_app_modal(cx);
    }
}
