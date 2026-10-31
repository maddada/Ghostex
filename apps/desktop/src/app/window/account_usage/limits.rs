use super::{data::*, panel::AccountUsagePanel, style::*};
use crate::*;
use serde_json::Value;

impl AccountUsagePanel {
    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User: Claude's Fable limit must stay visible beside the five-hour and weekly limits; additional models start collapsed.
    /// SEE-ALSO: packages/shared/account-usage-windows.ts (deleted 2026-10-01) and apps/desktop/src/app/titlebar/account_usage.rs select the two tightest headline limits.
    pub(super) fn render_limits(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let p = self.palette;
        let windows = array(&self.account, "usage");
        let main: Vec<_> = windows
            .iter()
            .filter(|w| w["model"].is_null() && w["id"] != "spend")
            .collect();
        let session = main
            .iter()
            .copied()
            .find(|w| w["id"] == "fiveHour" || w["limitWindowSeconds"] == 18000);
        let weekly = main.iter().copied().find(|w| {
            w["id"] == "sevenDay" || w["limitWindowSeconds"].as_i64().unwrap_or(0) >= 604800
        });
        let scoped: Vec<_> = windows
            .iter()
            .filter(|w| !text(w, "model").is_empty())
            .collect();
        let fable = (!p.codex)
            .then(|| {
                scoped
                    .iter()
                    .copied()
                    .find(|w| text(w, "model").to_lowercase().contains("fable"))
                    .or_else(|| scoped.first().copied())
            })
            .flatten();
        let other: Vec<_> = windows
            .iter()
            .filter(|w| {
                w["id"] != "spend"
                    && ![session, weekly, fable]
                        .into_iter()
                        .flatten()
                        .any(|selected| std::ptr::eq(*w, selected))
            })
            .collect();
        let mut bars = v_flex()
            .w_full()
            .gap(px(LIMIT_GAP))
            .child(self.render_bar(session, "5h limit".into(), 13.0))
            .child(self.render_bar(weekly, "7d limit".into(), 13.0));
        if let Some(fable) = fable {
            bars = bars.child(self.render_bar(
                Some(fable),
                format!("{} 7d limit", text(fable, "model")),
                13.0,
            ));
        }
        let mut card = p.card().child(bars);
        if !other.is_empty() {
            let mut models = v_flex().w_full().mt(px(LIMIT_GAP));
            models = models.child(
                h_flex()
                    .id("account-usage-models-toggle")
                    .track_focus(&self.model_focus)
                    .focus_visible(|this| this.shadow(focus_ring()))
                    .items_center()
                    .self_start()
                    .gap(px(6.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(move |this| this.text_color(p.ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_models(cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                            this.toggle_models(cx);
                        }
                    }))
                    .child(chevron(self.models_open, false, p.muted))
                    .child(format!("More model limits ({})", other.len())),
            );
            if self.models_open {
                models =
                    models.child(
                        v_flex()
                            .w_full()
                            .mt(px(LIMIT_GAP))
                            .pl(px(12.0))
                            .border_l_1()
                            .border_color(p.line)
                            .gap(px(LIMIT_GAP))
                            .children(other.into_iter().map(|w| {
                                self.render_bar(Some(w), text(w, "label").to_string(), 12.0)
                            })),
                    );
            }
            card = card.child(models);
        }
        let row = |title: &'static str| {
            h_flex()
                .w_full()
                .min_h(px(KV_ROW_HEIGHT))
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .text_size(px(12.5))
                .text_color(p.muted)
                .child(title)
        };
        // CDXC:AgentProviders 2026-09-24 DECISION:
        // User: Claude resets must be shown and usable like Codex resets. Claude keeps its Extra usage row and gains the same Rate limit resets row beneath it once Anthropic reports the reset program for the account.
        let reports_resets = p.codex
            || self.account["resetCredits"].is_u64()
            || !text(&self.account, "resetCreditsError").is_empty();
        if !p.codex || reports_resets {
            card = card.child(separator(p));
        }
        if !p.codex {
            let extra = windows
                .iter()
                .find(|w| w["id"] == "spend")
                .and_then(|w| w["usedPercent"].as_f64())
                .map(|n| format!("{}% used", n.round()))
                .unwrap_or_else(|| "Not reported".into());
            card = card.child(row("Extra usage").child(label(
                extra,
                12.5,
                chrome_ink().opacity(0.80).into(),
            )));
        }
        if reports_resets {
            let resets_text = self.account["resetCredits"]
                .as_u64()
                .map(|n| format!("{n} available"))
                .unwrap_or_else(|| "Not reported".into());
            card = card.child(
                row("Rate limit resets").child(
                    p.control("account-usage-reset-toggle")
                        .track_focus(&self.reset_focus)
                        .focus_visible(|this| this.shadow(focus_ring()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_resets(cx);
                        }))
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                this.toggle_resets(cx);
                            }
                        }))
                        .child(resets_text)
                        .child(chevron(
                            true,
                            self.resets_open,
                            chrome_ink().opacity(0.70).into(),
                        )),
                ),
            );
            if self.resets_open {
                card = card.child(self.render_resets(cx));
            }
        }
        card.into_any_element()
    }

    fn render_bar(&self, bar: Option<&Value>, title: String, label_size: f32) -> AnyElement {
        let p = self.palette;
        let bar = bar.unwrap_or(&Value::Null);
        let percent = bar["usedPercent"].as_f64();
        let warning = if p.codex {
            pace_warning(bar, self.now)
        } else {
            String::new()
        };
        let reset = text(bar, "resetsAt");
        let reset = if reset.is_empty() {
            String::new()
        } else {
            reset_text(reset, self.now)
        };
        let mut meta = h_flex()
            .flex_shrink_0()
            .items_baseline()
            .whitespace_nowrap()
            .font_features(tabular_numbers())
            .text_size(px(12.0))
            .line_height(px(LIMIT_LINE))
            .text_color(p.muted);
        if let Some(percent) = percent {
            meta = meta
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.ink)
                        .child(format!("{}%", percent.round())),
                )
                .when(!reset.is_empty(), |this| {
                    this.child(div().ml(px(4.0)).child(format!("\u{b7} {reset}")))
                });
        } else {
            meta = meta.child(if reset.is_empty() {
                "Not reported".into()
            } else {
                reset
            });
        }
        // Red near the limit and amber when it is close or on pace to run out; the provider's own
        // colour otherwise.
        let fill: Hsla = if percent.unwrap_or(0.0) >= 95.0 {
            rgb(0xe5534b).into()
        } else if !warning.is_empty() || percent.unwrap_or(0.0) >= 80.0 {
            amber()
        } else {
            p.accent
        };
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(
                h_flex()
                    .w_full()
                    .h(px(LIMIT_LINE))
                    .items_baseline()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        label(title, label_size, p.ink)
                            .line_height(px(LIMIT_LINE))
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis(),
                    )
                    .child(meta),
            )
            .child(
                div()
                    .w_full()
                    .mt(px(LIMIT_BAR_GAP))
                    .h(px(LIMIT_BAR_HEIGHT))
                    .rounded(px(2.0))
                    .overflow_hidden()
                    .bg(p.track)
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(
                                (percent.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0) as f32,
                            ))
                            .rounded(px(2.0))
                            .bg(fill),
                    ),
            )
            .when(!warning.is_empty(), |this| {
                this.child(
                    h_flex()
                        .mt(px(6.0))
                        .items_center()
                        .gap(px(5.0))
                        .text_size(px(11.0))
                        .line_height(px(WARNING_LINE))
                        .text_color(amber())
                        .child(
                            div()
                                .size(px(6.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(amber()),
                        )
                        .child(warning),
                )
            })
            .into_any_element()
    }
}

/// The hairline between the limits and the rows under them.
fn separator(p: Palette) -> gpui::Div {
    div()
        .w_full()
        .h(px(1.0))
        .my(px(SEPARATOR_MARGIN))
        .bg(p.line)
}

/// A limit's label line, the bar below it, and the gap between limits.
pub(super) const LIMIT_LINE: f32 = 17.0;
pub(super) const LIMIT_BAR_GAP: f32 = 5.0;
pub(super) const LIMIT_BAR_HEIGHT: f32 = 4.0;
pub(super) const LIMIT_GAP: f32 = 10.0;
pub(super) const WARNING_LINE: f32 = 16.0;
pub(super) const SEPARATOR_MARGIN: f32 = 10.0;
pub(super) const KV_ROW_HEIGHT: f32 = 24.0;
