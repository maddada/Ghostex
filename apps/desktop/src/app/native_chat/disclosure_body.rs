//! The body of an open disclosure: the vertical rail down its left and the
//! indent that hangs its content off that rail.
//!
//! React drew it with `SessionChatExpansion` (session-chat-expansion.tsx) and
//! the `.ghostex-chat-expansion*` rules in styles/chat.css (both deleted on
//! 2026-09-25): a two-pixel line in
//! `muted-foreground` at 42%, stretched over the whole body, with the content
//! starting a fixed distance to the right of it. Everything that opens onto
//! more rows uses it, so a reader can see at a glance which rows belong to the
//! heading they expanded: a reasoning row's
//! detail and tool run, an expanded tool's arguments and result, and the
//! "+N previous tool calls" and "N tool calls" groups. Pressing the rail closes
//! the disclosure it belongs to, as React's `.ghostex-chat-expansion-rail`
//! button did. A finished turn's "Worked for" log has no rail since 2026-11-10
//! (completed_work_row.rs).

use super::appearance::ChatAppearance;
use super::state::NativeChatView;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

/// Where the rail hangs, measured from the left edge of the row that owns it.
#[derive(Clone, Copy)]
pub(super) enum DisclosureRail {
    /// A body opened by a heading whose chevron sits in the transcript's marker
    /// column: the completed-work log, a reasoning row, a tool group's toggle.
    /// React's `.ghostex-chat-expansion` with no override.
    Marker,
    /// One tool row's own detail, indented past the tool rows around it.
    /// React's `.ghostex-chat-work-detail`.
    ToolDetail,
}

impl DisclosureRail {
    /// Where the body's first column of content starts, from the left edge of the row that owns
    /// the rail. These are React's positions (the marker rail's `-5px` inset plus the box and the
    /// `calc(0.75rem - 5px)` gap, and `.ghostex-chat-work-detail`'s own override); they are kept
    /// as they were so moving the line never moves the content.
    fn content_x(self) -> f32 {
        match self {
            Self::Marker => 17.0,
            Self::ToolDetail => 29.5,
        }
    }
}

/// The width of every disclosure rail's line.
///
/// CDXC:SessionChat 2026-10-12 DECISION: the user pointed at the thin, subtle line down an opened
/// "Worked" fold and the thicker, brighter line beside a tool group's rows and said "make the line
/// in B look like the one at A". Every rail (the fold's, a tool group's, one tool's detail) is that
/// hairline at `RAIL_LINE_OPACITY` of the foreground; the phone's `DisclosureBody` and
/// `theme.ink(0.1)` in `apps/mobile/app/src/chat/native/transcript/Disclosure.tsx` match it.
const RAIL_LINE_WIDTH: f32 = 1.0;

/// How much of the foreground colour a disclosure rail's line shows at rest.
const RAIL_LINE_OPACITY: f32 = 0.1;

/// The centre of the line, from the left edge of the row that owns the rail: the
/// centre of the chevron or glyph slot that opens the body (`MARKER_INSET` 2 plus half of the
/// 16-pixel `MARKER_SLOT`; the chevron rows in transcript.rs and tool_run.rs and the tool row's
/// glyph all use that slot). One value for every depth, so each line runs straight down from
/// the middle of the icon above it; React's lines sat at 2.5 and 15, 7.5 left and 5 right of it.
const SLOT_CENTRE: f32 = 2.0 + 16.0 / 2.0;

/// The rail's hit target: the line is centred in it, and it stays inside the row's
/// content column (its right edge is the marker rail's content start), so the reader does not
/// have to land on two pixels to close the body.
const RAIL_BOX: f32 = 14.0;

/// Hovering the rail's box lights the line inside it, as React's `:hover::before` did.
const RAIL_GROUP: &str = "native-chat-disclosure-rail";

/// Wrap an open disclosure's rows in the rail that says they belong to the
/// heading above them. `gap` is the spacing between those rows, which stays
/// whatever the surrounding column already used. `key` is the disclosure the
/// heading toggles; pressing the rail closes it and records the close against
/// verbose mode's default, which is inert for the rows that default to closed.
/// `label` is React's `aria-label` for that rail.
pub(super) fn disclosure_body(
    p: &ChatAppearance,
    rail: DisclosureRail,
    gap: f32,
    key: String,
    label: impl Into<SharedString>,
    children: impl IntoIterator<Item = AnyElement>,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    let s = p.scale;
    let (line_width, line_color) = (RAIL_LINE_WIDTH, p.foreground.opacity(RAIL_LINE_OPACITY));
    div()
        .flex()
        .min_w_0()
        .ml(px((SLOT_CENTRE - RAIL_BOX / 2.0) * s))
        .gap(px((rail.content_x() - SLOT_CENTRE - RAIL_BOX / 2.0) * s))
        .child(
            div()
                .id(SharedString::from(format!("rail:{key}")))
                .group(RAIL_GROUP)
                .role(gpui::Role::Button)
                .aria_label(label)
                .w(px(RAIL_BOX * s))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .chat_cursor_pointer()
                .child(
                    div()
                        .w(px(line_width * s))
                        .rounded(px(line_width / 2.0 * s))
                        .bg(line_color)
                        .group_hover(RAIL_GROUP, |style| style.bg(p.foreground)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_marker_disclosure(key.clone(), true, cx);
                    cx.stop_propagation();
                })),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(gap * s))
                .children(children),
        )
        .into_any_element()
}
