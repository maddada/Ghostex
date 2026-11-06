//! Settings > Autopilot's summary (what it did since it went on) and its log (every decision,
//! newest first), from the same `/api/autopilot` answer as the rest of the page (autopilot.rs).
use super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_list_item, settings_section,
    static_note,
};
use super::super::palette::SettingsPalette;
use super::autopilot::{AutopilotTab, LOG_PAGE, list, text, when};
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

impl AutopilotTab {
    /// Four numbers since Autopilot went on: what it did, how many sessions it kept moving, what
    /// it left for the user, and what failed. Shown once it has done something.
    pub(super) fn summary_section(
        &mut self,
        p: &SettingsPalette,
        _cx: &mut Context<Self>,
    ) -> Option<Div> {
        let view = self.view.as_ref()?;
        let summary = &view["summary"];
        let count = |key: &str| summary[key].as_u64().unwrap_or(0);
        let on = view["config"]["enabled"].as_bool() == Some(true);
        if !on && list(view, "log").is_empty() {
            return None;
        }
        let tile = |value: u64, label: &str| {
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(p.hairline)
                .bg(p.raised)
                .child(
                    div()
                        .text_size(px(20.0))
                        .line_height(px(26.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.foreground)
                        .child(value.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .child(label.to_string()),
                )
        };
        let tiles = h_flex()
            .w_full()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(12.0))
            .child(tile(count("actions"), "Actions"))
            .child(tile(count("keptMoving"), "Sessions kept moving"))
            .child(tile(count("parkedCount"), "Left for you"))
            .child(tile(count("failed"), "Failed"));
        let mut rows: Vec<AnyElement> = vec![tiles.into_any_element()];
        for parked in list(&summary.clone(), "parked").into_iter().take(5) {
            rows.push(settings_list_item(
                p,
                Some(ListItemStatus::Warning),
                None,
                format!("{} ({})", text(&parked, "title"), text(&parked, "project")),
                Some(
                    div()
                        .child(format!(
                            "{} · {}",
                            text(&parked, "note"),
                            when(parked["at"].as_str())
                        ))
                        .into_any_element(),
                ),
                None,
            ));
        }
        let since = view["config"]["enabledAt"].as_str();
        settings_section(
            p,
            "Summary",
            Some(if on {
                format!("Since Autopilot went on, {}.", when(since)).into()
            } else {
                "The last 24 hours.".into()
            }),
            None,
            rows,
        )
    }

    /// Every decision, newest first: the session, what Autopilot saw, what it did, and when.
    pub(super) fn log_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let view = self.view.clone()?;
        let log = list(&view, "log");
        let mut rows: Vec<AnyElement> = Vec::new();
        if log.is_empty() {
            rows.push(static_note(
                p,
                "Nothing yet. Each time Autopilot acts on a session, or leaves one for you, it shows here.",
                false,
            ));
        }
        for entry in &log {
            let status = match entry["outcome"].as_str() {
                Some("done") => ListItemStatus::Success,
                _ => ListItemStatus::Warning,
            };
            let detail = v_flex()
                .gap(px(2.0))
                .child(div().child(text(entry, "saw")))
                .child(div().text_color(p.foreground).child(text(entry, "did")))
                .child(div().child(format!(
                    "{} · {}",
                    text(entry, "project"),
                    when(entry["at"].as_str())
                )));
            rows.push(settings_list_item(
                p,
                Some(status),
                None,
                text(entry, "title"),
                Some(detail.into_any_element()),
                None,
            ));
        }
        if log.len() as u64 >= self.log_limit {
            rows.push(
                h_flex()
                    .w_full()
                    .justify_center()
                    .py(px(8.0))
                    .child(settings_button(
                        p,
                        "autopilot-log-more",
                        "Show more",
                        None,
                        ButtonVariant::Outline,
                        false,
                        None,
                        |page: &mut Self, _window, cx| {
                            page.log_limit += LOG_PAGE;
                            page.call(json!({ "op": "read" }), cx);
                        },
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        settings_section(
            p,
            "Log",
            Some("Kept 30 days. Also ghostex autopilot log.".into()),
            None,
            rows,
        )
    }
}
