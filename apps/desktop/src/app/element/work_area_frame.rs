//! The elements that draw the work area frame (`app/helpers/work_area_frame.rs` holds its setting,
//! sizes and colours): the card, which records where it is for the panes and web pages inside it,
//! and the gutter ring under glass.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    PathBuilder, Pixels, Styled, Window, canvas, point, px,
};

use crate::app::helpers::{
    WORK_AREA_FRAME_MARGIN, WORK_AREA_FRAME_RADIUS, sidebar_glass_tint, window_glass_active,
    work_area_frame_active,
};

thread_local! {
    /// Each window's card this frame (its outer edge, border included), by window id. The card
    /// records itself in prepaint, before any element inside it prepaints, so the panes and web
    /// pages in it read this frame's bounds, never the last frame's.
    static CARD_BOUNDS: RefCell<HashMap<u64, Bounds<Pixels>>> = RefCell::new(HashMap::new());
}

fn window_key(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

/// The card's outer edge in `window` this frame, while the frame is on and the window draws one.
pub(crate) fn work_area_frame_card_bounds(window: &Window) -> Option<Bounds<Pixels>> {
    if !work_area_frame_active() {
        return None;
    }
    CARD_BOUNDS.with(|cards| cards.borrow().get(&window_key(window)).copied())
}

/// Wraps the work area column, which is styled as the card, and records its bounds.
pub(crate) struct WorkAreaFrameCard<E>(E);

pub(crate) fn work_area_frame_card<E: Element + Styled>(element: E) -> WorkAreaFrameCard<E> {
    WorkAreaFrameCard(element)
}

impl<E: Element + Styled> IntoElement for WorkAreaFrameCard<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: Element + Styled> Element for WorkAreaFrameCard<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.0.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.0.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.0.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        CARD_BOUNDS.with(|cards| {
            cards.borrow_mut().insert(window_key(window), bounds);
        });
        self.0.prepaint(id, inspector_id, bounds, state, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .paint(id, inspector_id, bounds, state, prepaint, window, cx);
    }
}

/// Adds `bounds` with all four corners rounded by `radius` as one closed subpath.
fn add_rounded_rect(path: &mut PathBuilder, bounds: Bounds<Pixels>, radius: Pixels) {
    let (left, top, right, bottom) = (bounds.left(), bounds.top(), bounds.right(), bounds.bottom());
    let radii = point(radius, radius);
    path.move_to(point(left + radius, top));
    path.line_to(point(right - radius, top));
    path.arc_to(radii, px(0.0), false, true, point(right, top + radius));
    path.line_to(point(right, bottom - radius));
    path.arc_to(radii, px(0.0), false, true, point(right - radius, bottom));
    path.line_to(point(left + radius, bottom));
    path.arc_to(radii, px(0.0), false, true, point(left, bottom - radius));
    path.line_to(point(left, top + radius));
    path.arc_to(radii, px(0.0), false, true, point(left + radius, top));
    path.close();
}

/// CDXC:Workarea 2026-11-05 WHY:
/// Under window glass the sidebar and the work area each tint the blurred desktop themselves and nothing tints the window beneath them (CDXC:Theming 2026-09-23 on `SIDEBAR_GLASS_PERCENT_DARK`), so the gutter cannot be a fill under the card: the card's own tint would stack on it. The gutter is the sidebar's tint painted as a ring around the card, from the sidebar's edge to the window's, with the card's rounded shape left out. Must come before the card in the body row.
pub(crate) fn work_area_frame_glass_gutter() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |bounds, _, window, _cx| {
            if !window_glass_active() {
                return;
            }
            let Some(card) = work_area_frame_card_bounds(window) else {
                return;
            };
            // The gutter: the strip above the sidebar and the card, then everything right of the
            // sidebar down to the window's bottom edge.
            let margin = px(WORK_AREA_FRAME_MARGIN);
            let side = card.left() - margin;
            let strip = bounds.top() + margin;
            let mut ring = PathBuilder::fill()
                .with_style(gpui::PathStyle::Fill(gpui::FillOptions::even_odd()));
            ring.move_to(bounds.origin);
            ring.line_to(point(bounds.right(), bounds.top()));
            ring.line_to(point(bounds.right(), bounds.bottom()));
            ring.line_to(point(side, bounds.bottom()));
            ring.line_to(point(side, strip));
            ring.line_to(point(bounds.left(), strip));
            ring.close();
            add_rounded_rect(&mut ring, card, px(WORK_AREA_FRAME_RADIUS));
            if let Ok(path) = ring.build() {
                window.paint_path(path, sidebar_glass_tint());
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Forgets what the frame recorded for a window that closed.
pub(crate) fn forget_work_area_frame_window(window_id: gpui::WindowId) {
    let key = window_id.as_u64();
    CARD_BOUNDS.with(|cards| cards.borrow_mut().remove(&key));
}
