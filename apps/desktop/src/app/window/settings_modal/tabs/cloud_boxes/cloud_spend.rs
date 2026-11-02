//! "Spending" (this month per provider and the monthly cap, Q1000, Q1001, D8) and "Stopping and
//! deleting" (the idle stop, the archived-box delete and phone starts, Q1011, Q1012, D7).
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, RowSpec, reset_key, select_field, setting_row,
    settings_button, settings_number_input, settings_section, toggle_field,
};
use super::super::super::palette::SettingsPalette;
use super::{CloudBoxesTab, info_row};
use ghostex_gx_protocol::cloud_box::{CloudProviderId, CloudSpendState, setting_keys};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::h_flex;
use serde_json::json;

const ICON_CHART: &str = "modals/settings/chart-bar.svg";
const ICON_REFRESH: &str = "modals/settings/refresh.svg";

const CAP_KEY: &str = setting_keys::MONTHLY_CAP_USD;
const IDLE_KEY: &str = setting_keys::IDLE_STOP_MINUTES;
const DELETE_KEY: &str = setting_keys::DELETE_STOPPED_AFTER_DAYS;
const PHONE_KEY: &str = setting_keys::PHONE_START_ENABLED;

pub(super) fn usd(amount: f64) -> String {
    format!("${amount:.2}")
}

/// The options of a catalog row (the Stop and Delete menus), plus the saved value when it is not
/// one of them (set with `ghostex settings set`).
fn row_options(key: &str, saved: &str, unit: &str) -> Vec<SettingOption> {
    let mut options: Vec<SettingOption> =
        ghostex_settings_catalog::extra_pages(ghostex_settings_catalog::Platform::current())
            .into_iter()
            .flat_map(|page| page.sections)
            .flat_map(|section| section.settings)
            .find(|row| row.key == key)
            .map(|row| {
                row.options
                    .into_iter()
                    .map(|option| SettingOption {
                        label: option.label,
                        value: option.value,
                    })
                    .collect()
            })
            .unwrap_or_default();
    if !options.iter().any(|option| option.value == saved) {
        options.push(SettingOption {
            label: format!("{saved} {unit}"),
            value: saved.to_string(),
        });
    }
    options
}

fn whole(value: f64) -> String {
    format!("{}", value.round() as i64)
}

impl CloudBoxesTab {
    pub(super) fn cloud_spending_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows = Vec::new();
        if show("cloudBoxesSpend") {
            rows.push(self.spend_row(p, cx));
        }
        if show(CAP_KEY) {
            rows.push(self.cap_row(p, window, cx));
        }
        settings_section(p, "Spending", None, None, rows).map(IntoElement::into_any_element)
    }

    /// "This month: Instabox $1.42 · Boat $0.30 · total $1.72 of $20".
    fn spend_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let loading = self.cloud.loading.contains("spend");
        let refresh = settings_button(
            p,
            "cloud-boxes-spend-refresh",
            "Refresh",
            Some(ICON_REFRESH),
            ButtonVariant::Ghost,
            loading || !self.rpc_available(cx),
            Some("Reading this month's spend…".into()),
            |page: &mut Self, _window, cx| page.load_cloud_spend(true, cx),
            cx,
        );
        let (dot, state) = match (&self.cloud.spend, &self.cloud.spend_error) {
            (_, Some(error)) => (
                ListItemStatus::Warning,
                format!("Ghostex could not read this month's spend: {error}"),
            ),
            (None, None) => (ListItemStatus::Neutral, "Reading…".to_string()),
            (Some(spend), None) => {
                let mut parts: Vec<String> = spend
                    .providers
                    .iter()
                    // The Local provider never bills; listing "$0.00" for it says nothing.
                    .filter(|line| line.provider != CloudProviderId::Local)
                    .map(|line| {
                        let mut text = format!(
                            "{} {}{}",
                            line.provider.label(),
                            if line.estimated { "≈" } else { "" },
                            usd(line.usd)
                        );
                        if line.error.is_some() {
                            text.push_str(" (last read)");
                        }
                        text
                    })
                    .collect();
                parts.push(match spend.cap_usd {
                    Some(cap) => format!("total {} of {}", usd(spend.total_usd), usd(cap)),
                    None => format!("total {}", usd(spend.total_usd)),
                });
                let dot = match spend.state {
                    CloudSpendState::Reached => ListItemStatus::Warning,
                    CloudSpendState::Warn => ListItemStatus::Warning,
                    _ => ListItemStatus::Success,
                };
                let mut state = parts.join(" · ");
                match spend.state {
                    CloudSpendState::Reached => {
                        state.push_str(" · Cap reached: new boxes won't start")
                    }
                    CloudSpendState::Warn => state.push_str(" · Over 80% of the cap"),
                    _ => {}
                }
                (dot, state)
            }
        };
        info_row(
            p,
            "cloud-spend",
            Some(dot),
            ICON_CHART,
            "This month",
            "Instabox counts everything on that account this month; Boat counts the boxes Ghostex made. A ≈ marks Ghostex's estimate from running time.",
            Some(state),
            vec![refresh],
        )
    }

    /// The cap in dollars; an empty box or 0 is no cap.
    fn cap_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let values = self.store.read(cx).values();
        let cap = values.f64(CAP_KEY);
        let shown = if cap > 0.0 { whole(cap) } else { String::new() };
        let id = SharedString::from(CAP_KEY);
        let input = FieldStates::text_state(
            self,
            &id,
            &shown,
            Some("No cap"),
            |page: &mut Self, text, _window, cx| {
                let text = text.trim().trim_start_matches('$').trim();
                let next = if text.is_empty() {
                    Some(0.0)
                } else {
                    text.parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .map(|value| value.min(10_000.0).round())
                };
                let Some(next) = next else {
                    return;
                };
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    if (store.f64(CAP_KEY) - next).abs() > f64::EPSILON {
                        store.update_setting(CAP_KEY, json!(next), cx);
                    }
                });
                page.load_cloud_spend(false, cx);
            },
            window,
            cx,
        );
        let control = h_flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(gpui::Hsla::from(p.muted))
                    .child("$"),
            )
            .child(settings_number_input(p, &input, 96.0, window, cx))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(gpui::Hsla::from(p.muted))
                    .child("a month"),
            )
            .into_any_element();
        setting_row(
            p,
            CAP_KEY,
            RowSpec::new("Monthly cap")
                .description("Ghostex warns at 80% and starts no new box at 100%. Running boxes keep running. Leave it empty for no cap.")
                .keyed(&values, CAP_KEY),
            Some(reset_key(CAP_KEY)),
            control,
            cx,
        )
    }

    pub(super) fn cloud_lifetime_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let values = self.store.read(cx).values();
        let mut rows = Vec::new();
        for (key, label, description, unit) in [
            (
                IDLE_KEY,
                "Stop idle boxes after",
                "With no agent working and no terminal open, a box pauses (Instabox) or stops (Boat). Sending a message wakes it.",
                "minutes",
            ),
            (
                DELETE_KEY,
                "Delete archived boxes after",
                "When every session on a box is archived, the box stops, and is deleted after this long. A box with unpushed work is kept.",
                "days",
            ),
        ] {
            if !show(key) {
                continue;
            }
            let saved = whole(values.f64(key));
            let options = row_options(key, &saved, unit);
            rows.push(select_field(
                self,
                p,
                key,
                RowSpec::new(label)
                    .description(description)
                    .keyed(&values, key),
                Some(reset_key(key)),
                &options,
                &saved,
                Some(180.0),
                move |page: &mut Self, value, _window, cx| {
                    if let Ok(number) = value.parse::<f64>() {
                        let store = page.store.clone();
                        store.update(cx, |store, cx| store.update_setting(key, json!(number), cx));
                    }
                },
                window,
                cx,
            ));
        }
        if show(PHONE_KEY) {
            rows.push(toggle_field(
                self,
                p,
                PHONE_KEY,
                RowSpec::new("Start boxes from your phone")
                    .description("Your paired phones keep your provider key, agent logins for boxes, Default snapshots and picked secrets in their secure storage, so they can start a box while this computer is off.")
                    .keyed(&values, PHONE_KEY),
                values.bool(PHONE_KEY),
                cx,
            ));
        }
        settings_section(p, "Stopping and deleting", None, None, rows)
            .map(IntoElement::into_any_element)
    }
}
