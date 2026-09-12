use super::{data::*, panel::AccountUsagePanel, style::*};
use crate::*;
use serde_json::Value;

impl AccountUsagePanel {
    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User: Claude's Fable limit must stay visible beside the five-hour and weekly limits; additional models start collapsed.
    /// SEE-ALSO: packages/shared/account-usage-windows.ts and apps/desktop/src/app/titlebar/account_usage.rs select the two tightest headline limits.
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
            .gap(px(13.0))
            .child(self.render_bar(session, "5-hour limit".into(), 12.0))
            .child(self.render_bar(weekly, "Weekly limit".into(), 12.0));
        if let Some(fable) = fable {
            bars = bars.child(self.render_bar(
                Some(fable),
                format!("{} weekly limit", text(fable, "model")),
                12.0,
            ));
        }
        let mut card = p
            .card()
            .child(
                section_heading().child(heading("This account")).child(
                    p.tag(true)
                        .child(div().size(px(6.0)).rounded_full().bg(p.accent).shadow(vec![
                            shadow(0.0, 0.0, 0.0, p.accent.opacity(0.25)).spread_radius(px(2.0)),
                            shadow(0.0, 0.0, 8.0, p.accent),
                        ]))
                        .child("Live limits"),
                ),
            )
            .child(bars);
        if !other.is_empty() {
            let mut models = v_flex().w_full().mt(px(12.0));
            models = models.child(
                h_flex()
                    .id("account-usage-models-toggle")
                    .track_focus(&self.model_focus)
                    .focus_visible(move |this| this.shadow(focus_ring(p)))
                    .items_center()
                    .self_start()
                    .gap(px(6.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .line_height(px(15.95))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(|this| this.text_color(rgb(0xe4e4e2)))
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
                            .mt(px(12.0))
                            .pl(px(14.0))
                            .border_l_2()
                            .border_color(p.strong)
                            .gap(px(13.0))
                            .children(other.into_iter().map(|w| {
                                self.render_bar(Some(w), text(w, "label").to_string(), 11.0)
                            })),
                    );
            }
            card = card.child(models);
        }
        let extra = windows.iter().find(|w| w["id"] == "spend");
        let extra_text = if p.codex {
            self.account["resetCredits"]
                .as_u64()
                .map(|n| format!("{n} available"))
        } else {
            extra
                .and_then(|w| w["usedPercent"].as_f64())
                .map(|n| format!("{}% used", n.round()))
        }
        .unwrap_or_else(|| "Not reported".into());
        let mut footer = h_flex()
            .w_full()
            .mt(px(13.0))
            .pt(px(11.0))
            .border_t_1()
            .border_color(p.line)
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .text_size(px(11.0))
            .line_height(px(15.95))
            .text_color(p.muted)
            .child(if p.codex {
                "Rate limit resets"
            } else {
                "Extra usage"
            });
        if p.codex {
            footer = footer.child(
                h_flex()
                    .id("account-usage-reset-toggle")
                    .track_focus(&self.reset_focus)
                    .focus_visible(move |this| this.shadow(focus_ring(p)))
                    .h(px(22.0))
                    .pl(px(9.0))
                    .pr(px(4.0))
                    .items_center()
                    .gap(px(6.0))
                    .rounded_full()
                    .border_1()
                    .border_color(p.accent_line)
                    .bg(p.soft)
                    .text_color(p.accent)
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(move |this| this.bg(p.accent.opacity(0.22)))
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
                    .child(extra_text)
                    .child(chevron(true, self.resets_open, p.accent)),
            );
        } else {
            footer = footer.child(
                label(extra_text, 11.0, rgb(0xe4e4e2).into()).font_weight(FontWeight::MEDIUM),
            );
        }
        card = card.child(footer);
        if p.codex && self.resets_open {
            card = card.child(self.render_resets(cx));
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
            .text_size(px(10.5))
            .line_height(px(15.225))
            .text_color(p.muted);
        if let Some(percent) = percent {
            meta = meta
                .child(
                    label(format!("{}%", percent.round()), 11.5, rgb(0xf2f2f0).into())
                        .font_weight(FontWeight::SEMIBOLD)
                        .mr(px(1.0)),
                )
                .when(!reset.is_empty(), |this| this.child(format!(" · {reset}")));
        } else {
            meta = meta.child(if reset.is_empty() {
                "Not reported".into()
            } else {
                reset
            });
        }
        let (from, to, glow) = if percent.unwrap_or(0.0) >= 95.0 {
            (
                rgb(0xd9463c).into(),
                rgb(0xf5817a).into(),
                rgb(0xee6b62).opacity(0.45).into(),
            )
        } else if !warning.is_empty() || percent.unwrap_or(0.0) >= 80.0 {
            (
                rgb(0xd98a2b).into(),
                rgb(0xf5b95c).into(),
                rgb(0xf0a94f).opacity(0.45).into(),
            )
        } else {
            (p.deep, p.light, p.accent.opacity(0.45))
        };
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(
                h_flex()
                    .w_full()
                    .items_baseline()
                    .justify_between()
                    .gap(px(10.0))
                    .mb(px(6.0))
                    .child(
                        label(title, label_size, rgb(0xe4e4e2).into())
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis(),
                    )
                    .child(meta),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(mix(p.accent, 0.07, rgb(0x2c2b2a).into()))
                    .shadow(vec![
                        shadow(0.0, 1.0, 1.0, rgb(0).opacity(0.35).into()).inset(),
                    ])
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(
                                (percent.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0) as f32,
                            ))
                            .rounded_full()
                            .bg(gradient(90.0, from, to))
                            .shadow(vec![shadow(0.0, 0.0, 8.0, glow)]),
                    ),
            )
            .when(!warning.is_empty(), |this| {
                this.child(
                    h_flex()
                        .mt(px(6.0))
                        .items_center()
                        .gap(px(5.0))
                        .text_size(px(10.5))
                        .line_height(px(15.225))
                        .text_color(rgb(0xf0a94f))
                        .child(
                            div()
                                .size(px(6.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(rgb(0xf0a94f)),
                        )
                        .child(warning),
                )
            })
            .into_any_element()
    }

    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User rejected the Redeem button color after reviewing the native panel; this supersedes preserving its previous light-blue fill.
    fn render_resets(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let p = self.palette;
        let credits = reset_credits(&self.account);
        let disabled =
            self.redeem_pending || credits.is_empty() || self.account["status"] != "ready";
        let notice = match text(&self.account, "resetCreditsError") {
            "" if !self.account["resetCreditDetails"].is_array() => {
                "Reset expiry details are unavailable. Refreshing usage may help."
            }
            "" if credits.is_empty() => "No resets available.",
            error => error,
        };
        v_flex()
            .id("account-usage-resets")
            .w_full()
            .mt(px(12.0))
            .pt(px(12.0))
            .border_t_1()
            .border_color(p.line)
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(10.0))
                    .line_height(px(14.5))
                    .text_color(p.dim)
                    .child(tracked("AVAILABLE RESETS", 0.6))
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb(px(8.0)),
            )
            .child(
                v_flex()
                    .gap(px(4.0))
                    .children(credits.into_iter().enumerate().map(|(index, credit)| {
                        let expires = timestamp(text(credit, "expiresAt"));
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap(px(9.0))
                            .pl(px(8.0))
                            .pr(px(9.0))
                            .py(px(7.0))
                            .border_1()
                            .border_color(p.line)
                            .rounded(px(8.0))
                            .bg(p.raised)
                            .child(
                                h_flex()
                                    .size(px(20.0))
                                    .flex_shrink_0()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(p.soft)
                                    .text_color(p.accent)
                                    .text_size(px(10.0))
                                    .font_weight(usage_font_weight(650.0))
                                    .child((index + 1).to_string()),
                            )
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .gap(px(1.0))
                                    .child(
                                        label(
                                            expires
                                                .map(|date| {
                                                    format!(
                                                        "Expires in {}",
                                                        duration(date - self.now)
                                                    )
                                                })
                                                .unwrap_or_else(|| "No expiry reported".into()),
                                            12.0,
                                            rgb(0xececea).into(),
                                        )
                                        .font_weight(usage_font_weight(550.0))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis(),
                                    )
                                    .child(label(
                                        if expires.is_some() {
                                            local_date(text(credit, "expiresAt"), true)
                                        } else {
                                            "Expiry unavailable".into()
                                        },
                                        10.5,
                                        p.muted,
                                    )),
                            )
                            .when(index == 0, |this| {
                                this.child(
                                    p.tag(true)
                                        .h(px(18.0))
                                        .px(px(7.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Used first"),
                                )
                            })
                    })),
            )
            .when(!notice.is_empty(), |this| {
                this.child(label(notice.to_string(), 10.5, p.muted).line_height(px(15.75)))
            })
            .child(
                h_flex()
                    .id("account-usage-redeem")
                    .w_full()
                    .h(px(32.0))
                    .mt(px(10.0))
                    .px(px(12.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.accent_line)
                    .bg(p.soft)
                    .text_color(p.accent)
                    .text_size(px(12.0))
                    .font_weight(usage_font_weight(650.0))
                    .when(disabled, |this| this.opacity(0.4).cursor_default())
                    .when(!disabled, |this| {
                        this.track_focus(&self.redeem_focus)
                            .focus_visible(move |this| this.shadow(focus_ring(p)))
                            .cursor_pointer()
                            .hover(move |this| {
                                this.bg(p.accent.opacity(0.20))
                                    .border_color(p.accent.opacity(0.48))
                            })
                            .active(move |this| {
                                this.bg(p.accent.opacity(0.10)).border_color(p.accent_line)
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.redeem(cx);
                            }))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.stop_propagation();
                                    this.redeem(cx);
                                }
                            }))
                    })
                    .child(tracked(
                        if self.redeem_pending {
                            "Opening reset chat…"
                        } else {
                            "Redeem a reset"
                        },
                        -0.05,
                    )),
            )
            .child(
                label(
                    "Opens a Codex chat and uses the reset expiring soonest.",
                    10.0,
                    p.dim,
                )
                .mt(px(7.0))
                .text_center(),
            )
            .into_any_element()
    }
}
