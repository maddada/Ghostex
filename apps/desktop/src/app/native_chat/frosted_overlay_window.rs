//! The small blurred window that carries the chat's rare floating control (the scroll-to-bottom
//! pill) while the main window is glass.
//!
//! CDXC:SessionChat 2026-09-23 WHY:
//! GPUI cannot blur behind an element inside a window, only behind a whole window, so each of these
//! controls is drawn in a small blurred child window of its own. The pane lays out an invisible
//! control of the same size where the in-window one would be and hands its bounds here; this module
//! opens, moves and closes the window on those bounds.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    WeakEntity, Window, WindowBounds, WindowOptions, div, point, px,
};
use gpui_component::{Root, tooltip::ManagedTooltipPlacement};
use std::{cell::Cell, rc::Rc, time::Duration};

/// Which floating control a window carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::native_chat) enum FrostedOverlay {
    ScrollBottom,
}

impl FrostedOverlay {
    const ALL: [FrostedOverlay; 1] = [FrostedOverlay::ScrollBottom];
}

/// The windows of every floating control, one each.
#[derive(Default)]
pub(in crate::app::native_chat) struct FrostedOverlayWindows {
    scroll_bottom: FrostedOverlayWindowState,
}

impl FrostedOverlayWindows {
    fn state(&mut self, overlay: FrostedOverlay) -> &mut FrostedOverlayWindowState {
        match overlay {
            FrostedOverlay::ScrollBottom => &mut self.scroll_bottom,
        }
    }

    /// The in-window placeholder's control, measured at its last paint (`None` while it is hidden).
    pub(in crate::app::native_chat) fn measured(
        &self,
        overlay: FrostedOverlay,
    ) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        match overlay {
            FrostedOverlay::ScrollBottom => self.scroll_bottom.measured.clone(),
        }
    }
}

#[derive(Default)]
struct FrostedOverlayWindowState {
    handle: Option<gpui::WindowHandle<Root>>,
    /// The open window's frame, in the chat window's content coordinates.
    frame: Option<Bounds<Pixels>>,
    /// The frame the control last asked for; the latest request wins while a window is opening.
    wanted: Option<Bounds<Pixels>>,
    busy: bool,
    measured: Rc<Cell<Option<Bounds<Pixels>>>>,
}

struct FrostedOverlayView {
    chat: WeakEntity<NativeChatView>,
    overlay: FrostedOverlay,
    /// Bumped on every hover change, so a delayed tooltip only shows for the hover that asked.
    tooltip_epoch: Rc<Cell<u64>>,
    _observe: Subscription,
    _release: Subscription,
}

impl NativeChatView {
    /// Closes every floating control's window and forgets their last measurements.
    pub(super) fn hide_frosted_overlays(&mut self, cx: &mut Context<Self>) {
        for overlay in FrostedOverlay::ALL {
            self.hide_frosted_overlay(overlay, cx);
        }
    }

    /// Closes `overlay`'s window and forgets its last measurement, so the next paint that shows the
    /// control opens it again.
    pub(super) fn hide_frosted_overlay(&mut self, overlay: FrostedOverlay, cx: &mut Context<Self>) {
        self.frosted_overlays.state(overlay).measured.set(None);
        self.place_frosted_overlay(overlay, None, cx);
    }

    /// Moves `overlay`'s window onto `control` (in the chat window's content coordinates), opening
    /// or closing it as needed.
    pub(super) fn place_frosted_overlay(
        &mut self,
        overlay: FrostedOverlay,
        control: Option<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let state = self.frosted_overlays.state(overlay);
        state.wanted = control.map(window_frame);
        if state.busy || state.frame == state.wanted {
            return;
        }
        state.busy = true;
        let chat = cx.entity();
        cx.defer(move |cx| apply_frosted_overlay(chat, overlay, cx));
    }

    /// Whether something else sits over the pane, so a control's own window (which would float
    /// above all of it) stays down: a chat popup window, the account-switch card or the subagent
    /// viewer.
    pub(super) fn frosted_overlay_covered(&self, _overlay: FrostedOverlay) -> bool {
        self.pane_hidden
            || self.pane_windows_open()
            || self.option_menu.is_some()
            || self.suggestions.is_open()
            || self.snapshot["accountSwitchCard"].is_object()
            || self.snapshot["subagent"].is_object()
    }

    /// The invisible reporter a placeholder carries: each paint hands the placeholder's bounds (or
    /// `None` while hidden or sticking out of the pane) to `overlay`'s window.
    pub(super) fn frosted_overlay_reporter(
        &self,
        overlay: FrostedOverlay,
        shown: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let measured = self.frosted_overlays.measured(overlay);
        let chat = cx.weak_entity();
        gpui::canvas(
            move |bounds, window, cx| {
                let control = (shown
                    && super::child_window::bounds_inside_content_mask(window, bounds))
                .then_some(bounds);
                if measured.replace(control) != control {
                    cx.defer(move |cx| {
                        let _ = chat.update(cx, |chat, cx| {
                            chat.place_frosted_overlay(overlay, control, cx);
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}

fn apply_frosted_overlay(
    chat: gpui::Entity<NativeChatView>,
    overlay: FrostedOverlay,
    cx: &mut gpui::App,
) {
    let (wanted, handle, parent, main) = chat.update(cx, |chat, cx| {
        let parent = chat.child_window_parent(cx);
        let state = chat.frosted_overlays.state(overlay);
        (
            state.wanted,
            state.handle.take(),
            parent,
            chat.main_window,
        )
    });
    // An open control follows its pane in place instead of being closed and reopened.
    if let (Some(frame), Some(handle)) = (wanted, handle)
        && super::child_window::move_child_window(handle.into(), parent, frame, cx)
    {
        finish(&chat, overlay, Some(handle), wanted, cx);
        return;
    }
    if let Some(handle) = handle {
        /*
        CDXC:SessionChat 2026-09-24 WHY:
        The handle was taken out of the chat's state above, so a window not moved in place has to
        be closed here: forgetting it left it on screen with nothing to move or close it, which is
        how hiding a pill (a space or session switch) or a failed move stacked extra "Scroll to
        bottom" pills in one chat.
        */
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    let placement = wanted.and_then(|frame| {
        main?
            .update(cx, |_, window, cx| {
                (
                    super::child_window::content_bounds(window).origin,
                    crate::app::window::popup_frame::PopupOwner::of(window, cx),
                )
            })
            .ok()
            .map(|(origin, owner)| (frame, origin, owner))
    });
    let Some((frame, origin, owner)) = placement else {
        finish(&chat, overlay, None, None, cx);
        return;
    };
    let radius = match overlay {
        FrostedOverlay::ScrollBottom => frame.size.height / 2.0,
    };
    let display_id = owner.display_for(Bounds::new(origin + frame.origin, frame.size), cx);
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                origin + frame.origin,
                frame.size,
            ))),
            display_id,
            titlebar: None,
            kind: gpui::WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            app_id: crate::gpui_platform_window_app_id(),
            icon: crate::gpui_platform_window_icon(),
            window_background: gpui::WindowBackgroundAppearance::Blurred,
            ..Default::default()
        },
        {
            let chat = chat.clone();
            move |window, cx| {
                window.set_background_corner_radius(radius);
                attach_overlay_window(window, parent);
                let view = cx.new(|cx| FrostedOverlayView {
                    chat: chat.downgrade(),
                    overlay,
                    tooltip_epoch: Rc::default(),
                    _observe: cx.observe(&chat, |_, _, cx| cx.notify()),
                    // A chat torn down with its control up takes the control with it.
                    _release: cx.observe_release_in(&chat, window, |_, _, window, _| {
                        window.remove_window();
                    }),
                });
                cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
            }
        },
    );
    match result {
        Ok(handle) => finish(&chat, overlay, Some(handle), Some(frame), cx),
        Err(error) => {
            chat.update(cx, |chat, _| chat.error = Some(error.to_string()));
            finish(&chat, overlay, None, None, cx);
        }
    }
}

/// Records what is on screen now and catches up with a request that arrived meanwhile.
fn finish(
    chat: &gpui::Entity<NativeChatView>,
    overlay: FrostedOverlay,
    handle: Option<gpui::WindowHandle<Root>>,
    frame: Option<Bounds<Pixels>>,
    cx: &mut gpui::App,
) {
    chat.update(cx, |chat, cx| {
        let state = chat.frosted_overlays.state(overlay);
        state.handle = handle;
        state.frame = frame;
        state.busy = false;
        if state.frame != state.wanted {
            state.busy = true;
            let chat = cx.entity();
            cx.defer(move |cx| apply_frosted_overlay(chat, overlay, cx));
        }
    });
}

impl Render for FrostedOverlayView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(chat) = self.chat.upgrade() else {
            return div().into_any_element();
        };
        let p = ChatAppearance::current(&chat.read(cx).snapshot).on_window_glass(true);
        let hover = if p.light {
            gpui::black().opacity(0.08)
        } else {
            gpui::white().opacity(0.11)
        };
        match self.overlay {
            FrostedOverlay::ScrollBottom => match chat.read(cx).pill_toast() {
                Some(toast) => pill_toast(toast, &p),
                None => {
                    scroll_bottom_pill(self.chat.clone(), &p, hover, self.tooltip_epoch.clone())
                }
            },
        }
    }
}

/// The Escape and "Agent was interrupted" toasts in the pill's window: its shape, no handlers.
fn pill_toast(toast: super::scroll_bottom::PillToast, p: &ChatAppearance) -> AnyElement {
    let (fill, border, text) = if toast.error {
        super::scroll_bottom::error_tone(p.light)
    } else {
        (p.composer_background, p.composer_border, p.primary)
    };
    div()
        .id("chat-interrupt-toast-window")
        .role(gpui::Role::Status)
        .aria_label(toast.text.clone())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(border)
        .bg(fill)
        .font_family(p.font.clone())
        .text_color(text)
        .text_size(px(super::scroll_bottom::font_size() * p.scale))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(toast.text)
        .into_any_element()
}

/// CDXC:SessionChat 2026-09-24 WHY:
/// The control's handlers hold the chat weakly. A strong handle kept in the window's drawn frame kept
/// a chat view alive after the app dropped it (a session's chat removed, evicted or rebuilt), so the
/// release hook that closes the control never ran and a stale "Scroll to bottom" pill stayed over the
/// pane, where a click scrolled the dropped view and did nothing visible.
fn scroll_bottom_pill(
    chat: WeakEntity<NativeChatView>,
    p: &ChatAppearance,
    hover: gpui::Hsla,
    tooltip_epoch: Rc<Cell<u64>>,
) -> AnyElement {
    let label = super::scroll_bottom::label().to_string();
    let shortcut = super::scroll_bottom::shortcut_label();
    let hover_chat = chat.clone();
    div()
        .id("chat-scroll-bottom-window")
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .chat_cursor_pointer()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(p.composer_border)
        .bg(p.composer_background)
        .hover(move |style| style.bg(hover))
        .font_family(p.font.clone())
        .text_color(p.primary)
        .text_size(px(super::scroll_bottom::font_size() * p.scale))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(label)
        .when_some(shortcut, move |pill, key| {
            pill.on_hover(move |hovered, window, cx| {
                let key = key.clone();
                overlay_hover_tooltip(
                    hover_chat.clone(),
                    FrostedOverlay::ScrollBottom,
                    *hovered,
                    &tooltip_epoch,
                    super::scroll_bottom::TOOLTIP_PLACEMENT,
                    Rc::new(move |window, cx| {
                        super::scroll_bottom::shortcut_tooltip(key.clone(), window, cx)
                    }),
                    window,
                    cx,
                );
            })
        })
        .on_click(move |_, _, cx| {
            let chat = chat.clone();
            cx.defer(move |cx| {
                if let Some(main) = chat.upgrade().and_then(|chat| chat.read(cx).main_window) {
                    let _ = main.update(cx, |_, window, cx| Root::hide_tooltip(window, cx));
                }
                let Ok(main) = chat.update(cx, |chat, cx| {
                    chat.jump_to_bottom(cx);
                    chat.main_window
                }) else {
                    return;
                };
                activate_main_window(main, cx);
            });
        })
        .into_any_element()
}

/// The control's window is too small to hold a tooltip, so its tooltip is shown in the chat's window
/// under the control's place there, after the managed tooltips' own delay, and hidden when the
/// pointer leaves.
#[allow(clippy::too_many_arguments)]
fn overlay_hover_tooltip(
    chat: WeakEntity<NativeChatView>,
    overlay: FrostedOverlay,
    hovered: bool,
    tooltip_epoch: &Rc<Cell<u64>>,
    placement: ManagedTooltipPlacement,
    build: Rc<dyn Fn(&mut Window, &mut gpui::App) -> gpui::AnyView>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let epoch = tooltip_epoch.get() + 1;
    tooltip_epoch.set(epoch);
    if !hovered {
        let main = chat.upgrade().and_then(|chat| chat.read(cx).main_window);
        if let Some(main) = main {
            let _ = main.update(cx, |_, window, cx| Root::hide_tooltip(window, cx));
        }
        return;
    }
    let tooltip_epoch = tooltip_epoch.clone();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = cx.update(|_, cx| {
                if tooltip_epoch.get() != epoch {
                    return;
                }
                let Some(view) = chat.upgrade() else {
                    return;
                };
                let (main, anchor) = {
                    let view = view.read(cx);
                    (
                        view.main_window,
                        view.frosted_overlays.measured(overlay).get(),
                    )
                };
                if let (Some(main), Some(anchor)) = (main, anchor) {
                    let _ = main.update(cx, |_, window, cx| {
                        Root::show_tooltip_for_bounds(
                            window,
                            cx,
                            anchor,
                            placement,
                            move |window, cx| build(window, cx),
                        )
                    });
                }
            });
        })
        .detach();
}

/// The control's window never takes the keyboard, so a click from another app brings the chat's
/// window forward the way clicking the pane would.
fn activate_main_window(main: Option<gpui::AnyWindowHandle>, cx: &mut gpui::App) {
    if let Some(main) = main {
        let _ = main.update(cx, |_, window, _| {
            if !window.is_window_active() {
                window.activate_window();
            }
        });
    }
}

/// AppKit places a window on whole points, so the control's window takes the whole points around
/// the measured control.
fn window_frame(control: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(control.left().floor(), control.top().floor()),
        point(control.right().ceil(), control.bottom().ceil()),
    )
}

/// The same child-window attachment the composer's suggestions use: a shadowless panel that stays
/// above the chat's window and never takes key status from it.
#[cfg(target_os = "macos")]
fn attach_overlay_window(window: &mut Window, parent: *mut std::ffi::c_void) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    unsafe extern "C" {
        fn GhostexGpuiAttachComposerSuggestionsWindow(
            view: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        );
    }
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
    {
        unsafe { GhostexGpuiAttachComposerSuggestionsWindow(handle.ns_view.as_ptr(), parent) };
    }
}

/// A click on the pill must leave the keyboard with the chat's window, as the frosted menu hosts do
/// (`frosted_host.rs`).
#[cfg(target_os = "windows")]
fn attach_overlay_window(window: &mut Window, parent: *mut std::ffi::c_void) {
    crate::app::window::make_gpui_popup_window_non_activating(window);
    crate::app::window::own_gpui_popup_window(window, parent);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn attach_overlay_window(_: &mut Window, _: *mut std::ffi::c_void) {}
