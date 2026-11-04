//! A Claude panel read as blocks (server/src/session_chat_claude_panel.rs): tabs, headings,
//! `Label  value` tables, usage meters, text, and code-font text kept as painted.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px, relative,
};
use serde_json::{Value, json};

impl NativeChatView {
    /// SEE-ALSO: apps/mobile/app/src/chat/native/cards/PanelBlocks.tsx draws the same blocks.
    pub(super) fn panel_blocks(
        &self,
        dialog: &Value,
        blocks: &[Value],
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        let dialog_id = text(dialog, "id");
        blocks
            .iter()
            .enumerate()
            .map(|(index, block)| match block["type"].as_str().unwrap_or_default() {
                "tabs" => {
                    let tabs = block["tabs"].as_array().cloned().unwrap_or_default();
                    let selected = tabs.iter().position(|tab| tab["selected"] == true);
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0 * s))
                        .children(tabs.iter().enumerate().map(|(position, tab)| {
                            let active = tab["selected"] == true;
                            let delta = selected.map(|at| position as i64 - at as i64);
                            let id = dialog_id.clone();
                            div()
                                .id(SharedString::from(format!("panel-tab:{position}")))
                                .role(gpui::Role::Tab)
                                .aria_label(text(tab, "label"))
                                .px(px(10.0 * s))
                                .py(px(3.0 * s))
                                .rounded(px(6.0 * s))
                                .text_size(px(13.0 * s))
                                // CDXC:SessionChat 2026-10-25 DECISION: User (screenshot of the `/help` panel, its selected "General" tab a white pill with white text): "please make the button not like this". The selected tab is the hover look kept on (a light fill under the normal text colour) instead of a solid foreground pill labelled in the card colour, which is see-through under a translucent or background-image theme. The phone's PanelBlocks.tsx matches.
                                .when(active, |tab| {
                                    tab.bg(p.border)
                                        .text_color(p.foreground)
                                        .font_weight(FontWeight::MEDIUM)
                                })
                                .when(!active, |tab| {
                                    tab.text_color(p.muted)
                                        .chat_cursor_pointer()
                                        .hover(|style| style.bg(p.border).text_color(p.foreground))
                                })
                                .child(text(tab, "label"))
                                .when_some(delta.filter(|delta| *delta != 0), |tab, delta| {
                                    tab.on_click(cx.listener(move |this, _, _, cx| {
                                        this.invoke(
                                            json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":id,"dialogAction":"selectTab","tabDelta":delta}}),
                                            cx,
                                        );
                                    }))
                                })
                        }))
                        .into_any_element()
                }
                "heading" => div()
                    .pt(px(if index == 0 { 0.0 } else { 4.0 * s }))
                    .text_size(px(13.0 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(text(block, "text"))
                    .into_any_element(),
                "table" => div()
                    .flex()
                    .flex_col()
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(p.border)
                    .overflow_hidden()
                    .text_size(px(13.0 * s))
                    .children(
                        block["rows"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                            .map(|(row_index, row)| {
                                div()
                                    .flex()
                                    .gap(px(12.0 * s))
                                    .px(px(10.0 * s))
                                    .py(px(5.0 * s))
                                    .when(row_index > 0, |row| {
                                        row.border_t_1().border_color(p.border.opacity(0.6))
                                    })
                                    .child(
                                        div()
                                            .w(relative(0.34))
                                            .flex_shrink_0()
                                            .text_color(p.muted)
                                            .child(text(row, "key")),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_color(p.foreground)
                                            .child(text(row, "value")),
                                    )
                            }),
                    )
                    .into_any_element(),
                "meter" => {
                    let percent = block["percent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0) as f32;
                    let label = text(block, "label");
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0 * s))
                        .text_size(px(13.0 * s))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .gap(px(12.0 * s))
                                .child(
                                    div().text_color(p.foreground).child(if label.is_empty() {
                                        text(block, "value")
                                    } else {
                                        format!("{label} · {}", text(block, "value"))
                                    }),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0 * s))
                                        .text_color(p.muted)
                                        .child(text(block, "detail")),
                                ),
                        )
                        .child(
                            div()
                                .h(px(6.0 * s))
                                .w_full()
                                .rounded(px(3.0 * s))
                                .bg(p.muted.opacity(0.22))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(percent / 100.0))
                                        .rounded(px(3.0 * s))
                                        .bg(p.primary),
                                ),
                        )
                        .into_any_element()
                }
                "pre" => div()
                    .id(SharedString::from(format!("panel-pre:{dialog_id}:{index}")))
                    .rounded(px(8.0 * s))
                    .p(px(10.0 * s))
                    .bg(p.border.opacity(0.3))
                    .overflow_x_scroll()
                    .font_family(super::fonts::CHAT_MONO)
                    .text_size(px(12.0 * s))
                    .line_height(px(19.0 * s))
                    .whitespace_nowrap()
                    .child(
                        div().flex().flex_col().children(
                            text(block, "text")
                                .lines()
                                .map(|line| div().child(line.to_string()))
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .into_any_element(),
                _ => div()
                    .text_size(px(13.0 * s))
                    .line_height(relative(1.45))
                    .text_color(if block["muted"] == true { p.muted } else { p.foreground })
                    .child(text(block, "text"))
                    .into_any_element(),
            })
            .collect()
    }
}
