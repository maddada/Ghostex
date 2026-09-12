use super::{data::*, panel::AccountUsagePanel, style::*};
use crate::*;

impl AccountUsagePanel {
    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User: shared provider history stays always expanded below per-account live limits; both sections use gxserver snapshots.
    /// SEE-ALSO: server/src/accounts/history.rs owns the provider-wide history snapshot.
    pub(super) fn render_history(&self) -> AnyElement {
        let p = self.palette;
        let provider = if p.codex { "Codex" } else { "Claude" };
        let history = &self.account["usageHistory"];
        let days = array(history, "days");
        let known = history["hasData"] == true;
        let remote = !matches!(text(&self.account, "titlebarMachine"), "" | "local");
        let count = self.account["providerAccountCount"].as_u64().unwrap_or(0);
        let scope = if count > 0 {
            format!("{count} saved account{}", if count == 1 { "" } else { "s" })
        } else {
            "All accounts".into()
        };
        let peak = days
            .iter()
            .map(|day| day["tokens"].as_f64().unwrap_or(0.0))
            .fold(1.0, f64::max);
        let total: f64 = days
            .iter()
            .map(|day| day["tokens"].as_f64().unwrap_or(0.0))
            .sum();
        let today = days
            .last()
            .and_then(|day| day["tokens"].as_f64())
            .unwrap_or(0.0);
        let yesterday = days
            .iter()
            .rev()
            .nth(1)
            .and_then(|day| day["tokens"].as_f64())
            .unwrap_or(0.0);
        let notice = if history.is_null() || history["status"] == "loading" {
            "Reading shared history…"
        } else if history["status"] == "unavailable" {
            "Update Ghostex on this computer to read shared history."
        } else if history["status"] == "partial" {
            "Some history could not be read. Totals may be incomplete."
        } else if !known {
            "No recorded token usage in the last 30 days."
        } else {
            ""
        };
        let zone = text(history, "timeZone");
        p.card()
            .child(
                section_heading()
                    .child(heading(format!("Shared {provider} history")))
                    .child(p.tag(false).child(scope)),
            )
            .child(
                label(
                    format!(
                        "Combined stats from all {provider} conversations on {}.",
                        if remote {
                            "the remote computer"
                        } else {
                            "this computer"
                        }
                    ),
                    10.5,
                    p.muted,
                )
                .line_height(px(15.75))
                .mb(px(12.0)),
            )
            .child(
                h_flex()
                    .w_full()
                    .h(px(60.0))
                    .items_end()
                    .gap(px(2.0))
                    .pb(px(1.0))
                    .border_b_1()
                    .border_color(p.strong)
                    .children(days.iter().enumerate().map(|(index, day)| {
                        let tokens = day["tokens"].as_f64().unwrap_or(0.0);
                        let tooltip =
                            format!("{}: {} tokens", text(day, "date"), exact_tokens(tokens));
                        div()
                            .id(("account-usage-day", index))
                            .flex_1()
                            .min_w_0()
                            .h(gpui::relative(if tokens > 0.0 {
                                (tokens / peak).max(0.03) as f32
                            } else {
                                0.0
                            }))
                            .rounded_t(px(2.0))
                            .bg(gradient(180.0, p.light, p.deep))
                            .opacity(if index + 1 == days.len() { 1.0 } else { 0.5 })
                            .when(index + 1 == days.len(), |this| {
                                this.shadow(vec![shadow(0.0, 0.0, 8.0, p.accent.opacity(0.55))])
                            })
                            .hover(|this| this.opacity(1.0))
                            .tooltip(move |window, cx| {
                                Tooltip::new(tooltip.clone()).build(window, cx)
                            })
                    })),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .mt(px(5.0))
                    .text_size(px(9.5))
                    .line_height(px(13.775))
                    .text_color(p.dim)
                    .child("Last 30 days")
                    .child("Today"),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .w_full()
                    .mt(px(13.0))
                    .gap(px(10.0))
                    .children(
                        [
                            ("Today", today),
                            ("Yesterday", yesterday),
                            ("Last 30 days", total),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (title, tokens))| {
                            let tooltip = format!("{} tokens", exact_tokens(tokens));
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .when(index > 0, |this| {
                                    this.border_l_1().border_color(p.line).pl(px(10.0))
                                })
                                .child(label(title, 10.0, p.muted).mb(px(3.0)).whitespace_nowrap())
                                .child(
                                    h_flex()
                                        .flex_wrap()
                                        .gap_x(px(3.5))
                                        .items_baseline()
                                        .child(
                                            div()
                                                .id(("account-usage-total", index))
                                                .font_features(tabular_numbers())
                                                .text_size(px(18.0))
                                                .line_height(px(19.8))
                                                .font_weight(usage_font_weight(650.0))
                                                .child(tracked(
                                                    if known {
                                                        compact_tokens(tokens)
                                                    } else {
                                                        "No data".into()
                                                    },
                                                    -0.4,
                                                ))
                                                .when(known, |this| {
                                                    this.tooltip(move |window, cx| {
                                                        Tooltip::new(tooltip.clone())
                                                            .build(window, cx)
                                                    })
                                                }),
                                        )
                                        .child(label("tokens", 9.5, p.dim).ml(px(3.0))),
                                )
                        }),
                    ),
            )
            .child(
                label(
                    format!(
                        "Conversation logs · Cached tokens included{}",
                        if zone.is_empty() {
                            String::new()
                        } else {
                            format!(" · Days in {zone}")
                        }
                    ),
                    9.5,
                    p.dim,
                )
                .line_height(px(14.25))
                .mt(px(12.0)),
            )
            .when(!notice.is_empty(), |this| {
                this.child(label(notice, 10.5, rgb(0xf0a94f).into()).mt(px(8.0)))
            })
            .into_any_element()
    }
}
