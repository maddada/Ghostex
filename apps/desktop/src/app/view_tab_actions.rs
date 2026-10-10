//! Popping a view out of the window. Reordering and pinning live in `view_strip_order.rs`.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User (screen 09): "Pop out (next to expand) moves the view to its own window, for a second
    /// monitor." Every view Ghostex can pop out is a page served over local HTTP (code-server for
    /// Code, gxserver for Kanban, Automate and Docs, the extension's own server, the tab's own
    /// address for Browser), so popping out hands that exact page to a window of its own instead of
    /// building a second CEF host inside the app. A view with no page yet cannot be popped out, and
    /// the control says so rather than opening an empty window.
    /// CDXC:Docs 2026-10-01 DECISION:
    /// User chose option 3A: Files is drawn natively and has no page of its own, so its Open Externally pops out the open HTML, Markdown or Excalidraw file through gxserver's file link (`native_docs/open_externally.rs`) instead of staying disabled; its embed page is never handed out, because it only works inside Ghostex.
    /// CDXC:ProjectBoard 2026-10-10 WHY:
    /// Kanban has no page to hand out (its React page is gone, CDXC:ProjectBoard 2026-09-30 in
    /// native_kanban/render.rs), so its pop-out opens the native board in a window of Ghostex's own
    /// instead (native_kanban/window.rs), and the same control brings it back while it is out.
    pub(crate) fn view_can_pop_out(&self, mode: TitlebarMode) -> bool {
        match mode {
            TitlebarMode::Manage => self.native_docs_external_file().is_some(),
            TitlebarMode::Kanban => self.native_kanban_can_open_window(),
            _ => self.view_pop_out_url(mode).is_some(),
        }
    }

    /// What the pop-out control is called for `mode`: Kanban moves between the panel and a window
    /// of Ghostex's own, the rest leave for the browser or another app.
    pub(crate) fn view_pop_out_label(&self, mode: TitlebarMode, menu: bool) -> &'static str {
        match (mode, self.native_kanban_detached(), menu) {
            (TitlebarMode::Kanban, true, false) => "Bring Back to Panel",
            (TitlebarMode::Kanban, true, true) => "Bring back to panel",
            (TitlebarMode::Kanban, false, false) => "Open in New Window",
            (TitlebarMode::Kanban, false, true) => "Open in new window",
            (_, _, false) => "Open Externally",
            (_, _, true) => "Open externally",
        }
    }

    fn view_pop_out_url(&self, mode: TitlebarMode) -> Option<String> {
        if mode == TitlebarMode::Manage {
            return None;
        }
        if mode == TitlebarMode::Browser {
            return self
                .browser_tabs
                .active_tab()
                .map(|tab| tab.url.clone())
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"));
        }
        let slot = ProjectWorkareaCefSurfaceSlotKey::for_titlebar_mode(mode)?;
        self.project_workarea_runtime_url_for_slot(slot)
            .map(|runtime_url| runtime_url.value.clone())
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
    }

    pub(crate) fn pop_out_view(&mut self, mode: TitlebarMode, cx: &mut gpui::Context<Self>) {
        if mode == TitlebarMode::Manage {
            if let Some(path) = self.native_docs_external_file() {
                self.native_docs_open_externally(&path, None, cx);
            }
            return;
        }
        if mode == TitlebarMode::Kanban {
            if self.native_kanban_detached() {
                self.native_kanban_bring_back(cx);
            } else {
                self.native_kanban_open_window(cx);
            }
            return;
        }
        let Some(url) = self.view_pop_out_url(mode) else {
            return;
        };
        if let Err(message) = gpui_open_external_http_url(&url) {
            self.upsert_gpui_app_toast(
                GpuiAppToast {
                    copy_text: None,
                    id: "gpui-view-pop-out-failed".to_string(),
                    level: GpuiAppToastLevel::from_raw(Some("warning")),
                    title: format!("Could not pop out {}", mode.tab_label()),
                    description: Some(message),
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    epoch: 0,
                },
                cx,
            );
        }
    }
}
