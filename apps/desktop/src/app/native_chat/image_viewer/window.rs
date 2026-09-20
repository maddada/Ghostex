//! The full-size chat image preview, as a pane-sized native child window.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! React's session-chat-image-viewer.tsx overlay: a picture opens centred at its original aspect
//! ratio, clicking it steps through zoom levels and back to the fitted size, Escape, a right-click, or a click on
//! the surround closes it, and Copy image / Copy path / Save image sit beside the close button.
//! A native child window owns its own frame and input, which is how every other chat overlay here
//! is built (save_markdown, model_picker).

use super::super::state::NativeChatView;
use gpui::{
    AppContext as _, Context, Entity, FocusHandle, Styled as _, Subscription, WindowBounds,
    WindowOptions, px,
};
use gpui_component::Root;
use serde_json::Value;

/// The picture on screen, and the rest of its message's pictures for next/previous.
pub(in crate::app::native_chat) struct ImageViewerRequest {
    pub(super) images: Vec<Value>,
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
        self.image_viewer.request = Some(ImageViewerRequest {
            images,
            index,
            zoom: 0,
            completed: None,
        });
        self.sync_image_viewer_window(cx);
        cx.notify();
    }

    pub(in crate::app::native_chat) fn close_image_viewer(&mut self, cx: &mut Context<Self>) {
        if self.image_viewer.request.take().is_some() {
            self.sync_image_viewer_window(cx);
            cx.notify();
        }
    }

    /// Next or previous picture across the message this one came from.
    pub(super) fn step_image_viewer(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(request) = &mut self.image_viewer.request else {
            return;
        };
        let count = request.images.len();
        if count < 2 {
            return;
        }
        let next = (request.index as isize + delta).rem_euclid(count as isize) as usize;
        request.index = next;
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
        if self.image_viewer.request.is_none() {
            // The closed-window observer belongs to the window that is going away; keeping it would
            // leave a stale watcher able to close a viewer the reader opened next.
            self.image_viewer.subscription = None;
            if let Some(handle) = self.image_viewer.handle.take() {
                let main = self.main_window;
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    if let Some(main) = main {
                        let _ = main.update(cx, |_, window, _| window.activate_window());
                    }
                });
            }
            return;
        }
        if self.image_viewer.handle.is_some() || self.image_viewer.opening {
            return;
        }
        let Some(main) = self.main_window else {
            return;
        };
        self.image_viewer.opening = true;
        let pane = self.bounds.get();
        let parent = self.config.parent_native_view;
        let chat = cx.entity();
        cx.defer(move |cx| {
            let result = main
                .update(cx, |_, window, cx| {
                    let origin = window.bounds().origin + pane.origin;
                    // Chat Lab's regular macOS titlebar is outside GPUI's content coordinates.
                    #[cfg(target_os = "macos")]
                    let origin = origin
                        + gpui::point(
                            px(0.0),
                            (window.bounds().size.height - window.viewport_size().height)
                                .max(px(0.0)),
                        );
                    (
                        gpui::Bounds::new(origin, pane.size),
                        window.display(cx).map(|display| display.id()),
                    )
                })
                .and_then(|(bounds, display_id)| {
                    cx.open_window(
                        WindowOptions {
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
                            window_background: gpui::WindowBackgroundAppearance::Transparent,
                            ..Default::default()
                        },
                        {
                            let chat = chat.clone();
                            move |window, cx| {
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
                                        _subscription: subscription,
                                    }
                                });
                                cx.new(|cx| {
                                    Root::new(view, window, cx).bg(gpui::transparent_black())
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
                        chat.error = Some(error.to_string());
                        chat.image_viewer.request = None;
                    }
                }
            });
        });
    }
}
