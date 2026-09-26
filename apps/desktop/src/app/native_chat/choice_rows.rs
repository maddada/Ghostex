use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

impl NativeChatView {
    pub(super) fn question_choice(
        &self,
        id: String,
        label: String,
        description: String,
        selected: bool,
        shortcut: Option<usize>,
        disabled: bool,
        action: Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.choice_row(
            id,
            label,
            description,
            selected,
            shortcut.map(|key| key.to_string()),
            false,
            false,
            disabled,
            action,
            p,
            cx,
        )
    }

    /// `single_line` keeps the label on one line, cut with an ellipsis, and shows the whole label
    /// in a hover tooltip.
    pub(super) fn choice_row(
        &self,
        id: String,
        label: String,
        description: String,
        selected: bool,
        shortcut: Option<String>,
        dense: bool,
        single_line: bool,
        disabled: bool,
        action: Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let tooltip = single_line.then(|| label.clone());
        div()
            .id(id)
            .when(!disabled, |row| row.tab_index(0))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .flex()
            .items_center()
            .gap(px(12.0 * s))
            .w_full()
            .px(px(12.0 * s))
            .py(px(if dense { 6.0 * s } else { 8.0 * s }))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(if selected {
                p.control_primary.opacity(0.3)
            } else {
                p.control_border
            })
            .text_color(p.foreground)
            .when(p.light, |row| row.bg(p.background))
            .when(selected, |row| row.bg(p.control_primary.opacity(0.1)))
            .when(disabled, |row| row.opacity(0.6))
            .when(!disabled, |row| {
                row.chat_cursor_pointer().when(!selected, |row| {
                    row.hover(|style| {
                        style.bg(if p.light {
                            p.input
                        } else {
                            p.input_border.opacity(0.3)
                        })
                    })
                })
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0 * s))
                    .child(
                        div()
                            .text_size(px(14.0 * s))
                            .line_height(px(19.25 * s))
                            .when(single_line, |label| label.truncate())
                            .child(label.clone()),
                    )
                    .when(!description.is_empty() && description != label, |column| {
                        column.child(
                            div()
                                .text_size(px(14.0 * s))
                                .line_height(px(19.25 * s))
                                .text_color(p.card_muted)
                                .child(description),
                        )
                    }),
            )
            .when(selected, |row| {
                row.child(
                    gpui::svg()
                        .path("titlebar/check.svg")
                        .size(px(16.0 * s))
                        .text_color(p.control_primary),
                )
            })
            .when(!selected && shortcut.is_some(), |row| {
                row.child(
                    div()
                        .h(px(20.0 * s))
                        .min_w(px(20.0 * s))
                        .px(px(4.0 * s))
                        .flex_shrink_0()
                        .border_1()
                        .border_color(p.control_border.opacity(0.6))
                        .bg(p.background.opacity(0.4))
                        .rounded(px(4.0 * s))
                        .flex()
                        .justify_center()
                        .items_center()
                        .font_family("Menlo")
                        .text_size(px(13.0 * s))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(p.muted)
                        .child(shortcut.unwrap_or_default()),
                )
            })
            .when_some(tooltip, |row, tooltip| {
                row.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
            .when(!disabled, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(action.clone(), cx);
                }))
            })
            .into_any_element()
    }
}
