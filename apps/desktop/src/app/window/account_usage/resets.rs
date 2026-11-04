use super::super::resources_style::{RESOURCE_CARD_RADIUS, RESOURCE_CONTROL_RADIUS};
use super::{data::*, panel::AccountUsagePanel, style::*};
use crate::*;
use serde_json::Value;

impl AccountUsagePanel {
    /// The reset list OpenUsage shows: each reset has its own Use button, which unfolds into an inline confirmation, runs the claim in place, and reports the result above the list.
    pub(super) fn render_resets(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let p = self.palette;
        let credits: Vec<_> = credit_keys(&self.account)
            .into_iter()
            .filter(|(key, _)| !self.claimed.contains(key))
            .collect();
        let busy = self.confirming.is_some() || self.claiming.is_some();
        let ready = self.account["status"] == "ready";
        let notice = match text(&self.account, "resetCreditsError") {
            "" if !self.account["resetCreditDetails"].is_array() => {
                "Reset details are unavailable. Refreshing usage may help."
            }
            "" if credits.is_empty() => "No resets available.",
            "" => "",
            error => error,
        };
        let mut list = v_flex().gap(px(4.0));
        for (index, (key, credit)) in credits.into_iter().enumerate() {
            let row = if self.confirming.as_deref() == Some(key.as_str()) {
                self.render_confirm(credit, cx)
            } else if self.claiming.as_deref() == Some(key.as_str()) {
                reset_row(p, index, credit)
                    .child(label("Resetting your usage…", 11.5, p.muted).flex_shrink_0())
                    .into_any_element()
            } else {
                let key = key.clone();
                reset_row(p, index, credit)
                    .when(busy, |this| this.opacity(0.45))
                    .when(!busy && ready, |this| {
                        this.child(
                            p.control(("account-usage-reset-use", index))
                                .tab_index(0)
                                .px(px(10.0))
                                .font_weight(FontWeight::MEDIUM)
                                .focus_visible(|this| this.shadow(focus_ring()))
                                .on_click(cx.listener({
                                    let key = key.clone();
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.confirm_reset(key.clone(), window, cx);
                                    }
                                }))
                                .on_key_down(cx.listener(
                                    move |this, event: &gpui::KeyDownEvent, window, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        {
                                            cx.stop_propagation();
                                            this.confirm_reset(key.clone(), window, cx);
                                        }
                                    },
                                ))
                                .child("Use"),
                        )
                    })
                    .into_any_element()
            };
            list = list.child(row);
        }
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
            .when_some(self.banner.as_ref(), |this, (outcome, message)| {
                let color: Hsla = match outcome.as_str() {
                    "success" => chrome_color(0x6fcf8f, 0x15803d).into(),
                    "nothingToReset" => p.ink,
                    "noCredit" => chrome_color(0xf0a94f, 0x986009).into(),
                    _ => chrome_color(0xf5817a, 0xb42318).into(),
                };
                this.child(
                    label(message.clone(), 10.5, color)
                        .mb(px(8.0))
                        .py(px(7.0))
                        .px(px(10.0))
                        .rounded(px(8.0))
                        .bg(color.opacity(0.09))
                        .border_1()
                        .border_color(color.opacity(0.22)),
                )
            })
            .child(list)
            .when(!notice.is_empty(), |this| {
                this.child(
                    label(notice.to_string(), 10.5, p.muted)
                        .mt(px(4.0))
                        .line_height(px(15.75)),
                )
            })
            .into_any_element()
    }

    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User rejected the Redeem button color after reviewing the native panel; this supersedes preserving its previous light-blue fill.
    fn render_confirm(&self, credit: &Value, cx: &mut gpui::Context<Self>) -> AnyElement {
        let p = self.palette;
        let note = text(credit, "note");
        v_flex()
            .w_full()
            .gap(px(3.0))
            .p(px(10.0))
            .border_1()
            .border_color(p.line)
            .rounded(px(RESOURCE_CARD_RADIUS))
            .bg(p.raised)
            .child(label("Use this reset?", 12.0, p.ink).font_weight(usage_font_weight(600.0)))
            .child(
                label(
                    "Immediately resets your usage limits. This can't be undone.",
                    10.5,
                    p.muted,
                )
                .line_height(px(15.0)),
            )
            .when(!note.is_empty(), |this| {
                this.child(label(note.to_string(), 10.5, p.muted))
            })
            .child(
                h_flex()
                    .w_full()
                    .mt(px(7.0))
                    .gap(px(8.0))
                    .child(
                        h_flex()
                            .id("account-usage-reset-confirm")
                            .track_focus(&self.confirm_focus)
                            .flex_1()
                            .h(px(30.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(RESOURCE_CONTROL_RADIUS))
                            .border_1()
                            .border_color(chrome_ink().opacity(0.16))
                            .bg(chrome_ink().opacity(0.12))
                            .text_color(p.ink)
                            .text_size(px(12.0))
                            .font_weight(usage_font_weight(650.0))
                            .cursor_pointer()
                            .focus_visible(|this| this.shadow(focus_ring()))
                            .hover(|this| this.bg(chrome_ink().opacity(0.18)))
                            .active(|this| this.bg(chrome_ink().opacity(0.10)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.claim_reset(cx);
                            }))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.stop_propagation();
                                    this.claim_reset(cx);
                                }
                            }))
                            .child(tracked("Reset", -0.05)),
                    )
                    .child(
                        p.outline_button("account-usage-reset-cancel")
                            .tab_index(0)
                            .focus_visible(|this| this.shadow(focus_ring()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.cancel_reset(cx);
                            }))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.stop_propagation();
                                    this.cancel_reset(cx);
                                }
                            }))
                            .child("Cancel"),
                    ),
            )
            .into_any_element()
    }
}

/// One reset in the list: its number, how long until it expires, and its exact deadline.
fn reset_row(p: Palette, index: usize, credit: &Value) -> gpui::Div {
    let expires = timestamp(text(credit, "expiresAt"));
    let now = now();
    let note = text(credit, "note");
    h_flex()
        .w_full()
        .min_h(px(44.0))
        .items_center()
        .gap(px(9.0))
        .pl(px(8.0))
        .pr(px(9.0))
        .py(px(7.0))
        .border_1()
        .border_color(p.line)
        .rounded(px(RESOURCE_CARD_RADIUS))
        .bg(p.raised)
        .child(
            h_flex()
                .size(px(20.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(chrome_ink().opacity(0.10))
                .text_color(p.ink)
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
                            .map(|date| format!("Expires in {}", duration(date - now)))
                            .unwrap_or_else(|| "No expiry reported".into()),
                        12.0,
                        p.ink,
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
                ))
                .when(!note.is_empty(), |this| {
                    this.child(label(note.to_string(), 10.0, p.dim))
                }),
        )
}
