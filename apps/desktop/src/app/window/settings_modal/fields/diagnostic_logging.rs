//! `DiagnosticLoggingSettingsField`: the "Turn logs off after" timer and one switch per
//! diagnostic area (`DIAGNOSTIC_LOGGING_SCENARIOS`), writing `diagnosticLogging` the way
//! `updateDiagnosticLoggingScenarios` and `setDiagnosticLoggingScenario` do.
use super::super::super::native_modal_kit::*;
use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::controls::switch_control;
use super::row::{PageAction, RowSpec, setting_row};
use super::select::settings_select;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// `DEFAULT_DIAGNOSTIC_LOGGING_ENABLE_DURATION`.
const DEFAULT_DURATION: &str = "1h";

#[derive(Default)]
pub(crate) struct DiagnosticLoggingState {
    /// The timer select's value; seeded from the enabled areas on first render.
    timer: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_index = (month + 9) % 12;
    let doy = (153 * month_index + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// `Date.prototype.toISOString` for a millisecond timestamp.
pub(crate) fn iso_string(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rest = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

/// `Date.parse` for the ISO timestamps the settings store.
pub(crate) fn parse_iso_ms(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, time) = text.split_once('T')?;
    let mut date_parts = date.split('-').map(|part| part.parse::<i64>());
    let (year, month, day) = (
        date_parts.next()?.ok()?,
        date_parts.next()?.ok()?,
        date_parts.next()?.ok()?,
    );
    let time = time.trim_end_matches('Z');
    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    let mut clock_parts = clock.split(':').map(|part| part.parse::<i64>());
    let (hour, minute, second) = (
        clock_parts.next()?.ok()?,
        clock_parts.next()?.ok()?,
        clock_parts.next().unwrap_or(Ok(0)).ok()?,
    );
    let millis: i64 = format!("{:0<3}", &fraction[..fraction.len().min(3)])
        .parse()
        .ok()?;
    Some(
        days_from_civil(year, month, day) * 86_400_000
            + ((hour * 60 + minute) * 60 + second) * 1000
            + millis,
    )
}

/// `getDiagnosticLoggingScenarioDuration`.
fn scenario_duration(value: &Value, scenario: &str) -> &'static str {
    let Some(state) = value
        .get("scenarios")
        .and_then(|scenarios| scenarios.get(scenario))
    else {
        return "off";
    };
    if state.get("enabled").and_then(Value::as_bool) != Some(true) {
        return "off";
    }
    let Some(expires_at) = state.get("expiresAt").and_then(Value::as_str) else {
        return "always";
    };
    let Some(expires_ms) = parse_iso_ms(expires_at) else {
        return "off";
    };
    let now = now_ms();
    if expires_ms <= now {
        return "off";
    }
    if expires_ms - now <= 30 * 60 * 1000 {
        "15m"
    } else {
        "1h"
    }
}

/// `getDiagnosticLoggingScenarioStateForDuration`.
fn scenario_state(duration: &str) -> Value {
    match duration {
        "15m" => json!({ "enabled": true, "expiresAt": iso_string(now_ms() + 15 * 60 * 1000) }),
        "1h" => json!({ "enabled": true, "expiresAt": iso_string(now_ms() + 60 * 60 * 1000) }),
        "always" => json!({ "enabled": true }),
        _ => json!({ "enabled": false }),
    }
}

/// `(id, label)` of every diagnostic area.
pub(crate) fn diagnostic_scenarios() -> Vec<(String, String)> {
    settings_catalog()
        .module_value(module::SETTINGS, "DIAGNOSTIC_LOGGING_SCENARIOS")
        .and_then(Value::as_array)
        .map(|scenarios| {
            scenarios
                .iter()
                .filter_map(|scenario| {
                    Some((
                        scenario.get("id")?.as_str()?.to_string(),
                        scenario.get("label")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `updateDiagnosticLoggingScenarios(scenarioIds, duration)` on top of `current`.
pub(crate) fn diagnostic_logging_with(
    current: &Value,
    scenario_ids: &[String],
    duration: &str,
) -> Value {
    let known: Vec<String> = diagnostic_scenarios()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let mut scenarios = Map::new();
    if let Some(existing) = current.get("scenarios").and_then(Value::as_object) {
        for (id, state) in existing {
            if known.contains(id) {
                scenarios.insert(id.clone(), state.clone());
            }
        }
    }
    let state = scenario_state(duration);
    for id in scenario_ids {
        scenarios.insert(id.clone(), state.clone());
    }
    json!({ "scenarios": Value::Object(scenarios), "version": 1 })
}

fn save<V: SettingsPage>(
    page: &mut V,
    scenario_ids: Vec<String>,
    duration: &str,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    let duration = duration.to_string();
    store.update(cx, |store, cx| {
        let next =
            diagnostic_logging_with(&store.value("diagnosticLogging"), &scenario_ids, &duration);
        store.update_setting("diagnosticLogging", next, cx);
    });
}

fn duration_options() -> Vec<SettingOption> {
    settings_catalog().options(
        module::SETTINGS_TYPES,
        "DIAGNOSTIC_LOGGING_DURATION_OPTIONS",
    )
}

/// `DiagnosticLoggingSettingsField` (a wide row).
#[allow(clippy::too_many_arguments)]
pub(crate) fn diagnostic_logging_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &Value,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let scenarios = diagnostic_scenarios();
    let enabled: Vec<String> = scenarios
        .iter()
        .filter(|(id, _)| scenario_duration(value, id) != "off")
        .map(|(id, _)| id.clone())
        .collect();
    let timer = page
        .field_states()
        .diagnostic_logging
        .timer
        .get_or_insert_with(|| {
            let durations: Vec<&str> = enabled
                .iter()
                .map(|id| scenario_duration(value, id))
                .collect();
            if durations.contains(&"always") {
                "always".to_string()
            } else {
                durations
                    .first()
                    .copied()
                    .unwrap_or(DEFAULT_DURATION)
                    .to_string()
            }
        })
        .clone();
    let enabled_for_timer = enabled.clone();
    let timer_select = settings_select(
        page,
        p,
        "diagnostic-logging-timer",
        &duration_options(),
        &timer,
        Some(144.0),
        false,
        None,
        move |page: &mut V, next, _window, cx| {
            page.field_states().diagnostic_logging.timer = Some(next.clone());
            if !enabled_for_timer.is_empty() {
                save(page, enabled_for_timer.clone(), &next, cx);
            }
            cx.notify();
        },
        window,
        cx,
    );
    let label = |text: String| {
        div()
            .text_size(px(14.0))
            .text_color(hsla(p.foreground))
            .child(text)
    };
    let mut rows: Vec<AnyElement> = vec![
        h_flex()
            .min_w_0()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(label("Turn logs off after".to_string()))
            .child(timer_select)
            .into_any_element(),
    ];
    for (id, name) in scenarios {
        let checked = enabled.contains(&id);
        let timer = timer.clone();
        let scenario = id.clone();
        rows.push(
            h_flex()
                .min_w_0()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(label(name.clone()))
                .child(switch_control(
                    p,
                    SharedString::from(format!("diagnostic-{id}")),
                    name.clone(),
                    checked,
                    false,
                    None,
                    move |page: &mut V, next, _window, cx| {
                        let duration = if next {
                            timer.clone()
                        } else {
                            "off".to_string()
                        };
                        save(page, vec![scenario.clone()], &duration, cx);
                    },
                    cx,
                ))
                .into_any_element(),
        );
    }
    let control = v_flex()
        .w_full()
        .gap(px(8.0))
        .children(rows)
        .into_any_element();
    setting_row(p, "diagnosticLogging", spec.wide(), on_reset, control, cx)
}
