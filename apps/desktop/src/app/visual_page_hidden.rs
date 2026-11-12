//! Ghostex pages put aside: a page window hidden by a click outside it, its Hide button, a link
//! that opened in the app's browser, or another page opening (window/visual_page_modal.rs). It
//! keeps running with its window ordered out, and a pill in the pills bar above the chat box of
//! the chat whose card opened it (native_chat/minimized_pages.rs) brings it back or closes it for
//! good.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User's picks on the hide-page mockup (page-links/hide-page.html): one button per hidden page, shown only in the chat the page came from and only while it is hidden. Pointing at it shows an x that closes the page for good (option A), with the user's note: "but please make the x appear like the one we show on toasts (floating on top right of it, appears on hover, doesn't push the text etc)": the toasts' round x, smaller, floating over the button's corner on the side the toasts' x is on (`close_buttons_on_left`), never moving or shortening its text. A hidden page is kept running (option A), so its button brings it back at once, where it was, at its size, scrolled where it was, with every pick and note as it was. Closing a hidden page keeps its saved answers with the page, so its card opens it again with them. A page whose answers are sent closes itself, which takes its button away.
//!
//! CDXC:SessionChat 2026-11-12 DECISION:
//! User, of the hidden page's tab squeezed and cut off in the chat's header between the back and forward buttons and the agent's buttons: "when we minimize the page there's no space for the minimized page here at the top. please move them to the pills bar above the composer instead, make the pill have preset width not too wide and make sure it makes sense in terms turning to icon with qs answered only on narrow screen". The button is a pill in the pills bar above the chat box, no longer a tab in the header: a preset width with the page's icon, its title cut short and its answered count ("3/16"), dropping the title on a narrow chat and the count on the narrowest, where a dot says questions are unanswered (native_chat/minimized_pages.rs). Supersedes the header tab (option C) and its count-less dot (option C) of 2026-11-11.

use crate::app::native_app_modal_lifecycle::NativeAppModal;
use crate::app::native_chat::minimized_pages::MinimizedPage;
use crate::app::window::visual_page_modal::GpuiVisualPageModalWindow;
use crate::*;
use gpui::{Entity, WindowHandle};
use gpui_component::Root;

/// A hidden page window, kept running.
pub(crate) struct HiddenVisualPage {
    pub(crate) key: u64,
    window: WindowHandle<Root>,
    view: Entity<GpuiVisualPageModalWindow>,
}

/// The window's native view, for `ghostex_capture::platform`'s hide and show.
fn native_view(window: WindowHandle<Root>, cx: &mut gpui::App) -> Option<usize> {
    window
        .update(cx, |_, window, _| {
            crate::app::helpers::cef_parent_native_view(window)
                .ok()
                .map(|native| native as usize)
        })
        .ok()
        .flatten()
}

impl GhostexGpuiApp {
    /// The shown page window's key, when the open app modal is a page window.
    pub(crate) fn shown_visual_page_key(&self, cx: &gpui::App) -> Option<u64> {
        let modal = self.native_app_modal.as_ref()?;
        if modal.kind != GpuiAppModalKind::VisualPage {
            return None;
        }
        let view = modal
            .view
            .clone()
            .downcast::<GpuiVisualPageModalWindow>()
            .ok()?;
        Some(view.read(cx).key())
    }

    /// The shown page window's address.
    pub(crate) fn shown_visual_page_url(&self, cx: &gpui::App) -> Option<String> {
        let modal = self.native_app_modal.as_ref()?;
        let view = modal
            .view
            .clone()
            .downcast::<GpuiVisualPageModalWindow>()
            .ok()?;
        Some(view.read(cx).url().to_string())
    }

    /// Puts the shown page window aside, still running. False when no page window is shown.
    pub(crate) fn hide_visual_page_modal(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::VisualPage) {
            return false;
        }
        let Some(modal) = self.native_app_modal.take() else {
            return false;
        };
        let Ok(view) = modal.view.clone().downcast::<GpuiVisualPageModalWindow>() else {
            self.native_app_modal = Some(modal);
            return false;
        };
        let key = view.read(cx).key();
        view.update(cx, |view, cx| view.set_hidden(true, cx));
        if let Some(native) = native_view(modal.window, cx) {
            crate::app::ghostex_capture::platform::detach_from_parent(native);
            crate::app::ghostex_capture::platform::hide_window(native);
        }
        self.hidden_visual_pages.push(HiddenVisualPage {
            key,
            window: modal.window,
            view,
        });
        self.sync_app_modal_over_chat(cx);
        self.restore_keyboard_focus_after_app_modal(cx);
        self.notify_minimized_page_chats(cx);
        cx.notify();
        true
    }

    /// Brings a hidden page back where it was. Another page shown meanwhile is put aside in its
    /// place; any other app modal closes, as opening a modal over it would.
    pub(crate) fn show_hidden_visual_page(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        let Some(index) = self
            .hidden_visual_pages
            .iter()
            .position(|page| page.key == key)
        else {
            return;
        };
        let page = self.hidden_visual_pages.remove(index);
        self.notify_minimized_page_chats(cx);
        if !self.hide_visual_page_modal(cx) {
            self.remove_gpui_app_modal_window_without_focus_restore(cx);
            self.remove_native_app_modal_window(cx);
        }
        let Some(native) = native_view(page.window, cx) else {
            // Its window is gone.
            cx.notify();
            return;
        };
        page.view.update(cx, |view, cx| view.set_hidden(false, cx));
        crate::app::ghostex_capture::platform::show_window(native, true);
        // It was taken off the main window when hidden (`detach_from_parent`); it moves with the
        // main window again.
        let main_window_native_view = self.parent_ns_view;
        let _ = page.window.update(cx, |_, window, _| {
            crate::app::window::attach_gpui_app_modal_window_to_main_window(
                window,
                main_window_native_view,
            );
            window.activate_window();
        });
        self.native_app_modal = Some(NativeAppModal {
            kind: GpuiAppModalKind::VisualPage,
            window: page.window,
            view: page.view.into_any(),
        });
        self.sync_app_modal_over_chat(cx);
        cx.notify();
    }

    /// Closes a hidden page for good; what it saved stays with the page.
    pub(crate) fn close_hidden_visual_page(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        let Some(index) = self
            .hidden_visual_pages
            .iter()
            .position(|page| page.key == key)
        else {
            return;
        };
        let page = self.hidden_visual_pages.remove(index);
        let _ = page
            .window
            .update(cx, |_, window, _| window.remove_window());
        self.notify_minimized_page_chats(cx);
        cx.notify();
    }

    /// The hidden page showing `url`, to bring back instead of opening the page a second time.
    pub(crate) fn hidden_visual_page_showing(&self, url: &str, cx: &gpui::App) -> Option<u64> {
        self.hidden_visual_pages
            .iter()
            .find(|page| same_page(page.view.read(cx).url(), url))
            .map(|page| page.key)
    }

    /// The pills of the pages `session_id`'s card opened that are hidden, oldest first
    /// (native_chat/minimized_pages.rs reads them through its `visual_popup.rs`).
    pub(crate) fn minimized_page_pills(
        &self,
        session_id: TerminalSessionId,
        cx: &gpui::App,
    ) -> Vec<MinimizedPage> {
        self.hidden_visual_pages
            .iter()
            .filter_map(|page| {
                let view = page.view.read(cx);
                (view.source_session() == Some(session_id)).then(|| MinimizedPage {
                    key: page.key,
                    title: view.title().to_string(),
                    questions_left: view.questions_left(),
                    counts: view.question_counts(),
                })
            })
            .collect()
    }

    /// The chats draw their pills from the hidden pages, so they draw again when those change.
    pub(crate) fn notify_minimized_page_chats(&self, cx: &mut gpui::Context<Self>) {
        for view in self
            .native_chat_views
            .values()
            .chain(self.side_panel_chat_views())
        {
            view.update(cx, |_, cx| cx.notify());
        }
    }
}

/// Whether two page addresses name the same page (whatever their fragments say).
pub(crate) fn same_page(a: &str, b: &str) -> bool {
    a.split('#').next() == b.split('#').next()
}
