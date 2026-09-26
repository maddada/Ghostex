//! The Subagents strip above the composer: one row per child the provider
//! reports, with its model label, what it is doing, its token counter and its
//! clock. Every value comes from the core's fleet rows
//! (packages/gx-chat-core/src/extras/agent_fleet.rs).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt;
use gpui::{
    AnyElement, Context, FontFeatures, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::{Value, json};
use std::time::Duration;

impl NativeChatView {
    /// The card's fold: the user's last choice, else expanded unless Simple mode.
    fn fleet_open(&self, strip: &Value, p: &ChatAppearance) -> bool {
        strip["openOverride"].as_bool().unwrap_or(!p.simple)
    }

    pub(super) fn render_agent_fleet(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let strip = self.snapshot["agentFleetStrip"].clone();
        if !strip.is_object() {
            return None;
        }
        let s = p.scale;
        let stale = strip["stale"] == true;
        let open = self.fleet_open(&strip, p);
        let motion = self.disclosure_frame("agent-fleet", open, cx);
        let header = self.panel_header(
            super::panel_card::PanelHeader {
                id: "chat-agent-fleet-header",
                // React's `IconUsers`, the two-person glyph, not the three-person group icon.
                icon: "titlebar/users.svg",
                // CDXC:SessionChat 2026-09-07 DECISION: User: the title is "Subagents", without a hyphen or all caps.
                title: "Subagents",
                meta: text(&strip, "countLabel"),
                open,
                has_body: open || motion.is_some(),
                toggle_label: if open {
                    "Minimize subagents"
                } else {
                    "Expand subagents"
                },
                trailing: None,
                command: json!({"type":"toggleAgentFleet","open":!open}),
            },
            p,
            cx,
        );
        let mut body = Vec::new();
        if open || motion.is_some() {
            if stale {
                body.push(
                    div()
                        .id("chat-agent-fleet-unavailable")
                        .role(gpui::Role::Status)
                        .text_size(px(12.0 * s))
                        .text_color(p.card_muted)
                        .child("Subagent status unavailable")
                        .into_any_element(),
                );
            }
            let mut rows = div()
                .id("chat-agent-fleet-rows")
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .max_h(px(180.0 * s))
                .overflow_y_scroll();
            let listed = strip["rows"].as_array().cloned().unwrap_or_default();
            let column = Self::fleet_name_column(&listed, p, cx);
            for row in &listed {
                rows = rows.child(self.fleet_row(row, column, stale, p, cx));
            }
            body.push(rows.into_any_element());
        }
        Some(
            div()
                .id("chat-agent-fleet")
                .role(gpui::Role::Group)
                .aria_label("Subagents")
                .w_full()
                .child(self.status_card_with_header_motion(
                    super::cards::CardBodyMotion {
                        key: "agent-fleet",
                        frame: motion,
                        shut_body: false,
                        shut: !open,
                    },
                    header,
                    body,
                    Vec::new(),
                    p,
                ))
                .into_any_element(),
        )
    }

    /// React laid the strip out as one grid whose model track is `fit-content(10rem)`, so every
    /// task starts at the same x whatever the names above it are. GPUI has no such track, so the
    /// column is measured once from the widest label in the roster and handed to every row.
    fn fleet_name_column(rows: &[Value], p: &ChatAppearance, cx: &Context<Self>) -> gpui::Pixels {
        let size = px(12.0 * p.scale);
        let font = gpui::Font {
            weight: FontWeight::MEDIUM,
            ..gpui::font(p.font.clone())
        };
        let text_system = cx.text_system();
        let font_id = text_system.resolve_font(&font);
        let mut widest = px(0.0);
        for row in rows {
            let width = text(row, "modelLabel")
                .chars()
                .map(|glyph| text_system.layout_width(font_id, size, glyph))
                .fold(px(0.0), |total, advance| total + advance);
            if width > widest {
                widest = width;
            }
        }
        let cap = px(160.0 * p.scale);
        if widest > cap { cap } else { widest }
    }

    fn fleet_row(
        &self,
        row: &Value,
        name_column: gpui::Pixels,
        stale: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        // Both linked cells open the same child transcript, as in React's session-chat-agent-fleet-strip.tsx.
        let open = Self::subagent_open_command(row);
        let key = text(row, "key");
        // CDXC:SessionChat 2026-09-12 DECISION: User: Codex fleet rows show the name/path in the status column instead of the link tooltip.
        let link_type = (row["showAgentType"] == true).then(|| text(row, "agentType"));
        let working = row["working"] == true;
        let idle = row["idle"] == true;
        let hint = |value: String, size: f32| {
            div()
                .flex_shrink_0()
                .text_size(px(size * s))
                .text_color(p.card_muted)
                .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                .child(value)
        };
        let pulse = div()
            .size(px(6.0 * s))
            .rounded_full()
            .flex_shrink_0()
            .bg(if working {
                p.control_primary
            } else {
                p.muted.opacity(0.5)
            });
        let pulse = if working && !crate::app::helpers::gpui_macos_reduce_motion_enabled() {
            pulse
                .with_throttled_animation(
                    "chat-agent-fleet-pulse",
                    Duration::from_millis(1600),
                    |dot, phase| {
                        let progress = if phase < 0.5 {
                            phase * 2.0
                        } else {
                            (1.0 - phase) * 2.0
                        };
                        dot.opacity(1.0 - 0.65 * progress)
                    },
                )
                .into_any_element()
        } else {
            pulse.into_any_element()
        };
        let mut work = div()
            .flex()
            .items_center()
            .gap(px(4.0 * s))
            .flex_1()
            .min_w_0()
            .text_size(px(12.0 * s));
        // CDXC:SessionChat 2026-09-10 DECISION: User: put the ‣ separator at the start of the status cell so it aligns across subagent rows regardless of model label width.
        if row["marker"] == true {
            work = work.child(div().flex_shrink_0().text_color(p.prose).child("‣"));
        }
        if idle && !stale {
            work = work.child(div().flex_shrink_0().text_color(p.card_muted).child("Idle"));
        }
        let status = text(row, "statusText");
        if !status.is_empty() {
            work = work.child(match open.clone() {
                Some(command) => {
                    let mut link = p.clone();
                    // The status cell's link keeps the row's own tone, as its CSS rule does.
                    link.control_primary = p.prose;
                    self.subagent_link(
                        format!("fleet-status:{key}"),
                        status,
                        link_type.clone(),
                        command,
                        &link,
                        cx,
                    )
                }
                None => div()
                    .min_w_0()
                    .truncate()
                    .text_color(p.prose)
                    .child(status)
                    .into_any_element(),
            });
        }
        if let Some(nested) = row["nested"].as_u64().filter(|nested| *nested > 0) {
            let title = text(row, "nestedTitle");
            // React drew the overflow marker as a pill, not bare digits
            // (`.ghostex-chat-agent-fleet-nested`), so it reads as a count of hidden children.
            work = work.child(
                div()
                    .id("chat-agent-fleet-nested")
                    .flex_shrink_0()
                    .px(px(5.0 * s))
                    .py(px(3.0 * s))
                    .rounded_full()
                    .bg(p.foreground.opacity(0.08))
                    .text_size(px(10.0 * s))
                    .text_color(p.card_muted)
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
                    })
                    .child(format!("+{nested}")),
            );
        }
        div()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .w_full()
            .min_w_0()
            .child(pulse)
            // The name cell is the child's transcript link, the model label React rendered inside it.
            .child(
                div()
                    .flex_shrink_0()
                    .w(name_column)
                    .min_w_0()
                    .text_size(px(12.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.foreground)
                    .child(match open {
                        Some(command) => {
                            let mut link = p.clone();
                            link.control_primary = p.foreground;
                            self.subagent_link(
                                format!("fleet-name:{key}"),
                                text(row, "modelLabel"),
                                link_type,
                                command,
                                &link,
                                cx,
                            )
                        }
                        None => div().child(text(row, "modelLabel")).into_any_element(),
                    }),
            )
            .child(work)
            // Counter, separator and clock are three tracks, not one cell: that is what
            // right-aligns every counter on the same edge no matter how long the one above it was.
            .child(hint(text(row, "tokens"), 11.0))
            .child(hint(text(row, "separator"), 11.0))
            .child(hint(text(row, "elapsedLabel"), 11.0))
            .into_any_element()
    }
}
