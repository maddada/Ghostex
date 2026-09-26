//! The small blurred windows that carry the chat's rare floating controls (the scroll-to-bottom
//! pill and the fork switcher) while the main window is glass.
//!
//! CDXC:SessionChat 2026-09-23 WHY:
//! GPUI cannot blur behind an element inside a window, only behind a whole window, so each of these
//! controls is drawn in a small blurred child window of its own. The pane lays out an invisible
//! control of the same size where the in-window one would be and hands its bounds here; this module
//! opens, moves and closes the window on those bounds.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::hotkeys::gpui_configured_hotkey_label;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    WeakEntity, Window, WindowBounds, WindowOptions, div, point, px, svg,
};
use gpui_component::Root;
use std::{cell::Cell, rc::Rc, time::Duration};

/// Which floating control a window carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::native_chat) enum FrostedOverlay {
    ScrollBottom,
    ForkBranches,
}

impl FrostedOverlay {
    const ALL: [FrostedOverlay; 2] = [FrostedOverlay::ScrollBottom, FrostedOverlay::ForkBranches];
}

/// The windows of every floating control, one each.
#[derive(Default)]
pub(in crate::app::native_chat) struct FrostedOverlayWindows {
    scroll_bottom: FrostedOverlayWindowState,
    fork_branches: FrostedOverlayWindowState,
}

impl FrostedOverlayWindows {
    fn state(&mut self, overlay: FrostedOverlay) -> &mut FrostedOverlayWindowState {
        match overlay {
            FrostedOverlay::ScrollBottom => &mut self.scroll_bottom,
            FrostedOverlay::ForkBranches => &mut self.fork_branches,
        }
    }

    /// The in-window placeholder's control, measured at its last paint (`None` while it is hidden).
    pub(in crate::app::native_chat) fn measured(
        &self,
        overlay: FrostedOverlay,
    ) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        match overlay {
            FrostedOverlay::ScrollBottom => self.scroll_bottom.measured.clone(),
            FrostedOverlay::ForkBranches => self.fork_branches.measured.clone(),
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
    /// Whether the pointer is over the control.
    hovered: Rc<Cell<bool>>,
    /// The window alpha last handed to `set_overlay_window_alpha`.
    alpha: Cell<f32>,
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
    /// viewer. The fork switcher's own menu opens below it and does not count.
    pub(super) fn frosted_overlay_covered(&self, overlay: FrostedOverlay) -> bool {
        let own_menu = overlay == FrostedOverlay::ForkBranches
            && self.chat_menu_is_open(super::fork_branches::FORK_BRANCHES_TRIGGER);
        self.pane_hidden
            || self.pane_windows_open()
            || (self.option_menu.is_some() && !own_menu)
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
                let control = (shown && window.content_mask().bounds.intersect(&bounds) == bounds)
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
    let (wanted, handle, parent, main, scale) = chat.update(cx, |chat, cx| {
        let scale = ChatAppearance::current(&chat.snapshot).scale;
        let parent = chat.child_window_parent(cx);
        let state = chat.frosted_overlays.state(overlay);
        (
            state.wanted,
            state.handle.take(),
            parent,
            chat.main_window,
            scale,
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
        // The fork switcher's tooltip is drawn in the chat's window; it goes with its control.
        if overlay == FrostedOverlay::ForkBranches
            && let Some(main) = main
        {
            let _ = main.update(cx, |_, window, cx| Root::hide_tooltip(window, cx));
        }
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
                    window.display(cx).map(|display| display.id()),
                )
            })
            .ok()
            .map(|(origin, display_id)| (frame, origin, display_id))
    });
    let Some((frame, origin, display_id)) = placement else {
        finish(&chat, overlay, None, None, cx);
        return;
    };
    let radius = match overlay {
        FrostedOverlay::ScrollBottom => frame.size.height / 2.0,
        FrostedOverlay::ForkBranches => px(super::fork_branches::BADGE_RADIUS * scale),
    };
    let display_id = crate::app::window::popup_frame::display_at(
        Bounds::new(origin + frame.origin, frame.size).center(),
        cx,
    )
    .or(display_id);
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
                // The fork switcher's window opens at rest, so it never shows a full-strength frame first.
                let alpha = match overlay {
                    FrostedOverlay::ScrollBottom => 1.0,
                    FrostedOverlay::ForkBranches => super::fork_branches::RESTING_OPACITY,
                };
                set_overlay_window_alpha(window, alpha);
                let view = cx.new(|cx| FrostedOverlayView {
                    chat: chat.downgrade(),
                    overlay,
                    tooltip_epoch: Rc::default(),
                    hovered: Rc::default(),
                    alpha: Cell::new(alpha),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            FrostedOverlay::ScrollBottom => scroll_bottom_pill(self.chat.clone(), &p, hover),
            FrostedOverlay::ForkBranches => {
                let lifted = self.hovered.get()
                    || chat
                        .read(cx)
                        .chat_menu_is_open(super::fork_branches::FORK_BRANCHES_TRIGGER);
                let opacity = if lifted {
                    1.0
                } else {
                    super::fork_branches::RESTING_OPACITY
                };
                if self.alpha.replace(opacity) != opacity {
                    set_overlay_window_alpha(window, opacity);
                }
                fork_branches_badge(
                    chat,
                    &p,
                    hover,
                    opacity,
                    self.tooltip_epoch.clone(),
                    self.hovered.clone(),
                    cx,
                )
            }
        }
    }
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
) -> AnyElement {
    let label = gpui_configured_hotkey_label("scrollChatToBottom")
        .filter(|label| !label.is_empty())
        .map_or_else(
            || super::scroll_bottom::label().to_string(),
            |key| format!("{} ({key})", super::scroll_bottom::label()),
        );
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
        .on_click(move |_, _, cx| {
            let chat = chat.clone();
            cx.defer(move |cx| {
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

/// The fork switcher's frosted twin of `render_fork_branch_badge`: the same icon and count, and a
/// click that opens the same menu under the badge's place in the pane. `opacity` is the switcher's
/// current strength, drawn by the badge itself only where its window cannot fade as a whole.
fn fork_branches_badge(
    chat: gpui::Entity<NativeChatView>,
    p: &ChatAppearance,
    hover: gpui::Hsla,
    opacity: f32,
    tooltip_epoch: Rc<Cell<u64>>,
    hovered_state: Rc<Cell<bool>>,
    cx: &mut Context<FrostedOverlayView>,
) -> AnyElement {
    let (count, tooltip, menu_open) = {
        let view = chat.read(cx);
        let branches = &view.snapshot["forkBranches"];
        (
            branches["count"].as_u64().unwrap_or_default(),
            branches["tooltip"].as_str().unwrap_or_default().to_owned(),
            view.chat_menu_is_open(super::fork_branches::FORK_BRANCHES_TRIGGER),
        )
    };
    let s = p.scale;
    let hover_chat = chat.downgrade();
    let click_chat = chat.downgrade();
    div()
        .id("chat-fork-branches-window")
        .role(gpui::Role::Button)
        .aria_label(tooltip.clone())
        .chat_cursor_pointer()
        .size_full()
        .px(px(6.0 * s))
        .flex()
        .items_center()
        .gap(px(4.0 * s))
        .rounded(px(super::fork_branches::BADGE_RADIUS * s))
        .border_1()
        .border_color(p.composer_border)
        .bg(if menu_open {
            hover
        } else {
            p.composer_background
        })
        .hover(move |style| style.bg(hover))
        .when(!WINDOW_ALPHA, |this| this.opacity(opacity))
        .font_family(p.font.clone())
        .text_size(px(11.0 * s))
        .text_color(p.muted)
        .whitespace_nowrap()
        .child(
            svg()
                .path("titlebar/git-branch.svg")
                .size(px(14.0 * s))
                .flex_shrink_0()
                .text_color(p.muted),
        )
        .child(count.to_string())
        // The window is too small to hold the tooltip, so it is shown in the chat's window under
        // the badge's place there, after the managed tooltips' own delay.
        .on_hover(move |hovered, window, cx| {
            hovered_state.set(*hovered);
            window.refresh();
            let epoch = tooltip_epoch.get() + 1;
            tooltip_epoch.set(epoch);
            let chat = hover_chat.clone();
            if !*hovered {
                let main = chat.upgrade().and_then(|chat| chat.read(cx).main_window);
                if let Some(main) = main {
                    let _ = main.update(cx, |_, window, cx| Root::hide_tooltip(window, cx));
                }
                return;
            }
            let tooltip_epoch = tooltip_epoch.clone();
            let tooltip = tooltip.clone();
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
                                view.frosted_overlays
                                    .measured(FrostedOverlay::ForkBranches)
                                    .get(),
                            )
                        };
                        if let (Some(main), Some(anchor)) = (main, anchor) {
                            let _ = main.update(cx, |_, window, cx| {
                                Root::show_tooltip_for_bounds(
                                    window,
                                    cx,
                                    anchor,
                                    super::fork_branches::TOOLTIP_PLACEMENT,
                                    move |window, cx| {
                                        super::fork_branches::fork_branches_tooltip(
                                            tooltip.clone(),
                                            window,
                                            cx,
                                        )
                                    },
                                )
                            });
                        }
                    });
                })
                .detach();
        })
        .on_click(move |_, _, cx| {
            let chat = click_chat.clone();
            cx.defer(move |cx| {
                let Some(chat) = chat.upgrade() else {
                    return;
                };
                let (main, anchor) = {
                    let view = chat.read(cx);
                    (
                        view.main_window,
                        view.frosted_overlays
                            .measured(FrostedOverlay::ForkBranches)
                            .get(),
                    )
                };
                let (Some(main), Some(anchor)) = (main, anchor) else {
                    return;
                };
                let _ = main.update(cx, |_, window, cx| {
                    Root::hide_tooltip(window, cx);
                    chat.update(cx, |chat, cx| {
                        chat.open_fork_branches_menu(anchor, window, cx)
                    });
                });
                activate_main_window(Some(main), cx);
            });
        })
        .into_any_element()
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

#[cfg(not(target_os = "macos"))]
fn attach_overlay_window(_: &mut Window, _: *mut std::ffi::c_void) {}

/// Whether `set_overlay_window_alpha` can fade a control's window; elsewhere the fork switcher dims
/// its own badge and the blur behind it stays at full strength.
const WINDOW_ALPHA: bool = cfg!(target_os = "macos");

/// Fades the control's whole window. The blur is the window's own backdrop, which an element's
/// opacity cannot reach.
#[cfg(target_os = "macos")]
fn set_overlay_window_alpha(window: &mut Window, alpha: f32) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    unsafe extern "C" {
        fn GhostexGpuiSetChildWindowAlpha(view: *mut std::ffi::c_void, alpha: f64);
    }
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
    {
        unsafe { GhostexGpuiSetChildWindowAlpha(handle.ns_view.as_ptr(), f64::from(alpha)) };
    }
}

#[cfg(not(target_os = "macos"))]
fn set_overlay_window_alpha(_: &mut Window, _: f32) {}
