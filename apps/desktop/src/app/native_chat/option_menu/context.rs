use super::window::ChatOptionMenuPanel;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

pub(super) fn height(
    context: &Value,
    width: f32,
    appearance: &super::super::appearance::ChatAppearance,
    cx: &gpui::App,
) -> anyhow::Result<f32> {
    let scale = appearance.scale;
    let system = gpui::WindowTextSystem::new(cx.text_system().clone());
    let font = gpui::font(appearance.font.clone());
    let mut height = 12.0 + 16.0 + 8.0 + 16.5 + 8.0 + 4.0 + 24.0;
    if context["usedPercentage"].is_number() {
        height += 14.0;
    }
    if let Some(groups) = context["details"].as_array() {
        height += 8.0 + 4.0 + 1.0 + 8.0 + 24.0;
        if groups.is_empty() {
            height += 20.5;
        }
        for group in groups {
            height += 24.0;
            for item in group["items"].as_array().into_iter().flatten() {
                let label = item["label"].as_str().unwrap_or_default();
                let value = item["value"].as_str().unwrap_or_default();
                let run = |text: &str| gpui::TextRun {
                    len: text.len(),
                    font: font.clone(),
                    ..Default::default()
                };
                let label_width = system
                    .shape_line(
                        label.to_owned().into(),
                        px(11.0 * scale),
                        &[run(label)],
                        None,
                    )
                    .width
                    .as_f32()
                    / scale;
                let lines = system.shape_text(
                    value.to_owned().into(),
                    px(11.0 * scale),
                    &[run(value)],
                    Some(px((width - 26.0 - label_width - 10.0).max(1.0) * scale)),
                    None,
                )?;
                height += lines
                    .iter()
                    .map(|line| line.size(px(16.0 * scale)).height.as_f32() / scale)
                    .sum::<f32>()
                    .max(16.0);
            }
        }
    }
    Ok(height)
}

impl ChatOptionMenuPanel {
    pub(super) fn render_context(&self, context: &Value, cx: &Context<Self>) -> AnyElement {
        let appearance = &self.menu.read(cx).appearance;
        let scale = appearance.scale;
        let muted = appearance.muted;
        let text = |key: &str| context[key].as_str().unwrap_or_default().to_owned();
        let mut content = div()
            .flex_shrink_0()
            .px(px(6.0 * scale))
            .py(px(6.0 * scale))
            .flex()
            .flex_col()
            .gap(px(8.0 * scale))
            .text_size(px(11.0 * scale))
            .line_height(px(16.5 * scale))
            .text_color(muted)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0 * scale))
                    .h(px(16.0 * scale))
                    .child(
                        div()
                            .text_size(px(12.0 * scale))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child("Context window"),
                    )
                    .child(text("summary")),
            );
        if let Some(percentage) = context["usedPercentage"].as_f64() {
            content = content.child(
                div()
                    .h(px(6.0 * scale))
                    .w_full()
                    .rounded_full()
                    .overflow_hidden()
                    .bg(appearance.muted.opacity(0.24))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative((percentage / 100.0).clamp(0.0, 1.0) as f32))
                            .rounded_full()
                            .bg(gpui::rgb(0xb9b9b9)),
                    ),
            );
        }
        let disabled = context["compactDisabled"] == true;
        let reason = text("compactDisabledReason");
        content = content
            .child(
                div()
                    .h(px(16.5 * scale))
                    .child("Compacts automatically as the window fills."),
            )
            .child(
                div()
                    .id("compact-context")
                    .when(self.selected == Some(0), |item| item.bg(appearance.border))
                    .role(gpui::Role::Button)
                    .aria_label("Compact context")
                    .mt(px(4.0 * scale))
                    .h(px(24.0 * scale))
                    .text_size(px(12.0 * scale))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0 * scale))
                    .border_1()
                    .border_color(appearance.border)
                    .text_color(appearance.primary)
                    .when(disabled, |item| item.opacity(0.5))
                    .when(!disabled, |item| {
                        item.chat_cursor_pointer()
                            .hover(|style| style.bg(appearance.border))
                    })
                    // CDXC:SessionChat 2026-09-26 DECISION: User: a button whose static text already says what the tooltip would say gets no tooltip; only the disabled button keeps one, to say why.
                    .when(disabled, |item| {
                        item.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(reason.clone()).build(window, cx)
                        })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !disabled {
                            this.menu.update(cx, |menu, cx| {
                                menu.close(Some(json!({"type":"contextCompact"})), cx)
                            });
                        }
                    }))
                    .child("Compact context"),
            );
        if let Some(groups) = context["details"].as_array() {
            let mut details = div()
                .mt(px(4.0 * scale))
                .pt(px(8.0 * scale))
                .border_t_1()
                .border_color(appearance.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .h(px(24.0 * scale))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child("More details"),
                        )
                        .child(
                            div()
                                .id("edit-context-details")
                                .when(self.selected == Some(1), |item| item.bg(appearance.border))
                                .role(gpui::Role::Button)
                                .aria_label("Choose which details to show")
                                .size(px(24.0 * scale))
                                .flex()
                                .items_center()
                                .justify_center()
                                .chat_cursor_pointer()
                                .child(
                                    svg()
                                        .path("titlebar/pencil.svg")
                                        .size(px(12.0 * scale))
                                        .text_color(muted),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.menu.update(cx, |menu, cx| {
                                        menu.close(Some(json!({"type":"contextEdit"})), cx)
                                    })
                                })),
                        ),
                );
            if groups.is_empty() {
                details = details.child(div().h(px(20.0 * scale)).child("Nothing selected."));
            }
            for group in groups {
                details = details.child(
                    div()
                        .pt(px(8.0 * scale))
                        .pb(px(2.0 * scale))
                        .h(px(24.0 * scale))
                        .text_size(px(9.5 * scale))
                        .line_height(px(14.0 * scale))
                        .text_color(muted.opacity(0.7))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(group["label"].as_str().unwrap_or_default().to_uppercase()),
                );
                for item in group["items"].as_array().into_iter().flatten() {
                    details = details.child(
                        div()
                            .min_h(px(16.0 * scale))
                            .line_height(px(16.0 * scale))
                            .flex()
                            .justify_between()
                            .gap(px(10.0 * scale))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(muted.opacity(0.75))
                                    .child(item["label"].as_str().unwrap_or_default().to_owned()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_align(gpui::TextAlign::Right)
                                    .text_color(muted)
                                    .child(item["value"].as_str().unwrap_or_default().to_owned()),
                            ),
                    );
                }
            }
            content = content.child(details);
        }
        content.into_any_element()
    }
}
