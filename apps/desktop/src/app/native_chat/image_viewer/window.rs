//! The full-size chat image preview, as a native child window over the whole window the chat is
//! drawn in (`expanded_area`).
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! React's session-chat-image-viewer.tsx (deleted 2026-10-01) overlay: a picture opens centred at its original aspect
//! ratio, clicking it steps through zoom levels and back to the fitted size, Escape, a right-click, or a click on
//! the surround closes it, and Copy image / Copy path / Save image sit beside the close button.
//! A native child window owns its own frame and input, which is how every other chat overlay here
//! is built (save_markdown, the context editor).
//!
//! CDXC:SessionChat 2026-10-23 SEE-ALSO:
//! The floating page window's pictures no longer open here: the page's own script draws a viewer in the chat viewer's style inside the page (apps/desktop/src/app/window/visual_page_modal/page_script.js).

use super::super::state::NativeChatView;
use gpui::{
    AppContext as _, Context, Entity, FocusHandle, Styled as _, Subscription, WindowBounds,
    WindowOptions, px,
};
use gpui_component::Root;
use serde_json::Value;

/// The picture on screen, and the rest of its message's pictures for next/previous.
pub(in crate::app::native_chat) struct ImageViewerRequest {
    pub(in crate::app::native_chat) images: Vec<Value>,
    pub(super) index: usize,
    /// 0 is the fitted size; every step after it enlarges, and the last step returns to fitted.
    pub(super) zoom: usize,
    pub(super) completed: Option<&'static str>,
}

#[derive(Default)]
pub(in crate::app::native_chat) struct ImageViewerState {
    pub(in crate::app::native_chat) handle: Option<gpui::WindowHandle<Root>>,
    opening: bool,
    subscription: Option<Subscription>,
    pub(in crate::app::native_chat) request: Option<ImageViewerRequest>,
}

pub(super) struct ImageViewerWindow {
    pub(super) chat: Entity<NativeChatView>,
    pub(super) focus: FocusHandle,
    /// The picture measured last and its own pixel size, so a resize or a zoom step does not read
    /// the same header again. One entry: the viewer paints one picture at a time.
    pub(super) measured: Option<(u64, Option<gpui::Size<gpui::Pixels>>)>,
    pub(super) scroll: gpui::ScrollHandle,
    pub(super) press: Option<super::pan::PicturePress>,
    pub(super) zoom_anchor: Option<super::pan::ZoomAnchor>,
    /// The size the picture was painted at last, when it is known.
    pub(super) painted: Option<gpui::Size<gpui::Pixels>>,
    /// The side the pointer is near, whose step arrow shows: -1 left, 1 right, 0 neither.
    pub(super) edge: isize,
    /// The side whose arrow is fading out since the pointer left it.
    pub(super) fading: Option<isize>,
    /// Bumped whenever the pointer changes side, naming the arrows' fade animations so each change
    /// fades from the start.
    pub(super) arrow_generation: u64,
    /// The picture painted last, so stepping to another one starts it unscrolled.
    pub(super) shown_index: usize,
    _subscription: Subscription,
}

impl NativeChatView {
    pub(in crate::app::native_chat) fn open_image_viewer(
        &mut self,
        images: Vec<Value>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if images.is_empty() {
            return;
        }
        let index = index.min(images.len() - 1);
        let previous = self.image_viewer.request.replace(ImageViewerRequest {
            images,
            index,
            zoom: 0,
            completed: None,
        });
        self.sync_image_viewer_window(cx);
        if let Some(replaced) = previous {
            self.release_replaced_viewer_pictures(replaced, cx);
        }
        cx.notify();
    }

    /// Opens a picture the transcript shows on the whole transcript's pictures, so next and
    /// previous move through the chat (CDXC:SessionChat 2026-10-24 in gx-chat-core's
    /// transcript/pictures.rs). A picture only the subagent viewer shows steps through that one's.
    pub(in crate::app::native_chat) fn open_transcript_picture(
        &mut self,
        picture: &Value,
        cx: &mut Context<Self>,
    ) {
        use ghostex_gx_chat_core::transcript::pictures::picture_gallery;
        let mut gallery = picture_gallery(self.items.iter(), picture);
        if gallery.images.len() < 2 && !self.subagent_items.is_empty() {
            gallery = picture_gallery(self.subagent_items.iter(), picture);
        }
        self.open_image_viewer(gallery.images, gallery.index, cx);
    }

    pub(in crate::app::native_chat) fn close_image_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(closed) = self.image_viewer.request.take() {
            self.sync_image_viewer_window(cx);
            self.release_viewed_pictures(&closed.images, cx);
            cx.notify();
        }
    }

    /// The pictures of a preview another one replaced, except those the new one shows too.
    fn release_replaced_viewer_pictures(
        &mut self,
        replaced: ImageViewerRequest,
        cx: &mut Context<Self>,
    ) {
        let shown = self
            .image_viewer
            .request
            .as_ref()
            .map(|request| request.images.as_slice())
            .unwrap_or_default();
        let gone: Vec<Value> = replaced
            .images
            .into_iter()
            .filter(|image| !shown.contains(image))
            .collect();
        self.release_viewed_pictures(&gone, cx);
    }

    /// Next or previous picture in the list the viewer opened on; nothing past either end.
    pub(super) fn step_image_viewer(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(request) = &mut self.image_viewer.request else {
            return;
        };
        let next = request.index as isize + delta;
        if next < 0 || next >= request.images.len() as isize || next == request.index as isize {
            return;
        }
        request.index = next as usize;
        request.zoom = 0;
        request.completed = None;
        cx.notify();
    }

    /// Past the last step the next click returns the picture to its fitted size.
    pub(super) fn zoom_image_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = &mut self.image_viewer.request {
            request.zoom = if request.zoom >= super::render::ZOOM_LEVEL_COUNT {
                0
            } else {
                request.zoom + 1
            };
            cx.notify();
        }
    }

    pub(super) fn note_image_viewer_action(
        &mut self,
        completed: &'static str,
        cx: &mut Context<Self>,
    ) {
        if let Some(request) = &mut self.image_viewer.request {
            request.completed = Some(completed);
            cx.notify();
        }
    }

    pub(in crate::app::native_chat) fn sync_image_viewer_window(&mut self, cx: &mut Context<Self>) {
        if self.pane_hidden || self.image_viewer.request.is_none() {
            // The closed-window observer belongs to the window that is going away; keeping it would
            // leave a stale watcher able to close a viewer the reader opened next.
            self.image_viewer.subscription = None;
            if let Some(handle) = self.image_viewer.handle.take() {
                let main = self.main_window.filter(|_| !self.pane_hidden);
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    if let Some(main) = main {
                        let _ = main.update(cx, |_, window, cx| {
                            crate::app::window_activation::activate_window_if_frontmost(
                                window,
                                cx,
                                "image viewer closed",
                            )
                        });
                    }
                });
            }
            return;
        }
        if self.image_viewer.handle.is_some() || self.image_viewer.opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.image_viewer.opening = true;
        let area = self.expanded_area();
        let parent = self.child_window_parent(cx);
        let glass = crate::app::helpers::window_glass_active_for(self.main_window);
        let chat = cx.entity();
        cx.defer(move |cx| {
            let result = main
                .update(cx, |_, window, cx| {
                    let origin = window.bounds().origin + area.origin;
                    // Chat Lab's regular macOS titlebar is outside GPUI's content coordinates.
                    #[cfg(target_os = "macos")]
                    let origin = origin
                        + gpui::point(
                            px(0.0),
                            (window.bounds().size.height - window.viewport_size().height)
                                .max(px(0.0)),
                        );
                    (
                        gpui::Bounds::new(origin, area.size),
                        crate::app::window::popup_frame::PopupOwner::of(window, cx),
                    )
                })
                .and_then(|(bounds, owner)| {
                    let display_id = owner.display_for(bounds, cx);
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
                            window_background:
                                super::super::child_window::pane_dialog_window_background(glass),
                            ..Default::default()
                        },
                        {
                            let chat = chat.clone();
                            move |window, cx| {
                                crate::app::window::popup_frame::strip_gpui_popup_window_frame(
                                    window,
                                );
                                super::super::child_window::prepare_pane_dialog_window(
                                    window, glass,
                                );
                                crate::app::window::attach_gpui_app_modal_window_to_main_window(
                                    window, parent,
                                );
                                let view = cx.new(|cx| {
                                    let subscription = cx.observe(&chat, |_, _, cx| cx.notify());
                                    let focus = cx.focus_handle();
                                    super::super::focus::reclaim_keyboard_focus(window);
                                    focus.focus(window, cx);
                                    /*
                                    CDXC:SessionChat 2026-09-18 WHY:
                                    Opening from a click inside the chat pane leaves the pane's
                                    own window key on macOS, and a child window that is not key
                                    is still clicked through (GPUI accepts first mouse) while no
                                    key event ever reaches it. Taking key status here is what
                                    makes Escape and the arrow keys work in the picture itself.
                                    */
                                    window.activate_window();
                                    ImageViewerWindow {
                                        chat,
                                        focus,
                                        measured: None,
                                        scroll: gpui::ScrollHandle::new(),
                                        press: None,
                                        zoom_anchor: None,
                                        painted: None,
                                        edge: 0,
                                        fading: None,
                                        arrow_generation: 0,
                                        shown_index: 0,
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
                chat.image_viewer.opening = false;
                match result {
                    Ok(handle) => {
                        chat.image_viewer.handle = Some(handle);
                        let weak = cx.weak_entity();
                        chat.image_viewer.subscription =
                            Some(cx.on_window_closed(move |cx, id| {
                                let _ = weak.update(cx, |chat, cx| {
                                    if chat
                                        .image_viewer
                                        .handle
                                        .is_some_and(|handle| handle.window_id() == id)
                                    {
                                        chat.image_viewer.handle = None;
                                        chat.close_image_viewer(cx);
                                    }
                                });
                            }));
                        chat.sync_image_viewer_window(cx);
                    }
                    Err(error) => {
                        chat.error = Some(
                            crate::app::native_chat::child_window::window_open_failed(error),
                        );
                        chat.image_viewer.request = None;
                    }
                }
            });
        });
    }
}
