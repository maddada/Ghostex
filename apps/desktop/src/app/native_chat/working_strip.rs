use super::{
    appearance::ChatAppearance,
    state::NativeChatView,
    working_spark::{css_ease_in_out, spark},
};
use crate::app::helpers::ThrottledAnimationExt;
use crate::assets::chat_working::VISUAL;
use gpui::StatefulInteractiveElement as _;
use gpui::{
    AnyElement, FontFeatures, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled,
    div, px, relative, svg,
};
use std::time::Duration;

impl NativeChatView {
    pub(crate) fn render_working_strip(
        &self,
        p: &ChatAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let status = &self.snapshot["workingStrip"];
        let reduced_motion = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        let armed = self.armed_action_items(p, cx);
        if status["presentation"].is_object() {
            let activity = self.render_working_activity(p, reduced_motion);
            if armed.is_empty() {
                return Some(activity);
            }
            return Some(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.0 * p.scale))
                    .child(activity)
                    .child(self.working_row(None, armed, p, reduced_motion))
                    .into_any_element(),
            );
        }
        let label = status["label"].as_str();
        if label.is_none() && armed.is_empty() {
            return None;
        }
        Some(self.working_row(label, armed, p, reduced_motion))
    }

    /// The working row: spark and word at the left (when working), armed actions pushed right by the
    /// lead's auto margin; items that do not fit wrap onto a left-aligned second line.
    fn working_row(
        &self,
        label: Option<&str>,
        armed: Vec<AnyElement>,
        p: &ChatAppearance,
        reduced_motion: bool,
    ) -> AnyElement {
        let s = p.scale;
        let armed_labels = self.armed_actions.as_array().into_iter().flatten();
        let aria = label
            .into_iter()
            .chain(armed_labels.filter_map(|action| action["label"].as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut lead = div()
            .flex()
            .items_center()
            .gap(px(VISUAL.gap * s))
            .min_w_0()
            .mr_auto();
        if let Some(label) = label {
            lead = lead.child(spark(p, reduced_motion)).child(
                div()
                    .min_w_0()
                    .text_size(px(VISUAL.font_size * s))
                    .line_height(relative(1.5))
                    .text_color(p.muted)
                    .truncate()
                    .child(label.to_owned()),
            );
        }
        div()
            .id("chat-working-strip")
            .role(gpui::Role::Status)
            .aria_label(aria)
            .w_full()
            .min_w_0()
            .min_h(px(VISUAL.min_height * s))
            .px(px(VISUAL.padding_x * s))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(VISUAL.armed_column_gap * s))
            .gap_y(px(VISUAL.armed_row_gap * s))
            .child(lead)
            .children(armed)
            .into_any_element()
    }

    /// CDXC:DelayedSend 2026-09-21 DECISION:
    /// User: clicking an armed Delayed Send or Close After Done indicator on the chat working row opens the Delayed Actions modal so they can be managed there.
    fn armed_action_items(
        &self,
        p: &ChatAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        self.armed_actions
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| {
                let color = VISUAL.armed_color(action["id"].as_str()?)?;
                let label = action["label"].as_str()?.to_owned();
                let id = action["id"].as_str()?.to_owned();
                Some(
                    div()
                        .id(format!("chat-armed-{id}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("{label}. Manage delayed actions"))
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.8))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.host(
                                "delayedActions",
                                serde_json::json!({"type": "host", "action": "delayedActions"}),
                                cx,
                            );
                        }))
                        .flex()
                        .items_center()
                        .gap(px(VISUAL.armed_icon_gap * s))
                        .min_w_0()
                        .child(
                            div()
                                .size(px(VISUAL.spark_box * s))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg()
                                        .path("titlebar/clock.svg")
                                        .size(px(VISUAL.spark_size * s))
                                        .text_color(color),
                                ),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(VISUAL.font_size * s))
                                .line_height(relative(1.5))
                                .text_color(p.foreground)
                                .child(label),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }

    fn render_working_activity(&self, p: &ChatAppearance, reduced_motion: bool) -> AnyElement {
        let activity = &self.snapshot["workingStrip"]["presentation"];
        let s = p.scale;
        let lead = if activity["shellsRunning"] == true {
            let glyph = svg()
                .path("titlebar/loader2.svg")
                .size(px(14.0 * s))
                .text_color(p.control_primary);
            if reduced_motion {
                glyph.into_any_element()
            } else {
                glyph
                    .with_throttled_animation(
                        "chat-shells-running",
                        Duration::from_secs(1),
                        |glyph, progress| {
                            glyph.with_transformation(gpui::Transformation::rotate(
                                gpui::percentage(progress),
                            ))
                        },
                    )
                    .into_any_element()
            }
        } else {
            let dot = div().size(px(6.0 * s)).rounded_full().bg(p.control_primary);
            if reduced_motion {
                dot.into_any_element()
            } else {
                dot.with_throttled_animation(
                    "chat-activity-dot",
                    Duration::from_millis(1600),
                    |dot, phase| {
                        let progress = css_ease_in_out(if phase < 0.5 {
                            phase * 2.0
                        } else {
                            (1.0 - phase) * 2.0
                        });
                        dot.opacity(1.0 - 0.65 * progress)
                    },
                )
                .into_any_element()
            }
        };
        let mut title = div()
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .text_color(p.foreground)
            .child(activity["label"].as_str().unwrap_or_default().to_owned());
        // CDXC:SessionChat 2026-09-11 DECISION: User: put the compaction hint in an info-circle tooltip immediately right of the title, replacing the visible hint line.
        if let Some(hint) = activity["hint"].as_str() {
            let hint = hint.to_owned();
            title = title.child(
                div()
                    .id("compaction-messaging-hint")
                    .role(gpui::Role::Button)
                    .aria_label("Messaging during compaction")
                    .ml(px(6.0 * s))
                    .size(px(20.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
                    })
                    .child(
                        svg()
                            .path("titlebar/info-circle.svg")
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    ),
            );
        }
        let mut trailing = div()
            .h(px(22.75 * s))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .flex_shrink_0()
            .text_size(px(14.0 * s))
            .text_color(p.muted)
            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()));
        if let Some(elapsed) = activity["elapsedLabel"].as_str() {
            trailing = trailing.child(elapsed.to_owned());
        }
        let percent = activity["percent"].as_f64();
        if let Some(percent) = percent {
            trailing = trailing.child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.foreground.opacity(0.8))
                    .child(format!("{percent}%")),
            );
        }
        let header = div()
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .text_size(px(14.0 * s))
            .line_height(relative(1.625))
            .child(
                div()
                    .h(px(22.75 * s))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .child(lead),
            )
            .child(title)
            .child(trailing)
            .into_any_element();
        let mut body = Vec::new();
        let primary = p.control_primary;
        if percent.is_some() || activity["indeterminate"] == true {
            let track = div()
                .relative()
                .h(px(4.0 * s))
                .w_full()
                .rounded_full()
                .overflow_hidden()
                .bg(p.foreground.opacity(0.1));
            let fill = div().h_full().rounded_full().bg(primary);
            let track = if let Some(percent) = percent {
                track
                    .child(fill.w(relative(percent as f32 / 100.0)))
                    .into_any_element()
            } else if reduced_motion {
                track
                    .flex()
                    .justify_center()
                    .child(fill.w(relative(0.35)))
                    .into_any_element()
            } else {
                track
                    .with_throttled_animation(
                        "chat-compaction-progress",
                        Duration::from_millis(1800),
                        move |track, phase| {
                            track.child(
                                div()
                                    .absolute()
                                    .h_full()
                                    .rounded_full()
                                    .bg(primary)
                                    .w(relative(0.35))
                                    .left(relative(-0.35 + 1.351 * css_ease_in_out(phase))),
                            )
                        },
                    )
                    .into_any_element()
            };
            body.push(track);
        }
        div()
            .id("chat-working-activity")
            .role(gpui::Role::Status)
            .w_full()
            .child(self.status_card_with_header(header, body, Vec::new(), p))
            .into_any_element()
    }
}
