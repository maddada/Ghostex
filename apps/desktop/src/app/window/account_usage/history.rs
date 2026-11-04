use super::panel::{SECTION_GAP, section_label};
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
        let card = p
            .card()
            .child(
                h_flex()
                    .w_full()
                    .h(px(CHART_HEIGHT))
                    .items_end()
                    .gap(px(2.0))
                    .border_b_1()
                    .border_color(p.line)
                    .children(days.iter().enumerate().map(|(index, day)| {
                        let tokens = day["tokens"].as_f64().unwrap_or(0.0);
                        let tooltip =
                            format!("{}: {} tokens", text(day, "date"), exact_tokens(tokens));
                        let today = index + 1 == days.len();
                        div()
                            .id(("account-usage-day", index))
                            .flex_1()
                            .min_w_0()
                            .h(gpui::relative(if tokens > 0.0 {
                                (tokens / peak).max(0.03) as f32
                            } else {
                                0.0
                            }))
                            .rounded_t(px(1.5))
                            .bg(p.accent.opacity(if today { 1.0 } else { 0.42 }))
                            .hover(move |this| this.bg(p.accent))
                            .tooltip(move |window, cx| {
                                Tooltip::new(tooltip.clone()).build(window, cx)
                            })
                    })),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .mt(px(4.0))
                    .text_size(px(11.0))
                    .line_height(px(AXIS_LINE))
                    .text_color(p.dim)
                    .child("Last 30 days")
                    .child("Today"),
            )
            .child(
                h_flex().w_full().mt(px(STATS_GAP)).children(
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
                            .flex_basis(px(0.0))
                            .min_w_0()
                            .gap(px(1.0))
                            .when(index > 0, |this| {
                                this.border_l_1().border_color(p.line).pl(px(10.0))
                            })
                            .when(index < 2, |this| this.pr(px(10.0)))
                            .child(
                                label(title, 11.5, p.muted)
                                    .line_height(px(STAT_LABEL_LINE))
                                    .whitespace_nowrap(),
                            )
                            // CDXC:AgentProviders 2026-10-30 DECISION: User picked mockup A, where each stat is just its number and the fine print says they are tokens, so a long total never wraps.
                            .child(
                                div()
                                    .id(("account-usage-total", index))
                                    .whitespace_nowrap()
                                    .font_features(tabular_numbers())
                                    .text_size(px(17.0))
                                    .line_height(px(STAT_VALUE_LINE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.ink)
                                    .child(if known {
                                        compact_tokens(tokens)
                                    } else {
                                        "No data".into()
                                    })
                                    .when(known, |this| {
                                        this.tooltip(move |window, cx| {
                                            Tooltip::new(tooltip.clone()).build(window, cx)
                                        })
                                    }),
                            )
                    }),
                ),
            )
            .child(
                label(fine_print(zone), 11.0, p.dim)
                    .line_height(px(FINE_PRINT_LINE))
                    .mt(px(FINE_PRINT_GAP)),
            )
            .when(!notice.is_empty(), |this| {
                this.child(
                    label(notice, 11.5, amber())
                        .line_height(px(NOTICE_LINE))
                        .mt(px(FINE_PRINT_GAP)),
                )
            });
        v_flex()
            .w_full()
            .mt(px(SECTION_GAP))
            .child(
                section_label(format!("SHARED {} HISTORY", provider.to_uppercase())).child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.muted)
                        .child(scope),
                ),
            )
            .child(card)
            .into_any_element()
    }
}

/// "Tokens from conversation logs · Cached tokens included · Days in +04:00": the unit the three
/// numbers above it no longer repeat.
pub(super) fn fine_print(zone: &str) -> String {
    let mut text = "Tokens from conversation logs \u{b7} Cached tokens included".to_string();
    if !zone.is_empty() {
        text.push_str(&format!(" \u{b7} Days in {zone}"));
    }
    text
}

pub(super) const CHART_HEIGHT: f32 = 64.0;
pub(super) const AXIS_LINE: f32 = 15.0;
pub(super) const STATS_GAP: f32 = 10.0;
pub(super) const STAT_LABEL_LINE: f32 = 16.0;
pub(super) const STAT_VALUE_LINE: f32 = 22.0;
pub(super) const FINE_PRINT_LINE: f32 = 15.0;
pub(super) const FINE_PRINT_GAP: f32 = 9.0;
pub(super) const NOTICE_LINE: f32 = 16.0;
