//! Open, close and link plumbing for the native Markdown table popup.
//! SEE-ALSO: apps/desktop/src/app/window/markdown_table_modal.rs (the window entity), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::native_chat::table_preview::{CHAT_PREVIEW_HEIGHT, CHAT_PREVIEW_WIDTH};
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the popup for the `markdownTable` open message the React Markdown renderer posts
    /// (its `source` is the table's Markdown). An empty source opens nothing, as React's host did.
    pub(crate) fn open_gpui_markdown_table_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(source) = message
            .get("source")
            .and_then(serde_json::Value::as_str)
            .filter(|source| !source.trim().is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_markdown_table_modal_command(command, cx);
        });
        let kind = GpuiAppModalKind::MarkdownTable;
        let (width, height) = self.chat_preview_modal_size(cx);
        self.open_native_app_modal(
            kind,
            width,
            height,
            move |window, cx| {
                cx.new(|cx| GpuiMarkdownTableModalWindow::new(source, palette, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_markdown_table_modal_command(
        &mut self,
        command: MarkdownTableModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::MarkdownTable) {
            return;
        }
        self.close_native_app_modal_from_bridge(cx);
        let MarkdownTableModalCommand::OpenLink { href, external } = command else {
            return;
        };
        // A web link opens where the app opens every web link (embedded browser or the system
        // browser, Shift for the system browser); anything else in a README table is not a place.
        let is_web = gpui::http_client::Url::parse(href.trim())
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https"));
        if !is_web {
            return;
        }
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update_in(cx, |app, window, cx| {
                app.open_session_chat_link(href.trim(), None, external, false, window, cx);
            });
        });
    }

    /// The main window as the owner of the app's modals, menus and toasts: its frame and display
    /// as of its last draw.
    pub(crate) fn main_window_popup_owner(&self) -> crate::app::window::popup_frame::PopupOwner {
        crate::app::window::popup_frame::PopupOwner::new(
            self.main_window_bounds,
            self.main_window_display_id,
        )
    }

    /// A popup's size, kept on the main window's screen: the window is centred on the main window,
    /// so it may use twice the room between that centre and the nearer screen edge, less a margin.
    pub(crate) fn gpui_native_modal_size_on_screen(
        &self,
        size: gpui::Size<gpui::Pixels>,
        cx: &gpui::App,
    ) -> (f32, f32) {
        let (width, height) = (f32::from(size.width), f32::from(size.height));
        let center = self.main_window_bounds.center();
        // The display `open_native_app_modal` opens the window on.
        let Some(visible) = self
            .main_window_popup_owner()
            .visible_frame(gpui::Bounds::centered_at(center, size), cx)
        else {
            return (width, height);
        };
        let room = |near: f32, far: f32| (near.min(far) - MODAL_SCROLL_FIT_SCREEN_MARGIN) * 2.0;
        let room_x = room(
            f32::from(center.x - visible.left()),
            f32::from(visible.right() - center.x),
        );
        let room_y = room(
            f32::from(center.y - visible.top()),
            f32::from(visible.bottom() - center.y),
        );
        (
            if room_x > 0.0 {
                width.min(room_x)
            } else {
                width
            },
            if room_y > 0.0 {
                height.min(room_y)
            } else {
                height
            },
        )
    }

    /// An app modal's opening frame, `frame` (centred on the main window), shrunk and moved as
    /// little as needed to lie inside the visible part of its display, a margin from each edge.
    ///
    /// CDXC:AppModal 2026-11-11 WHY:
    /// On Windows "Connect a Phone" could not be used (Discord report, 11.3.0, Windows 11): the window opened 780px tall, centred on the main window, so on a 1280x720 screen (1080p at 150%) its top sat above the screen. Its title was out of reach, and the corner close button's window, centred on that corner, had its centre on no display, which GPUI on Windows answers by opening the window at the display's default size: one big round X over the whole dialog. Large panels were already shrunk to fit (`gpui_native_modal_size_on_screen`); every other modal opened at its full size wherever the main window's centre put it.
    pub(crate) fn gpui_native_modal_frame_on_screen(
        &self,
        frame: gpui::Bounds<gpui::Pixels>,
        cx: &gpui::App,
    ) -> gpui::Bounds<gpui::Pixels> {
        let Some(visible) = self.main_window_popup_owner().visible_frame(frame, cx) else {
            return frame;
        };
        let margin = MODAL_SCROLL_FIT_SCREEN_MARGIN;
        let (left, top) = (
            f32::from(visible.left()) + margin,
            f32::from(visible.top()) + margin,
        );
        let (right, bottom) = (
            f32::from(visible.right()) - margin,
            f32::from(visible.bottom()) - margin,
        );
        if right <= left || bottom <= top {
            return frame;
        }
        let width = f32::from(frame.size.width).min(right - left);
        let height = f32::from(frame.size.height).min(bottom - top);
        let x = f32::from(frame.origin.x).clamp(left, right - width);
        let y = f32::from(frame.origin.y).clamp(top, bottom - height);
        gpui::Bounds::new(
            gpui::point(gpui::px(x), gpui::px(y)),
            gpui::size(gpui::px(width), gpui::px(height)),
        )
    }

    /// The size the floating previews open at: the page window (`ghostex show`), the Mermaid
    /// diagram viewer and the Markdown table viewer, kept on the main window's screen.
    ///
    /// CDXC:SessionChat 2026-10-22 DECISION:
    /// User: the floating page window "and similar pop ups" open at 95% of the app window's height and 80% of its width. The similar ones are the larger previews of chat content: the Mermaid diagram viewer and the Markdown table viewer (app modals) and the chat's own table preview (native_chat/table_preview/window.rs). The image viewer already covers the whole window and is unchanged. This supersedes the fixed 1248 x 912 (diagram and table) and 1120 x 820 (page) opening sizes.
    pub(crate) fn chat_preview_modal_size(&self, cx: &gpui::App) -> (f32, f32) {
        let main = self.main_window_bounds.size;
        self.gpui_native_modal_size_on_screen(
            gpui::size(
                main.width * CHAT_PREVIEW_WIDTH,
                main.height * CHAT_PREVIEW_HEIGHT,
            ),
            cx,
        )
    }
}
