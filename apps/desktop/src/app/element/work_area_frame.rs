//! The elements that draw the work area frame (`app/helpers/work_area_frame.rs` holds its sizes
//! and colours): the card, which records where it is for the panes and web pages inside it,
//! and the gutter ring under glass.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    PathBuilder, Pixels, Styled, Window, canvas, point, px,
};

use crate::app::helpers::{
    WORK_AREA_FRAME_BORDER, WORK_AREA_FRAME_INNER_RADIUS, WORK_AREA_FRAME_MARGIN,
    WORK_AREA_FRAME_RADIUS, floating_sidebar_layout, sidebar_glass_tint, window_glass_active,
    workspace_column_background,
};

thread_local! {
    /// Each window's card this frame (its outer edge, border included), by window id. The card
    /// records itself in prepaint, before any element inside it prepaints, so the panes and web
    /// pages in it read this frame's bounds, never the last frame's.
    static CARD_BOUNDS: RefCell<HashMap<u64, Bounds<Pixels>>> = RefCell::new(HashMap::new());
    /// Each window's docked sidebar divider this frame, by window id: where the sidebar's own tint
    /// ends, as layout snapped it. Taken by the glass gutter when it paints.
    static SIDEBAR_DIVIDER_BOUNDS: RefCell<HashMap<u64, Bounds<Pixels>>> =
        RefCell::new(HashMap::new());
    /// Each window's floating sidebar card parts this frame (the rail's and the sidebar's), by
    /// window id and part, with their corner radii (`work_area_frame_sidebar_card_marker`).
    static SIDEBAR_CARD_BOUNDS: RefCell<HashMap<(u64, u8), (Bounds<Pixels>, gpui::Corners<Pixels>)>> =
        RefCell::new(HashMap::new());
}

fn window_key(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

/// The card's outer edge in `window` this frame, once the window has drawn one.
pub(crate) fn work_area_frame_card_bounds(window: &Window) -> Option<Bounds<Pixels>> {
    CARD_BOUNDS.with(|cards| cards.borrow().get(&window_key(window)).copied())
}

/// The inside of the card's border, where its panes and web pages sit. `None` in the Floating
/// sidebar layout, where the work area is flat and nothing in it rounds to a card.
pub(crate) fn work_area_frame_inner_bounds(window: &Window) -> Option<Bounds<Pixels>> {
    if floating_sidebar_layout() {
        return None;
    }
    work_area_frame_card_bounds(window).map(|card| {
        let border = px(WORK_AREA_FRAME_BORDER);
        Bounds::from_corners(
            point(card.left() + border, card.top() + border),
            point(card.right() - border, card.bottom() - border),
        )
    })
}

/// The corner radii a rectangle inside the card needs: the inner radius on each corner that sits on
/// one of the card's inner corners (within a device pixel or two), none elsewhere.
/// Which sides of `rect` lie on the inside of the card's border.
pub(crate) fn work_area_frame_touching_edges(
    rect: Bounds<Pixels>,
    window: &Window,
) -> Option<gpui::Edges<bool>> {
    let inner = work_area_frame_inner_bounds(window)?;
    let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(2.0);
    Some(gpui::Edges {
        left: near(rect.left(), inner.left()),
        right: near(rect.right(), inner.right()),
        top: near(rect.top(), inner.top()),
        bottom: near(rect.bottom(), inner.bottom()),
    })
}

pub(crate) fn work_area_frame_corner_radii(
    rect: Bounds<Pixels>,
    window: &Window,
) -> Option<gpui::Corners<Pixels>> {
    let gpui::Edges {
        left,
        right,
        top,
        bottom,
    } = work_area_frame_touching_edges(rect, window)?;
    let radius = |round: bool| {
        px(if round {
            WORK_AREA_FRAME_INNER_RADIUS
        } else {
            0.0
        })
    };
    Some(gpui::Corners {
        top_left: radius(top && left),
        top_right: radius(top && right),
        bottom_right: radius(bottom && right),
        bottom_left: radius(bottom && left),
    })
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

/// Adds `bounds` as one closed square subpath.
fn add_rect(path: &mut PathBuilder, bounds: Bounds<Pixels>) {
    path.move_to(bounds.origin);
    path.line_to(point(bounds.right(), bounds.top()));
    path.line_to(point(bounds.right(), bounds.bottom()));
    path.line_to(point(bounds.left(), bounds.bottom()));
    path.close();
}

/// Adds `bounds` with each corner rounded by its own radius (square where it is 0) as one closed
/// subpath.
fn add_rounded_rect_corners(
    path: &mut PathBuilder,
    bounds: Bounds<Pixels>,
    radii: gpui::Corners<Pixels>,
) {
    let (left, top, right, bottom) = (bounds.left(), bounds.top(), bounds.right(), bounds.bottom());
    let arc = |path: &mut PathBuilder, radius: Pixels, to: gpui::Point<Pixels>| {
        if radius > px(0.0) {
            path.arc_to(point(radius, radius), px(0.0), false, true, to);
        } else {
            path.line_to(to);
        }
    };
    path.move_to(point(left + radii.top_left, top));
    path.line_to(point(right - radii.top_right, top));
    arc(path, radii.top_right, point(right, top + radii.top_right));
    path.line_to(point(right, bottom - radii.bottom_right));
    arc(
        path,
        radii.bottom_right,
        point(right - radii.bottom_right, bottom),
    );
    path.line_to(point(left + radii.bottom_left, bottom));
    arc(
        path,
        radii.bottom_left,
        point(left, bottom - radii.bottom_left),
    );
    path.line_to(point(left, top + radii.top_left));
    arc(path, radii.top_left, point(left + radii.top_left, top));
    path.close();
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
/// `card_beside_rail`: the card starts at the rail's right edge (`work_area_frame_left_gutter`), so
/// no gutter lies left of it and the rail tints its own column.
///
/// CDXC:Workarea 2026-11-24 WHY:
/// The user on Windows: "i see a 2nd line between the native ... please remove this line here no need for it. on linux and mac i dont see this extra line". The ring's left edge was worked out as the card's left less the 6pt gutter, but layout snaps the divider (1pt) and the card's padding (5pt) to device pixels one by one. At 175% that is 2 + 9 = 11 device pixels, not the 10.5 the 6pt gutter comes to, so the ring started half a device pixel right of the sidebar and left a column that was barely tinted, where the blurred desktop showed through as a second line beside the card's border. At 200% every one of those is a whole pixel, which is why macOS never showed it. The ring now starts at the divider's own snapped edge, where the sidebar's tint ends (`work_area_frame_sidebar_edge_marker`).
pub(crate) fn work_area_frame_glass_gutter(card_beside_rail: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _cx| {
            if !window_glass_active() {
                return;
            }
            let Some(card) = work_area_frame_card_bounds(window) else {
                return;
            };
            if floating_sidebar_layout() {
                // The Floating sidebar layout's gutter is the work area's: its tint everywhere the
                // flat work area and the sidebar card do not cover (the rail included, which draws
                // no fill of its own there).
                let key = window_key(window);
                let parts: Vec<_> = SIDEBAR_CARD_BOUNDS.with(|cards| {
                    let mut cards = cards.borrow_mut();
                    [0u8, 1]
                        .iter()
                        .filter_map(|part| cards.remove(&(key, *part)))
                        .collect()
                });
                let mut ring = PathBuilder::fill()
                    .with_style(gpui::PathStyle::Fill(gpui::FillOptions::even_odd()));
                add_rect(&mut ring, bounds);
                for (part, radii) in parts {
                    if part.right() <= card.left() {
                        add_rounded_rect_corners(&mut ring, part, radii);
                    }
                }
                add_rect(&mut ring, card);
                if let Ok(path) = ring.build() {
                    window.paint_path(path, workspace_column_background());
                }
                return;
            }
            // The gutter: the strip above the sidebar and the card, then everything right of the
            // sidebar down to the window's bottom edge.
            let margin = px(WORK_AREA_FRAME_MARGIN);
            let divider = SIDEBAR_DIVIDER_BOUNDS
                .with(|dividers| dividers.borrow_mut().remove(&window_key(window)));
            // A divider recorded in an earlier frame (glass turned on with the sidebar since
            // hidden) no longer sits left of the card and is ignored.
            let (side, strip) = match divider {
                Some(divider) if !card_beside_rail && divider.right() <= card.left() => {
                    (divider.left(), divider.top())
                }
                _ if card_beside_rail => (card.left(), bounds.top() + margin),
                _ => (card.left() - margin, bounds.top() + margin),
            };
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

/// A zero-paint child of the docked sidebar's divider that records the divider's laid-out bounds
/// for the glass gutter this frame (see `work_area_frame_glass_gutter`). The divider comes before
/// the gutter in the body row, so its prepaint runs before the gutter paints.
pub(crate) fn work_area_frame_sidebar_edge_marker() -> impl IntoElement {
    canvas(
        |bounds, window, _cx| {
            SIDEBAR_DIVIDER_BOUNDS.with(|dividers| {
                dividers.borrow_mut().insert(window_key(window), bounds);
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A zero-paint child of one part of the floating sidebar card (the Floating sidebar layout: `part`
/// 0 the rail's, 1 the sidebar's) that records its laid-out bounds (inside the card's border) and its
/// corners for the glass gutter this frame, which leaves every part's shape out.
pub(crate) fn work_area_frame_sidebar_card_marker(
    part: u8,
    radii: gpui::Corners<Pixels>,
) -> impl IntoElement {
    canvas(
        move |bounds, window, _cx| {
            // The marker spans the card's inside (its border excluded), so the gutter's tint runs
            // under the card's see-through 1px border; leaving the border out of the tint showed
            // the raw artwork's dots through it as a glowing, dotted edge.
            SIDEBAR_CARD_BOUNDS.with(|cards| {
                cards
                    .borrow_mut()
                    .insert((window_key(window), part), (bounds, radii));
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Clips a web page's native view to the card's inner corners in `window`; nothing before the
/// window has drawn its card.
/// SEE-ALSO: native/macos/GpuiWindowCorners.h (`GhostexGpuiClipNativeViewToWorkAreaCard`).
#[cfg(target_os = "macos")]
pub(crate) fn refresh_native_work_area_frame_clip(
    native_view: *mut std::ffi::c_void,
    window: &Window,
) {
    unsafe extern "C" {
        fn GhostexGpuiCEFRefreshNativeViewWorkAreaCardCorners(
            native_view: *mut std::ffi::c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            radius: f64,
        );
    }
    let Some(inner) = work_area_frame_inner_bounds(window) else {
        return;
    };
    unsafe {
        GhostexGpuiCEFRefreshNativeViewWorkAreaCardCorners(
            native_view,
            f64::from(inner.left().as_f32()),
            f64::from(inner.top().as_f32()),
            f64::from(inner.size.width.as_f32()),
            f64::from(inner.size.height.as_f32()),
            f64::from(WORK_AREA_FRAME_INNER_RADIUS),
        )
    }
}

/// Rounds a web page's native window on Windows and Linux to the card's inner corners in `window`
/// where it meets them; square where it does not and before the window has drawn its card.
/// SEE-ALSO: cef/linux_x11.rs and cef/windows.rs (`set_native_view_corner_radii`).
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) fn clip_native_view_to_work_area_frame(
    browser: &crate::cef::CefBrowser,
    bounds: Bounds<Pixels>,
    window: &Window,
) {
    let radii = work_area_frame_corner_radii(bounds, window).map_or([0.0; 4], |corners| {
        [
            corners.top_left.as_f32(),
            corners.top_right.as_f32(),
            corners.bottom_right.as_f32(),
            corners.bottom_left.as_f32(),
        ]
    });
    browser.set_corner_radii(radii, window.scale_factor());
}

/// Forgets what the frame recorded for a window that closed.
pub(crate) fn forget_work_area_frame_window(window_id: gpui::WindowId) {
    let key = window_id.as_u64();
    CARD_BOUNDS.with(|cards| cards.borrow_mut().remove(&key));
    SIDEBAR_DIVIDER_BOUNDS.with(|dividers| dividers.borrow_mut().remove(&key));
    SIDEBAR_CARD_BOUNDS.with(|cards| cards.borrow_mut().retain(|(window, _), _| *window != key));
}
