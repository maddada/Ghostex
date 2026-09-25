//! The body of an open disclosure: the vertical rail down its left and the
//! indent that hangs its content off that rail.
//!
//! React drew it with `SessionChatExpansion` (session-chat-expansion.tsx) and
//! the `.ghostex-chat-expansion*` rules in styles/chat.css: a two-pixel line in
//! `muted-foreground` at 42%, stretched over the whole body, with the content
//! starting a fixed distance to the right of it. Everything that opens onto
//! more rows uses it, so a reader can see at a glance which rows belong to the
//! heading they expanded: the turn's "Worked for Xs" log, a reasoning row's
//! detail and tool run, an expanded tool's arguments and result, and the
//! "+N previous tool calls" and "N tool calls" groups. Pressing the rail closes
//! the disclosure it belongs to, as React's `.ghostex-chat-expansion-rail`
//! button did.

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
    /// The centre of the two-pixel line. React reached these two values through
    /// the marker-column tokens plus `.ghostex-chat-expansion`'s own negative
    /// inset, which `.ghostex-chat-work-detail` overrides and the other bodies
    /// do not; they are written out here because GPUI has no cascade to inherit
    /// them from.
    fn centre(self) -> f32 {
        match self {
            Self::Marker => 2.5,
            Self::ToolDetail => 15.0,
        }
    }
}

/// The rail's hit target, React's fifteen-pixel `.ghostex-chat-expansion-rail` box: the two-pixel
/// line is centred in it, so the line and the content keep the places they had when the rail was
/// only the line, and the reader does not have to land on two pixels to close the body.
const RAIL_BOX: f32 = 15.0;

/// The gap between the rail's box and the first column of content, React's
/// `calc(0.75rem - 5px)` on `.ghostex-chat-expansion`.
const RAIL_TO_CONTENT: f32 = 7.0;

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
    div()
        .flex()
        .min_w_0()
        // Negative on the marker rail, as React's `margin-inline-start: -5px` was: the box widens
        // into the row's own padding, never over a neighbouring control.
        .ml(px((rail.centre() - RAIL_BOX / 2.0) * s))
        .gap(px(RAIL_TO_CONTENT * s))
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
                        .w(px(2.0 * s))
                        .rounded(px(1.0 * s))
                        .bg(p.muted.opacity(0.42))
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
