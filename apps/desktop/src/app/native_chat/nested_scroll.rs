//! A height-capped box that scrolls inside the transcript: a tool's arguments
//! and result, long harness output, a command card's captured output.

use super::state::NativeChatView;
use gpui::{
    AnyElement, DispatchPhase, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollEvent, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, canvas,
    div, point, px,
};

impl NativeChatView {
    /// Make `scroller` a vertical scroll box keyed by `key`, which keeps its offset across frames.
    ///
    /// CDXC:SessionChat 2026-09-21 WHY: the transcript is a GPUI `list`, which registers its wheel handler after painting its rows, so on the bubble pass it scrolls before any box inside a row sees the wheel and both then take the same delta: the page moves and the box slides out from under the pointer. The box therefore scrolls itself on the capture pass and stops the event there, and lets it through once it is at its edge in that direction, which is how the browser chains a nested scroller into the page in React's transcript.
    pub(super) fn nested_scroll(&self, key: impl Into<SharedString>, scroller: Div) -> AnyElement {
        let key = key.into();
        let handle = self
            .nested_scrolls
            .borrow_mut()
            .entry(key.clone())
            .or_default()
            .clone();
        let wheel = handle.clone();
        div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .child(scroller.id(key).track_scroll(&handle).overflow_y_scroll())
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
                        let handle = wheel.clone();
                        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture
                                || !hitbox.should_handle_scroll(window)
                            {
                                return;
                            }
                            let delta = event.delta.pixel_delta(window.line_height()).y;
                            let offset = handle.offset();
                            let next = (offset.y + delta).clamp(-handle.max_offset().y, px(0.0));
                            if next == offset.y {
                                return;
                            }
                            handle.set_offset(point(offset.x, next));
                            cx.stop_propagation();
                            window.refresh();
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// The offsets of every nested box the reader has opened, keyed like the boxes themselves.
pub(crate) type NestedScrolls =
    std::cell::RefCell<std::collections::HashMap<SharedString, ScrollHandle>>;
