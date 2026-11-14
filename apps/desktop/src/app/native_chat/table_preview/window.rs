//! The larger table preview, as a native child window centred on the window the chat is drawn in.
//!
//! CDXC:SessionChat 2026-09-24 DECISION:
//! "Make the bigger preview window that we show use gpui and render with glass effect matching the sidebar of the app when glass is enabled (solid, matching the chat view bg when not enabled)." It replaces the React table modal the app modal host drew for the GPUI chat. Under glass the window shows the main window's glass picture (`sync_overlay_window_glass`) under the sidebar's own tint, so it reads as the sidebar does; without glass the card is the chat's solid background.

use super::super::state::NativeChatView;
use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, Pixels, Styled as _, Subscription,
    WindowBounds, WindowOptions, px,
};
use gpui_component::Root;

#[derive(Default)]
pub(in crate::app::native_chat) struct TablePreviewState {
    pub(in crate::app::native_chat) handle: Option<gpui::WindowHandle<Root>>,
    opening: bool,
    subscription: Option<Subscription>,
    /// The Markdown of the table on show; `None` while the preview is closed.
    pub(in crate::app::native_chat) source: Option<String>,
}

pub(super) struct TablePreviewWindow {
    pub(super) chat: Entity<NativeChatView>,
    pub(super) focus: FocusHandle,
    pub(super) scroll: gpui::ScrollHandle,
    _subscription: Subscription,
}

impl NativeChatView {
    pub(in crate::app::native_chat) fn open_table_preview(
        &mut self,
        source: String,
        cx: &mut Context<Self>,
    ) {
        self.table_preview.source = Some(source);
        self.sync_table_preview_window(cx);
        cx.notify();
    }

    pub(in crate::app::native_chat) fn close_table_preview(&mut self, cx: &mut Context<Self>) {
        if self.table_preview.source.take().is_some() {
            self.sync_table_preview_window(cx);
            cx.notify();
        }
    }

    pub(in crate::app::native_chat) fn sync_table_preview_window(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.pane_hidden || self.table_preview.source.is_none() {
            // The closed-window observer belongs to the window that is going away.
            self.table_preview.subscription = None;
            if let Some(handle) = self.table_preview.handle.take() {
                let main = self.main_window.filter(|_| !self.pane_hidden);
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    if let Some(main) = main {
                        let _ = main.update(cx, |_, window, _| window.activate_window());
                    }
                });
            }
            return;
        }
        if self.table_preview.handle.is_some() || self.table_preview.opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.table_preview.opening = true;
        let area = self.expanded_area();
        let parent = self.child_window_parent(cx);
        let chat = cx.entity();
        let scale = super::super::appearance::ChatAppearance::current(&self.snapshot).scale;
        let glass = crate::app::helpers::window_glass_active();
        cx.defer(move |cx| {
            let frame = table_preview_frame(area);
            let result = main
                .update(cx, |_, window, cx| {
                    let origin = super::super::child_window::content_bounds(window).origin;
                    let bounds = Bounds::new(origin + frame.origin, frame.size);
                    (
                        bounds,
                        crate::app::window::popup_frame::PopupOwner::of(window, cx)
                            .display_for(bounds, cx),
                    )
                })
                .and_then(|(bounds, display_id)| {
                    cx.open_window(
                        WindowOptions {
                            kind: crate::app::window::popup_frame::child_window_kind(),
                            window_decorations:
                                crate::app::window::popup_frame::child_window_decorations(),
                            #[cfg(target_os = "linux")]
                            x11_parent: Some(main),
                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                            display_id,
                            app_id: crate::gpui_platform_window_app_id(),
                            icon: crate::gpui_platform_window_icon(),
                            focus: true,
                            show: true,
                            is_resizable: false,
                            is_minimizable: false,
                            is_movable: false,
                            titlebar: None,
                            window_background: if glass {
                                gpui::WindowBackgroundAppearance::Blurred
                            } else {
                                gpui::WindowBackgroundAppearance::Transparent
                            },
                            ..Default::default()
                        },
                        {
                            let chat = chat.clone();
                            move |window, cx| {
                                window.set_background_corner_radius(px(12.0 * scale));
                                crate::app::window::popup_frame::strip_gpui_popup_window_frame(
                                    window,
                                );
                                crate::app::window::attach_gpui_app_modal_window_to_main_window(
                                    window, parent,
                                );
                                let view = cx.new(|cx| {
                                    let subscription = cx.observe(&chat, |_, _, cx| cx.notify());
                                    let focus = cx.focus_handle();
                                    super::super::focus::reclaim_keyboard_focus(window);
                                    focus.focus(window, cx);
                                    // Key status is what lets Escape reach the preview after a
                                    // click inside the pane opened it (image_viewer/window.rs).
                                    window.activate_window();
                                    TablePreviewWindow {
                                        chat,
                                        focus,
                                        scroll: Default::default(),
                                        _subscription: subscription,
                                    }
                                });
                                cx.new(|cx| {
                                    Root::new(view, window, cx)
                                        .bordered(false)
                                        .bg(gpui::transparent_black())
                                })
                            }
                        },
                    )
                });
            chat.update(cx, |chat, cx| {
                chat.table_preview.opening = false;
                match result {
                    Ok(handle) => {
                        chat.table_preview.handle = Some(handle);
                        let weak = cx.weak_entity();
                        chat.table_preview.subscription =
                            Some(cx.on_window_closed(move |cx, id| {
                                let _ = weak.update(cx, |chat, cx| {
                                    if chat
                                        .table_preview
                                        .handle
                                        .is_some_and(|handle| handle.window_id() == id)
                                    {
                                        chat.table_preview.handle = None;
                                        chat.close_table_preview(cx);
                                    }
                                });
                            }));
                        chat.sync_table_preview_window(cx);
                    }
                    Err(error) => {
                        chat.error = Some(
                            crate::app::native_chat::child_window::window_open_failed(error),
                        );
                        chat.table_preview.source = None;
                    }
                }
            });
        });
    }
}

/// The share of the app window's width and height the floating previews open at: this table
/// preview, and the page window and the diagram and table viewers
/// (`chat_preview_modal_size` in app/markdown_table_modal_lifecycle.rs, CDXC:SessionChat 2026-10-22
/// there).
pub(crate) const CHAT_PREVIEW_WIDTH: f32 = 0.8;
pub(crate) const CHAT_PREVIEW_HEIGHT: f32 = 0.95;

/// The preview card, centred in `area` (the window the chat is drawn in, `expanded_area`) at the
/// floating previews' share of it. This replaced React's 1248px table modal size.
pub(in crate::app::native_chat) fn table_preview_frame(area: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = area.size.width * CHAT_PREVIEW_WIDTH;
    let height = area.size.height * CHAT_PREVIEW_HEIGHT;
    Bounds::new(
        area.origin
            + gpui::point(
                (area.size.width - width) / 2.0,
                (area.size.height - height) / 2.0,
            ),
        gpui::size(width, height),
    )
}
