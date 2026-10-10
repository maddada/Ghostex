use super::*;
use gpui::{
    AnyElement, App, Bounds, Context, Div, FocusHandle, InteractiveElement as _, KeyDownEvent,
    ParentElement as _, Pixels, Render, Stateful, StatefulInteractiveElement as _, Styled as _,
    Window, div, px, size,
};
use gpui_component::v_flex;
use std::cell::Cell;
use std::rc::Rc;

/// Sizes the child window to the dialog's own layout.
///
/// The window opens at a per-modal first-frame estimate. The first prepaint
/// knows the real header and body heights, so the window is resized to fit
/// them exactly; later prepaints only grow it (a validation or failure line
/// appearing) and never shrink it, so a shorter follow-up state keeps the frame
/// the opening layout established, like the React fit-height pass did.
pub(crate) struct ModalFit {
    requested: Rc<Cell<Option<f32>>>,
    /// A fixed-frame modal (the editors that fill their window) never resizes.
    fixed: bool,
}

impl ModalFit {
    pub(crate) fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(None)),
            fixed: false,
        }
    }

    /// For modals whose React twin is pinned to `100vh`: the window keeps the
    /// size it opened at and the body stretches to fill it.
    pub(crate) fn fixed() -> Self {
        Self {
            requested: Rc::new(Cell::new(None)),
            fixed: true,
        }
    }

    /// Fits the window to its content again on the next paint, shrinking it too: for a dialog
    /// whose fields come and go (a row that only one mode shows).
    pub(crate) fn refit(&self) {
        self.requested.set(None);
    }

    /// A prepaint listener for the column that holds the header and body.
    pub(crate) fn listener(
        &self,
        extra_height: f32,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let requested = self.requested.clone();
        let fixed = self.fixed;
        move |bounds, window, cx| {
            if fixed || bounds.is_empty() {
                return;
            }
            let content: f32 = bounds
                .iter()
                .map(|bounds| f32::from(bounds.size.height))
                .sum();
            let needed = (content + extra_height).round();
            let current = f32::from(window.viewport_size().height).round();
            let first = requested.get().is_none();
            if !first && needed <= current {
                return;
            }
            if (needed - current).abs() < 1.0 || requested.get() == Some(needed) {
                if first {
                    requested.set(Some(needed));
                }
                return;
            }
            requested.set(Some(needed));
            let handle = window.window_handle();
            let width = window.viewport_size().width;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_root, window, _cx| {
                    window.resize(size(width, px(needed)));
                });
            });
        }
    }
}

/// The dialog frame: the window surface, the platform UI font, 24px padding,
/// header and body 20px apart, the footer pinned to the bottom, keyboard focus
/// on the frame so Escape and Enter reach the modal's key handler.
pub(crate) fn modal_shell<V: Render>(
    p: &ModalPalette,
    id: &'static str,
    focus_handle: &FocusHandle,
    fit: &ModalFit,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    content: Vec<AnyElement>,
    footer: AnyElement,
    overlay: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(id)
        .size_full()
        .overflow_hidden()
        .bg(hsla(p.surface))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .p(px(MODAL_WINDOW_PADDING))
                .gap(px(MODAL_SECTION_GAP))
                .child(
                    v_flex()
                        .flex_1()
                        .w_full()
                        .gap(px(MODAL_SECTION_GAP))
                        .on_children_prepainted(fit.listener(MODAL_FIT_EXTRA_HEIGHT))
                        .children(content),
                )
                .child(footer),
        )
        .children(overlay)
}

/// [`modal_shell`] with a caller-chosen window inset for the React dialogs that
/// own their own edge padding in the child window (Add Worktree: 17px). The fit
/// pass adds `padding * 2` plus the two 20px gaps and the 32px footer row.
pub(crate) fn modal_shell_inset<V: Render>(
    p: &ModalPalette,
    id: &'static str,
    focus_handle: &FocusHandle,
    fit: &ModalFit,
    padding: f32,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    content: Vec<AnyElement>,
    footer: AnyElement,
    overlay: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let extra_height = padding * 2.0 + MODAL_SECTION_GAP * 2.0 + MODAL_FOOTER_BUTTON_HEIGHT;
    div()
        .id(id)
        .size_full()
        .overflow_hidden()
        .bg(hsla(p.surface))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .p(px(padding))
                .gap(px(MODAL_SECTION_GAP))
                .child(
                    v_flex()
                        .flex_1()
                        .w_full()
                        .gap(px(MODAL_SECTION_GAP))
                        .on_children_prepainted(fit.listener(extra_height))
                        .children(content),
                )
                .child(footer),
        )
        .children(overlay)
}

/// Space kept between a scrolling modal's bottom edge and the display's
/// visible bottom edge (above the Dock or taskbar) when the window grows.
pub(crate) const MODAL_SCROLL_FIT_SCREEN_MARGIN: f32 = 16.0;

/// A large panel modal's opening size: its base frame 10% wider and taller
/// (CDXC:AppModal 2026-10-01 in app/model/app_modal_kind.rs).
pub(crate) fn large_panel_modal_size(width: f32, height: f32) -> (f32, f32) {
    ((width * 1.1).round(), (height * 1.1).round())
}

/// The window fit for a modal whose body scrolls (Remote Setup): the window is
/// sized once, on open, to the header plus the body's own content and never
/// past the display's visible bottom. Anything taller later (the Android
/// popover, an error line) scrolls inside the body, as in the React twin.
pub(crate) struct ModalScrollFit {
    requested: Rc<Cell<Option<f32>>>,
    content_height: Rc<Cell<f32>>,
}

impl ModalScrollFit {
    pub(crate) fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(None)),
            content_height: Rc::new(Cell::new(0.0)),
        }
    }

    /// A prepaint listener for the scroll container, whose only child is the body column.
    fn content_listener(&self) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let content_height = self.content_height.clone();
        move |bounds, _window, _cx| {
            content_height.set(
                bounds
                    .iter()
                    .map(|bounds| f32::from(bounds.size.height))
                    .sum(),
            );
        }
    }

    /// A prepaint listener for the column holding the header and the scroll container.
    fn listener(
        &self,
        extra_height: f32,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let requested = self.requested.clone();
        let content_height = self.content_height.clone();
        move |bounds, window, cx| {
            let Some(header) = bounds.first() else {
                return;
            };
            let content = f32::from(header.size.height) + content_height.get();
            let mut needed = (content + extra_height).round();
            // gpui can resize a window but not move it, and the resize keeps
            // the top edge, so the cap is the room below the window's top.
            if let Some(display) = window.display(cx) {
                let visible = display.visible_bounds();
                let visible_bottom = f32::from(visible.origin.y + visible.size.height);
                let top = f32::from(window.bounds().origin.y);
                let limit = (visible_bottom - top - MODAL_SCROLL_FIT_SCREEN_MARGIN).round();
                if limit > 0.0 {
                    needed = needed.min(limit);
                }
            }
            let current = f32::from(window.viewport_size().height).round();
            // CDXC:AppModal 2026-09-16 DECISION:
            // User: a scrolling modal must not grow when a section expands (Remote Setup's "How to install"); it keeps the height it opened with and the body scrolls instead.
            if requested.get().is_some() {
                return;
            }
            requested.set(Some(needed));
            if (needed - current).abs() < 1.0 {
                return;
            }
            let handle = window.window_handle();
            let width = window.viewport_size().width;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_root, window, _cx| {
                    window.resize(size(width, px(needed)));
                });
            });
        }
    }
}

/// [`modal_shell`] for a dialog whose body is a scroll container: the header
/// stays put, `body` scrolls inside the remaining height, and the optional
/// footer keeps its row. Sized by a [`ModalScrollFit`]. `body_id` names the
/// scroll container.
pub(crate) fn modal_shell_scrolling<V: Render>(
    p: &ModalPalette,
    id: &'static str,
    body_id: &'static str,
    focus_handle: &FocusHandle,
    fit: &ModalScrollFit,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    header: AnyElement,
    body: AnyElement,
    footer: Option<AnyElement>,
    overlay: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let extra_height = MODAL_WINDOW_PADDING * 2.0
        + MODAL_SECTION_GAP
        + if footer.is_some() {
            MODAL_SECTION_GAP + MODAL_FOOTER_BUTTON_HEIGHT
        } else {
            0.0
        };
    div()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(hsla(p.surface))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .p(px(MODAL_WINDOW_PADDING))
                .gap(px(MODAL_SECTION_GAP))
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .gap(px(MODAL_SECTION_GAP))
                        .on_children_prepainted(fit.listener(extra_height))
                        .child(header)
                        .child(
                            div()
                                .on_children_prepainted(fit.content_listener())
                                .id(body_id)
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .overflow_y_scroll()
                                .child(body),
                        ),
                )
                .children(footer),
        )
        .children(overlay)
}

// ---------------------------------------------------------------------------
// Added with the Agents Hub modal: the 40px raised tab rail with shortcut
// hints, the bordered segmented control, skinned text inputs and the shadcn
// `Button` variants with caller-chosen colors.
// ---------------------------------------------------------------------------

/// The corner close button's diameter; it is centred on the modal's top-right corner.
pub(crate) const MODAL_CORNER_CLOSE_SIZE: f32 = 24.0;
/// How far down from the window's top edge, across its full width, the pointer reveals the corner
/// close button.
pub(crate) const MODAL_CORNER_CLOSE_REVEAL_HEIGHT: f32 = 80.0;

/// The hover-only close button a native app modal's window frame draws in its top-right corner
/// (window/modal_window_frame.rs).
pub(crate) trait ModalCornerClose: Render {
    /// Closes the modal the way its Escape key does.
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>);

    /// False while the modal must stay open (first-run onboarding before a project exists) or when
    /// it draws its own close button in that corner.
    fn shows_corner_close(&self, _cx: &App) -> bool {
        true
    }

    /// True while a click away must leave the modal open (an unsaved form, a hotkey being
    /// recorded). Read only by the modals that close on click-away
    /// (`popup_dismissal::close_app_modal_on_click_away`).
    fn keeps_open_on_click_away(&self, _window: &Window, _cx: &App) -> bool {
        false
    }
}
