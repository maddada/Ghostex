//! The "+N" pill at the end of the pills row (pills_strip.rs): pills the row has no room for, even
//! with their labels dropped, wait in its list instead of being scrolled or faded out of sight.
//! A row of that list does what the pill itself would (`activate_chat_pill`).
//!
//! CDXC:SessionChat 2026-11-10 DECISION:
//! User, of the pills above the chat box (Board and Autopilot faded at the two edges of a row that
//! scrolled sideways since the pills got bigger): "the pills hiding is still bad, please make this
//! simpler". No edge fade and no sideways scroll: every pill stays whole. When the row does not fit,
//! pills drop their label from the right, least used first, and show icon and count (the tooltip
//! names them); only if even that does not fit, the last ones go into one "+N" pill that opens a
//! list of them. The phone matches in PillsStrip.tsx.

use super::chat_pills::{inline_dots, pill_dot_color, pill_menu_column, pill_menu_header};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};

/// The list's id among the pill menus, and its trigger's.
pub(super) const OVERFLOW_MENU: &str = "stripOverflow";
pub(super) const OVERFLOW_TRIGGER: &str = "chat-pill-overflow";
pub(super) const OVERFLOW_MENU_WIDTH: f32 = 300.0;

/// One row and the gap between two, unscaled: fixed, so the menu knows its height before it opens.
const ROW_HEIGHT: f32 = 28.0;
const ROW_GAP: f32 = 2.0;

impl NativeChatView {
    /// The "+N" pill, drawn like the panel pills (chat_pills.rs `chat_pill`).
    pub(super) fn strip_overflow_pill(
        &self,
        count: usize,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let open = self.chat_menu_is_open(OVERFLOW_TRIGGER);
        let bounds = self.pill_bounds.cell(OVERFLOW_MENU);
        let measured = bounds.clone();
        let label = format!("{count} more pills");
        let tooltip = label.clone();
        div()
            .id(OVERFLOW_TRIGGER)
            .relative()
            .role(gpui::Role::Button)
            .aria_label(label)
            .aria_expanded(open)
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(5.0 * s))
            .px(px(8.0 * s))
            .py(px(2.0 * s))
            .rounded_full()
            .border(px(s))
            .border_color(p.foreground.opacity(0.1))
            .text_size(px(13.0 * s))
            .line_height(px(19.0 * s))
            .whitespace_nowrap()
            .text_color(p.foreground)
            .chat_cursor_pointer()
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(
                svg()
                    .path("titlebar/dots.svg")
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.card_muted),
            )
            .child(
                div()
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .child(format!("+{count}")),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_pill_menu(OVERFLOW_MENU, bounds.get(), window, cx);
            }))
            .child(
                canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset(px(-s)),
            )
            .into_any_element()
    }

    /// The list's content height (unscaled), or `None` once every pill fits the row again.
    pub(super) fn strip_overflow_height(&self) -> Option<f32> {
        let rows = self.pills_strip.hidden.borrow().len();
        (rows > 0).then(|| {
            super::chat_pills::MENU_HEADER_HEIGHT
                + super::chat_pills::MENU_SECTION_GAP
                + rows as f32 * ROW_HEIGHT
                + (rows - 1) as f32 * ROW_GAP
        })
    }

    /// The list's body: one row per hidden pill, with its icon, name and count or dots.
    pub(super) fn strip_overflow_body(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let hidden = self.pills_strip.hidden.borrow().clone();
        if hidden.is_empty() {
            return None;
        }
        let s = p.scale;
        let header = pill_menu_header(
            "titlebar/dots.svg",
            "More pills",
            format!("{} hidden", hidden.len()),
            None,
            p,
        );
        let mut rows = div()
            .id("chat-strip-overflow-rows")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(ROW_GAP * s));
        for pill in &hidden {
            let id = text(pill, "id");
            let switch = pill["switch"].as_bool().is_some();
            let name = Some(text(pill, "label"))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| text(pill, "ariaLabel"));
            let dots: Vec<(gpui::Hsla, String)> = pill["dots"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|dot| (pill_dot_color(&text(dot, "tone")), text(dot, "count")))
                .collect();
            let count = if dots.is_empty() {
                text(pill, "count")
            } else {
                String::new()
            };
            let row_id = id.clone();
            rows = rows.child(
                div()
                    .id(gpui::ElementId::Name(
                        format!("chat-strip-overflow:{id}").into(),
                    ))
                    .role(gpui::Role::Button)
                    .aria_label(text(pill, "ariaLabel"))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .w_full()
                    .min_w_0()
                    .flex_shrink_0()
                    .h(px(ROW_HEIGHT * s))
                    .px(px(4.0 * s))
                    .rounded(px(5.0 * s))
                    .text_size(px(13.0 * s))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.06)))
                    .child(
                        svg()
                            .path(super::chat_pills::pill_icon(&id))
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(p.card_muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(p.foreground)
                            .child(name),
                    )
                    .children((!dots.is_empty()).then(|| inline_dots(&dots, p)))
                    .when(!count.is_empty(), |row| {
                        row.child(
                            div()
                                .text_color(p.card_muted)
                                .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                                .child(count),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        // The pill is not painted, so its menu lines up with the "+N" pill instead.
                        let at = this.pill_bounds.cell(OVERFLOW_MENU).get();
                        this.pill_bounds.cell(&row_id).set(at);
                        this.close_pill_menu(cx);
                        this.activate_chat_pill(&row_id, switch, at, window, cx);
                    })),
            );
        }
        Some(pill_menu_column(
            "chat-strip-overflow",
            "More pills",
            header,
            Some(rows.into_any_element()),
            None,
            p,
        ))
    }
}
