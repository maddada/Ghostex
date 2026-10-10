//! The board's scrolling as the user sees it: the bar under the lanes and the bar in each lane,
//! and the sideways scroll that carries a dragged card to a lane off screen. The scroll
//! containers themselves are in `render.rs` (the lanes row) and `lane.rs` (a lane's cards).

use std::time::Duration;

use gpui::{Bounds, Context, Pixels, Point, px};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};

use super::palette::KanbanPalette;
use crate::GhostexGpuiApp;

/// How far from the board's left or right edge a dragged card starts the board scrolling.
const DRAG_AUTOSCROLL_EDGE: f32 = 56.0;
/// The board's fastest scroll under a dragged card, in pixels per frame, reached at the edge.
const DRAG_AUTOSCROLL_MAX_STEP: f32 = 14.0;
const DRAG_AUTOSCROLL_FRAME: Duration = Duration::from_millis(16);

/// A scrollbar in the board's own colours, shown while the pointer is over its scroll area.
///
/// CDXC:ProjectBoard 2026-10-10 WHY:
/// The lanes row has scrolled sideways since the native board arrived, but nothing showed it: a
/// trackpad swipe or Shift+wheel moved it, and a mouse alone had no way to. The bar is what a
/// mouse has, and it also tells the user that there are lanes off screen.
pub(crate) fn kanban_scrollbar(scrollbar: Scrollbar, p: &KanbanPalette) -> Scrollbar {
    let thumb = p.foreground.opacity(if p.glass { 0.30 } else { 0.22 });
    let thumb_hover = p.foreground.opacity(if p.glass { 0.48 } else { 0.38 });
    scrollbar.mode(ScrollbarMode::Hover).styles(|styles| {
        styles
            .thumb(|style| {
                style
                    .bg(thumb)
                    .width(px(6.0))
                    .inset(px(2.0))
                    .radius(px(3.0))
            })
            .thumb_hover(|style| style.bg(thumb_hover))
    })
}

/// How far the board scrolls per frame for a card dragged at `position` over the lanes at
/// `bounds`: nothing in the middle, faster the nearer the pointer is to an edge, negative towards
/// the left.
fn drag_autoscroll_step(position: Point<Pixels>, bounds: Bounds<Pixels>) -> f32 {
    if !bounds.contains(&position) {
        return 0.0;
    }
    let x = f32::from(position.x);
    let into_left = f32::from(bounds.left()) + DRAG_AUTOSCROLL_EDGE - x;
    let into_right = x - (f32::from(bounds.right()) - DRAG_AUTOSCROLL_EDGE);
    let depth = if into_left > 0.0 {
        -into_left
    } else if into_right > 0.0 {
        into_right
    } else {
        return 0.0;
    };
    (depth / DRAG_AUTOSCROLL_EDGE).clamp(-1.0, 1.0) * DRAG_AUTOSCROLL_MAX_STEP
}

impl GhostexGpuiApp {
    /// A card is being dragged at `position` over the lanes at `bounds`. Near the board's left or
    /// right edge the board scrolls that way, every frame, until the pointer leaves the edge or
    /// the drag ends: a drag only reports moves, so a pointer held still at the edge would
    /// otherwise stop the scroll.
    pub(crate) fn native_kanban_drag_autoscroll(
        &mut self,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let step = drag_autoscroll_step(position, bounds);
        self.native_kanban.drag_autoscroll_step = step;
        if step == 0.0 || self.native_kanban.drag_autoscroll.is_some() {
            return;
        }
        self.native_kanban.drag_autoscroll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(DRAG_AUTOSCROLL_FRAME).await;
                let Ok(true) =
                    this.update(cx, |this, cx| this.native_kanban_drag_autoscroll_frame(cx))
                else {
                    break;
                };
            }
        }));
    }

    /// One frame of the scroll under a dragged card: `false` once there is nothing left to do,
    /// which ends the task.
    fn native_kanban_drag_autoscroll_frame(&mut self, cx: &mut Context<Self>) -> bool {
        let step = self.native_kanban.drag_autoscroll_step;
        let scroll = self.native_kanban.lanes_scroll.clone();
        let mut offset = scroll.offset();
        let current = f32::from(offset.x);
        // The offset runs from 0 (scrolled to the left) to minus the overflow.
        let next = (current - step).clamp(-f32::from(scroll.max_offset().x).max(0.0), 0.0);
        if step == 0.0 || !cx.has_active_drag() || next == current {
            // The task ends with this frame. It is let go rather than dropped from inside itself.
            if let Some(task) = self.native_kanban.drag_autoscroll.take() {
                task.detach();
            }
            return false;
        }
        offset.x = px(next);
        scroll.set_offset(offset);
        self.native_kanban_notify(cx);
        true
    }
}
