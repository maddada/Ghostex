//! The transcript minimap: one dash per user prompt, down the left side of the chat.
//!
//! Layout is a real sibling column of the list, never a layer on top of it: each
//! dash owns a strip of the rail, the strips tile without overlapping, and the
//! visible dash is itself the click target. A gutter of the same width on the
//! right keeps the transcript centered on the composer, which a rail on one side
//! alone would pull off center.
//!
//! What a dash means, how wide it gets near the pointer, how long a preview runs
//! and how far apart the dashes sit come from packages/gx-chat-core/visual/minimap.json
//! (read by `extras/minimap_rail.rs` in the core). The rows themselves are
//! projected once per change in packages/gx-chat-core/src/extras/minimap.rs.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Bounds, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde::Deserialize;
use serde_json::Value;
use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MinimapSpec {
    minimum_turns: usize,
    scale: f32,
    spacing: f32,
    dash_height: f32,
    rail_width: f32,
    line_offset: f32,
    column_width: f32,
    padding_block: f32,
    dash_widths: Vec<f32>,
}

/// The widest a transcript row gets before it centres itself, the cap `transcript.rs` applies.
const TRANSCRIPT_MAX_WIDTH: f32 = 768.0;

static SPEC: LazyLock<MinimapSpec> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/minimap.json"
    ))
    .expect("shared minimap geometry")
});

pub(crate) struct MinimapMarker {
    /// The transcript row this dash jumps to.
    item: usize,
    /// The hover card's text, joined once here rather than per frame.
    preview: SharedString,
}

#[derive(Default)]
pub(crate) struct MinimapState {
    markers: Arc<Vec<MinimapMarker>>,
    /// The dash under the pointer, which widens its neighbours and names the turn.
    hovered: Option<usize>,
    /// The column's last measured height, which decides how far apart the dashes sit.
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl MinimapState {
    /// Adopt the rows the host shipped; it sends them only when they changed.
    pub(crate) fn adopt(&mut self, rows: &Value) {
        self.markers = Arc::new(
            rows.as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            let prompt = row["prompt"].as_str().unwrap_or_default();
                            let reply = row["reply"].as_str().unwrap_or_default();
                            MinimapMarker {
                                item: row["item"].as_u64().unwrap_or(0) as usize,
                                preview: SharedString::from(if reply.is_empty() {
                                    prompt.to_owned()
                                } else {
                                    format!("{prompt}\n\n{reply}")
                                }),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
        );
        if self
            .hovered
            .is_some_and(|index| index >= self.markers.len())
        {
            self.hovered = None;
        }
    }
}

impl NativeChatView {
    pub(super) fn minimap_visible(&self) -> bool {
        self.minimap.markers.len() >= SPEC.minimum_turns
    }

    /// The turns whose prompt row is on screen, which the rail draws in the
    /// foreground tone. Following the tail answers from the follow state, because
    /// the list anchors past its last row then and has no bounds to ask.
    fn minimap_in_view(&self, markers: &[MinimapMarker]) -> Range<usize> {
        if markers.is_empty() {
            return 0..0;
        }
        if self.list.is_following_tail() {
            return markers.len() - 1..markers.len();
        }
        let start = markers
            .partition_point(|marker| self.list.item_is_above_viewport(marker.item) == Some(true));
        let mut end = start;
        while end < markers.len()
            && self.list.item_is_below_viewport(markers[end].item) == Some(false)
        {
            end += 1;
        }
        start..end
    }

    /// Scroll the transcript so the chosen prompt starts the viewport, the
    /// `align: 'start'` React's rail used. Aiming at a row before the end also
    /// releases the list's tail follow.
    fn minimap_jump(&mut self, item: usize, cx: &mut Context<Self>) {
        self.list.scroll_to(gpui::ListOffset {
            item_ix: item,
            offset_in_item: px(0.0),
        });
        cx.notify();
    }

    fn minimap_dash_color(&self, distance: usize, in_view: bool, p: &ChatAppearance) -> Hsla {
        // React painted in-view over hovered: both rules had the same weight and
        // the in-view one came last in session-chat-minimap.css.
        if in_view {
            p.foreground.opacity(0.9)
        } else if distance == 0 {
            p.muted.opacity(0.75)
        } else {
            p.muted.opacity(0.35)
        }
    }

    fn render_minimap_column(
        &self,
        p: &ChatAppearance,
        width: Pixels,
        cx: &Context<Self>,
    ) -> AnyElement {
        let markers = self.minimap.markers.clone();
        let s = p.scale * SPEC.scale;
        let in_view = self.minimap_in_view(&markers);
        let hovered = self.minimap.hovered;
        let padding = px(SPEC.padding_block * p.scale);
        let available = (self.minimap.bounds.get().size.height - padding * 2.0)
            .max(px(0.0))
            .as_f32();
        // The rail keeps one `spacing` step per turn until it runs out of room,
        // then it compresses, the way React's `max-height: 100%` rail did.
        let natural = SPEC.spacing * s;
        let step = px(if available > 0.0 {
            natural.min((available / markers.len() as f32).max(1.0))
        } else {
            natural
        });
        let dash_height = px(SPEC.dash_height * s).min(step);
        let bounds = self.minimap.bounds.clone();
        let mut rail = div()
            .relative()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(SPEC.rail_width * s))
            .child(
                // The hairline React drew behind the dashes; visual only.
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(SPEC.line_offset * s))
                    .w(px(1.0))
                    .bg(p.border.opacity(0.15)),
            );
        for (index, marker) in markers.iter().enumerate() {
            let distance = hovered.map_or(usize::MAX, |active| active.abs_diff(index));
            let dash_width = SPEC.dash_widths[distance.min(SPEC.dash_widths.len() - 1)];
            let color = self.minimap_dash_color(distance, in_view.contains(&index), p);
            let item = marker.item;
            let preview = marker.preview.clone();
            rail = rail.child(
                div()
                    .id(("chat-minimap-dash", index))
                    .h(step)
                    .w_full()
                    .flex()
                    .items_center()
                    .chat_cursor_pointer()
                    .on_hover(cx.listener(move |chat, hovered: &bool, _, cx| {
                        let next = hovered.then_some(index);
                        if chat.minimap.hovered != next
                            && (*hovered || chat.minimap.hovered == Some(index))
                        {
                            chat.minimap.hovered = next;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |chat, _, _, cx| chat.minimap_jump(item, cx)))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(preview.clone()).build(window, cx)
                    })
                    .child(
                        div()
                            .h(dash_height)
                            .w(px(dash_width * s))
                            .rounded_full()
                            .bg(color),
                    ),
            );
        }
        div()
            // Hidden until the reader hovers this column, the rule session-chat-minimap.css
            // carries as a user decision. The column keeps its width either way, so
            // revealing the rail never moves the transcript under it.
            .id("chat-minimap")
            .opacity(0.0)
            .hover(|style| style.opacity(1.0))
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .flex_shrink_0()
            .w(width)
            .h_full()
            .overflow_hidden()
            .py(padding)
            .pl(px(SPEC.line_offset * s))
            .child(rail)
            .child(
                gpui::canvas(move |rect, _, _| bounds.set(rect), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .into_any_element()
    }

    /// The transcript, with the rail beside it and a matching gutter opposite, so
    /// the rows stay centered on the composer.
    ///
    /// CDXC:SessionChat 2026-09-18 WHY:
    /// React floated its rail over the transcript, which cost the rows no width. A native overlay
    /// on an interactive region is out (AGENTS.md layout discipline), so the rail is a real column
    /// that may only eat the empty margin beside the centered rows. On a pane too narrow to have
    /// that margin the rail is dropped rather than made to wrap the text earlier than React did.
    pub(super) fn minimap_row(&self, transcript: AnyElement, cx: &Context<Self>) -> AnyElement {
        if !self.minimap_visible() {
            return transcript;
        }
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        let pane = self.bounds.get().size.width;
        let content = p
            .transcript_width
            .map_or(px(TRANSCRIPT_MAX_WIDTH * p.scale), |ratio| pane * ratio);
        let width = px(SPEC.column_width * SPEC.scale * p.scale);
        if (pane - content) / 2.0 < width {
            return transcript;
        }
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(self.render_minimap_column(&p, width, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(transcript),
            )
            .child(div().flex_shrink_0().w(width))
            .into_any_element()
    }
}
