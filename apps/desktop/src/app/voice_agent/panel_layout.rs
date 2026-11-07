//! The voice panel's size, its transcript following new lines, and the voice list opening on the
//! current voice: the decisions as plain functions, plus the panel's resize edges and its "Scroll
//! to bottom" pill. Depends only on gpui, so the preview binary
//! (`src/bin/native_modal_demo/voice_panel.rs`) draws the same.
//!
//! CDXC:VoiceMode 2026-11-08 DECISION: A Discord user asked for three things: "scroll to bottom automatically during conversation", "let me increase the size of the voice agent dialog" and "scroll to the selected voice when opening the list"; the user: "implement these for voice agent pls".
//! - The transcript follows new lines the way the chat does: it keeps following until the reader scrolls up, and following comes back once they scroll back to the end. While they read higher up, the chat's down-arrow "Scroll to bottom" pill (`chat_pills.rs`) shows at the transcript's bottom right and takes them back. Supersedes the 2026-10-20 rule that decided following from the scroll offset at each new line.
//! - Every edge and corner of the panel resizes it, between `MIN_SIZE` and `MAX_SIZE` and inside the main window; the size is kept on disk (`gpui-voice-panel.json`), the place still only for the app's run.
//! - The voice list opens scrolled so the current voice shows with two rows above it, on a highlighted row.
//!
//! CDXC:VoiceMode 2026-11-08 WHY: the panel's transcript used to ask its scroll handle for the bottom when a line arrived, outside the panel's own frame. gpui drops a "scroll to the bottom" asked of a handle that is at its bottom when any window's frame ends (`fast/layers/scroll_to_bottom.rs`, thread-local), so when the main window drew first the request was lost, the line landed below the fold, and every later line counted the transcript as scrolled away. Following is now a flag the panel keeps, asked again in each of its own renders, which its own prepaint takes in the same frame.

use gpui::{
    AnyElement, App, Bounds, CursorStyle, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, Window, div, point, px, size, svg,
};

use super::controls::ControlColors;

/// The panel's size until the user resizes it.
pub(crate) const DEFAULT_SIZE: Size<Pixels> = Size {
    width: px(340.0),
    height: px(360.0),
};
/// The smallest the panel gets, by a resize or in a small window.
pub(crate) const MIN_SIZE: Size<Pixels> = Size {
    width: px(260.0),
    height: px(220.0),
};
/// The largest a resize makes it.
pub(crate) const MAX_SIZE: Size<Pixels> = Size {
    width: px(760.0),
    height: px(1000.0),
};
/// How thick the resize edges are, and how far the corners reach along each side.
const EDGE_THICKNESS: f32 = 5.0;
const CORNER_REACH: f32 = 12.0;

/// Within this distance of the transcript's end, a scroll down there follows new lines again.
pub(crate) const FOLLOW_SLACK: f32 = 24.0;
/// gpui turns a wheel's lines into this many pixels each, as the chat counts them.
pub(crate) const WHEEL_LINE: f32 = 20.0;

/// The voice list's rows, as `voice_picker` lays them out.
pub(crate) const VOICE_ROW_HEIGHT: f32 = 28.0;
pub(crate) const VOICE_ROW_GAP: f32 = 1.0;
pub(crate) const VOICE_LIST_PADDING: f32 = 6.0;
/// Rows left above the current voice when the list opens.
const VOICE_LIST_ROWS_ABOVE: usize = 2;

const ICON_ARROW_DOWN: &str = "titlebar/arrow-down.svg";

/// `wanted` held between `MIN_SIZE` and `MAX_SIZE` and to the `room` there is; where the room is
/// smaller than `MIN_SIZE` the floor wins, as it always has in a small window.
pub(crate) fn clamp_panel_size(wanted: Size<Pixels>, room: Size<Pixels>) -> Size<Pixels> {
    size(
        wanted
            .width
            .min(MAX_SIZE.width)
            .min(room.width)
            .max(MIN_SIZE.width),
        wanted
            .height
            .min(MAX_SIZE.height)
            .min(room.height)
            .max(MIN_SIZE.height),
    )
}

/// The panel edge or corner a resize drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ResizeEdge {
    pub(crate) const ALL: [ResizeEdge; 8] = [
        ResizeEdge::Left,
        ResizeEdge::Right,
        ResizeEdge::Top,
        ResizeEdge::Bottom,
        ResizeEdge::TopLeft,
        ResizeEdge::TopRight,
        ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight,
    ];

    fn left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    fn right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }

    fn top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }

    fn bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }

    pub(crate) fn cursor(self) -> CursorStyle {
        match self {
            Self::Left | Self::Right => CursorStyle::ResizeLeftRight,
            Self::Top | Self::Bottom => CursorStyle::ResizeUpDown,
            Self::TopLeft | Self::BottomRight => CursorStyle::ResizeUpLeftDownRight,
            Self::TopRight | Self::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Left => "voice-panel-resize-left",
            Self::Right => "voice-panel-resize-right",
            Self::Top => "voice-panel-resize-top",
            Self::Bottom => "voice-panel-resize-bottom",
            Self::TopLeft => "voice-panel-resize-top-left",
            Self::TopRight => "voice-panel-resize-top-right",
            Self::BottomLeft => "voice-panel-resize-bottom-left",
            Self::BottomRight => "voice-panel-resize-bottom-right",
        }
    }
}

/// The frame a resize from `start` makes once the mouse moved `delta`: the dragged edges move,
/// the others stay, the size stays between `MIN_SIZE` and `MAX_SIZE`, and the frame stays inside
/// `area` (the main window less its margins). All in the main window's content coordinates.
pub(crate) fn resize_frame(
    start: Bounds<Pixels>,
    edge: ResizeEdge,
    delta: gpui::Point<Pixels>,
    area: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let (mut left, mut right) = (start.left(), start.right());
    let (mut top, mut bottom) = (start.top(), start.bottom());
    // `max` then `min`: where the area is too small for the floor, the floor wins.
    if edge.left() {
        left = (left + delta.x)
            .max(area.left().max(right - MAX_SIZE.width))
            .min(right - MIN_SIZE.width);
    }
    if edge.right() {
        right = (right + delta.x)
            .min(area.right().min(left + MAX_SIZE.width))
            .max(left + MIN_SIZE.width);
    }
    if edge.top() {
        top = (top + delta.y)
            .max(area.top().max(bottom - MAX_SIZE.height))
            .min(bottom - MIN_SIZE.height);
    }
    if edge.bottom() {
        bottom = (bottom + delta.y)
            .min(area.bottom().min(top + MAX_SIZE.height))
            .max(top + MIN_SIZE.height);
    }
    Bounds {
        origin: point(left, top),
        size: size(right - left, bottom - top),
    }
}

/// Whether the transcript follows new lines after a wheel or trackpad scroll of `delta_y`
/// (positive toward the top), `distance_to_end` from its end as last laid out. A scroll up stops
/// following; a scroll down that reaches the end resumes it. A transcript that doesn't scroll
/// always follows, so a stray wheel over a short one doesn't leave later lines below the fold.
pub(crate) fn follow_after_wheel(
    following: bool,
    distance_to_end: f32,
    scrollable: bool,
    delta_y: f32,
) -> bool {
    if !scrollable {
        return true;
    }
    if delta_y > 0.0 {
        return false;
    }
    following || distance_to_end + delta_y <= FOLLOW_SLACK
}

/// The "Scroll to bottom" pill shows while the reader is away from the end on purpose.
pub(crate) fn shows_scroll_to_bottom(following: bool, distance_to_end: f32) -> bool {
    !following && distance_to_end > 1.0
}

/// How far down the voice list opens: the current voice (`index` in the list) with
/// `VOICE_LIST_ROWS_ABOVE` rows above it, or the top when no voice is current. The list clamps an
/// offset past its end.
pub(crate) fn voice_list_scroll_top(index: Option<usize>) -> Pixels {
    let first = index.map_or(0, |index| index.saturating_sub(VOICE_LIST_ROWS_ABOVE));
    px(first as f32 * (VOICE_ROW_HEIGHT + VOICE_ROW_GAP))
}

/// The panel's resize edges and corners, laid over its rim. `on_press` gets the edge pressed and
/// the press, and starts the resize.
pub(crate) fn resize_edges(
    on_press: impl Fn(ResizeEdge, &MouseDownEvent, &mut Window, &mut App) + Clone + 'static,
) -> Vec<AnyElement> {
    ResizeEdge::ALL
        .into_iter()
        .map(|edge| {
            let on_press = on_press.clone();
            let mut handle = div().id(edge.id()).absolute().cursor(edge.cursor());
            let (thick, reach) = (px(EDGE_THICKNESS), px(CORNER_REACH));
            handle = match edge {
                ResizeEdge::Left => handle.left_0().top(reach).bottom(reach).w(thick),
                ResizeEdge::Right => handle.right_0().top(reach).bottom(reach).w(thick),
                ResizeEdge::Top => handle.top_0().left(reach).right(reach).h(thick),
                ResizeEdge::Bottom => handle.bottom_0().left(reach).right(reach).h(thick),
                ResizeEdge::TopLeft => handle.top_0().left_0().size(reach),
                ResizeEdge::TopRight => handle.top_0().right_0().size(reach),
                ResizeEdge::BottomLeft => handle.bottom_0().left_0().size(reach),
                ResizeEdge::BottomRight => handle.bottom_0().right_0().size(reach),
            };
            handle
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        on_press(edge, event, window, cx);
                    },
                )
                .into_any_element()
        })
        .collect()
}

/// The chat's down-arrow "Scroll to bottom" pill (`scroll_bottom_icon_pill` in `chat_pills.rs`),
/// at the transcript's bottom right while the reader is away from the end.
pub(crate) fn scroll_to_bottom_pill(
    colors: ControlColors,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let label = SharedString::new_static("Scroll to bottom");
    div()
        .id("voice-panel-scroll-to-bottom")
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .absolute()
        .right(px(10.0))
        .bottom(px(8.0))
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .cursor_default()
        .hover(move |this| this.bg(colors.ink.opacity(0.10)))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
        })
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_click(move |_, window, cx| on_press(window, cx))
        .child(
            svg()
                .size(px(14.0))
                .path(ICON_ARROW_DOWN)
                .text_color(colors.text),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    fn main_area() -> Bounds<Pixels> {
        bounds(6.0, 6.0, 1388.0, 888.0)
    }

    #[test]
    fn a_size_stays_between_the_floor_and_the_ceiling() {
        let room = size(px(1400.0), px(900.0));
        assert_eq!(
            clamp_panel_size(size(px(500.0), px(600.0)), room),
            size(px(500.0), px(600.0))
        );
        assert_eq!(clamp_panel_size(size(px(100.0), px(50.0)), room), MIN_SIZE);
        assert_eq!(
            clamp_panel_size(size(px(5000.0), px(5000.0)), room),
            size(MAX_SIZE.width, px(900.0))
        );
    }

    #[test]
    fn a_small_window_shrinks_the_panel_down_to_its_floor() {
        let room = size(px(300.0), px(150.0));
        assert_eq!(
            clamp_panel_size(DEFAULT_SIZE, room),
            size(px(300.0), MIN_SIZE.height)
        );
    }

    #[test]
    fn dragging_the_left_edge_widens_the_panel_and_keeps_its_right_edge() {
        let start = bounds(1000.0, 80.0, 340.0, 360.0);
        let frame = resize_frame(
            start,
            ResizeEdge::Left,
            point(px(-160.0), px(40.0)),
            main_area(),
        );
        assert_eq!(frame, bounds(840.0, 80.0, 500.0, 360.0));
    }

    #[test]
    fn a_corner_resizes_both_ways() {
        let start = bounds(1000.0, 80.0, 340.0, 360.0);
        let frame = resize_frame(
            start,
            ResizeEdge::BottomLeft,
            point(px(-100.0), px(200.0)),
            main_area(),
        );
        assert_eq!(frame, bounds(900.0, 80.0, 440.0, 560.0));
        let frame = resize_frame(
            start,
            ResizeEdge::TopRight,
            point(px(20.0), px(-30.0)),
            main_area(),
        );
        assert_eq!(frame, bounds(1000.0, 50.0, 360.0, 390.0));
    }

    #[test]
    fn a_resize_stops_at_the_floor_the_ceiling_and_the_window() {
        let start = bounds(1000.0, 80.0, 340.0, 360.0);
        // Narrower than the floor: the left edge stops MIN_SIZE from the right one.
        let frame = resize_frame(
            start,
            ResizeEdge::Left,
            point(px(300.0), px(0.0)),
            main_area(),
        );
        assert_eq!(frame.size.width, MIN_SIZE.width);
        assert_eq!(frame.right(), start.right());
        // Wider than the ceiling.
        let frame = resize_frame(
            start,
            ResizeEdge::Left,
            point(px(-900.0), px(0.0)),
            main_area(),
        );
        assert_eq!(frame.size.width, MAX_SIZE.width);
        // Out of the window: the edge stops at the window's margin.
        let frame = resize_frame(
            start,
            ResizeEdge::BottomRight,
            point(px(400.0), px(900.0)),
            main_area(),
        );
        assert_eq!(frame.right(), main_area().right());
        assert_eq!(frame.bottom(), main_area().bottom());
        let frame = resize_frame(
            start,
            ResizeEdge::Top,
            point(px(0.0), px(-500.0)),
            main_area(),
        );
        assert_eq!(frame.top(), main_area().top());
        assert_eq!(frame.bottom(), start.bottom());
    }

    #[test]
    fn scrolling_up_stops_following_and_reaching_the_end_resumes_it() {
        // At the end, following: a scroll up stops it.
        assert!(!follow_after_wheel(true, 0.0, true, 30.0));
        // Away and scrolling down, still far from the end: not yet.
        assert!(!follow_after_wheel(false, 200.0, true, -40.0));
        // A scroll down that lands within the slack of the end follows again.
        assert!(follow_after_wheel(false, 50.0, true, -40.0));
        // Following and scrolling down stays following.
        assert!(follow_after_wheel(true, 0.0, true, -10.0));
        // A transcript too short to scroll always follows.
        assert!(follow_after_wheel(false, 0.0, false, 30.0));
    }

    #[test]
    fn the_scroll_to_bottom_pill_shows_only_while_the_reader_is_away() {
        assert!(!shows_scroll_to_bottom(true, 300.0));
        assert!(shows_scroll_to_bottom(false, 300.0));
        assert!(!shows_scroll_to_bottom(false, 0.5));
    }

    #[test]
    fn the_voice_list_opens_on_the_current_voice_with_two_rows_above_it() {
        let stride = VOICE_ROW_HEIGHT + VOICE_ROW_GAP;
        assert_eq!(voice_list_scroll_top(None), px(0.0));
        assert_eq!(voice_list_scroll_top(Some(0)), px(0.0));
        assert_eq!(voice_list_scroll_top(Some(1)), px(0.0));
        // Ursa, 26th of Grok's 28 voices.
        assert_eq!(voice_list_scroll_top(Some(25)), px(23.0 * stride));
    }
}
