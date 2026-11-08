//! Usage windows: where a percentage comes from, and how it reads.
//!
//! Port of the usage-window half of
//! `packages/core-ui/chat/session-chat-context-details-agents.ts` (`accountWindowKind`,
//! `sessionUsageWindow`, `usageWindows`, `accountUsageSamples`, `usagePercentText`,
//! `usageResetCountdown`, `usageResetText`, `formatWindowDuration`) plus the two shared account
//! rules it calls (`accountUsageLabel`, `isFiveHourWindow`, `isWeeklyWindow`).
//!
//! Those last three also exist in `packages/gx-core/src/sidebar_accounts/usage.rs`. This crate
//! does not depend on `gx-core`, so they are here too; when the two crates grow a shared base
//! they should come from one place.

use crate::menus::context::status::{
    AccountUsageWindow, AgentAccount, CodexRateLimitWindow, ContextDetailStatus,
};
use crate::menus::context::time::date_parse;
use crate::menus::context::usage::format_reset_countdown;
use crate::menus::picker::js::{js_number, js_round};

/// Which window a row draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageWindowKind {
    FiveHour,
    SevenDay,
    Model,
    /// The account-wide extra spending allowance, which only `accountSpending` reads.
    Spend,
}

/// One window, normalized.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageWindowSample {
    /// The app-wide compact window label (5h, 7d, Fable).
    pub label: String,
    pub used_percent: f64,
    /// Epoch seconds; `None` when the source reported no reset time.
    pub resets_at: Option<f64>,
}

/// `isWeeklyWindow`.
pub fn is_weekly_window(window: &AccountUsageWindow) -> bool {
    window.id == "sevenDay" || window.limit_window_seconds.unwrap_or(0.0) >= 604_800.0
}

/// `isFiveHourWindow`.
pub fn is_five_hour_window(window: &AccountUsageWindow) -> bool {
    window.id == "fiveHour" || window.limit_window_seconds == Some(18_000.0)
}

/// `accountUsageLabel`: `7d`, `5h`, `Fable`, `<model> 7d`, or the window's own label.
///
/// CDXC:AgentProviders 2026-09-12 DECISION:
/// User: the Fable limit percentage says "Fable:" throughout the app, without "7d".
pub fn account_usage_label(window: &AccountUsageWindow) -> String {
    if window
        .model
        .as_deref()
        .is_some_and(|model| model.to_lowercase() == "fable")
    {
        return "Fable".to_string();
    }
    // `window.model` is read as a truthy check, so an empty string is no model.
    let model = window.model.as_deref().filter(|model| !model.is_empty());
    let seconds = window.limit_window_seconds;
    let duration = match seconds {
        Some(seconds) if seconds > 0.0 => Some(if seconds % 86_400.0 == 0.0 {
            format!("{}d", js_number(seconds / 86_400.0))
        } else if seconds % 3_600.0 == 0.0 {
            format!("{}h", js_number(seconds / 3_600.0))
        } else {
            format!("{}m", js_number((seconds / 60.0).floor()))
        }),
        _ if window.id == "fiveHour" => Some("5h".to_string()),
        _ if window.id == "sevenDay" || model.is_some() => Some("7d".to_string()),
        _ => None,
    };
    match duration {
        Some(duration) => match model {
            Some(model) => format!("{model} {duration}"),
            None => duration,
        },
        None => window.label.clone(),
    }
}

/// CDXC:AgentProviders 2026-09-12 WHY:
/// Codex can report a weekly limit as its primary window, so primary/secondary ids do not
/// identify duration. Use the shared account window rules so chat agrees with the titlebar and
/// Accounts.
fn account_window_kind(window: &AccountUsageWindow) -> UsageWindowKind {
    if window
        .model
        .as_deref()
        .is_some_and(|model| !model.is_empty())
    {
        return UsageWindowKind::Model;
    }
    if is_five_hour_window(window) {
        return UsageWindowKind::FiveHour;
    }
    if is_weekly_window(window) {
        return UsageWindowKind::SevenDay;
    }
    if window.id == "spend" {
        UsageWindowKind::Spend
    } else {
        UsageWindowKind::Model
    }
}

/// `formatWindowDuration`.
fn format_window_duration(minutes: f64) -> String {
    if minutes % 1440.0 == 0.0 {
        return format!("{}d", js_number(minutes / 1440.0));
    }
    if minutes % 60.0 == 0.0 {
        return format!("{}h", js_number(minutes / 60.0));
    }
    format!("{}m", js_number(minutes))
}

/// Which row a window Codex reported in its rollout belongs to.
///
/// CDXC:AgentProviders 2026-10-09 WHY:
/// A Codex plan with only a weekly limit reports it as `primary` and sends no `secondary`, so reading the slot alone put the weekly window under the 5h rows and left the 7d rows empty; with no linked account to read instead, Codex's default status line (account email, 7d limit, 7d reset, account resets) had no value at all and stayed a skeleton. The window's own duration decides, as `account_window_kind` does for accounts; the slot only decides a window that reports no recognised duration.
fn codex_window_kind(window: &CodexRateLimitWindow, primary: bool) -> UsageWindowKind {
    match window.window_minutes {
        Some(minutes) if minutes == 300.0 => UsageWindowKind::FiveHour,
        Some(minutes) if minutes >= 10_080.0 => UsageWindowKind::SevenDay,
        _ if primary => UsageWindowKind::FiveHour,
        _ => UsageWindowKind::SevenDay,
    }
}

/// The window as it stands at `now`, for a reading the agent made some time ago.
///
/// CDXC:AgentProviders 2026-11-09 WHY:
/// GitHub #514: without a saved account a session's limits are the last numbers its agent reported, and an idle coordinator reports nothing new, so its old percentages (5h 78%, 7d 84%) stayed on screen long after the window's reset time. Once `resets_at` has passed, that reading describes a window that no longer exists: it reads as reset (0% used, no reset time to count down to) until the agent, or another session on the same login (gxserver hands the newest of those over), reports fresh numbers.
fn expire_reading(used_percent: f64, resets_at: Option<f64>, now: f64) -> (f64, Option<f64>) {
    match resets_at.filter(|value| value.is_finite()) {
        Some(seconds) if seconds * 1000.0 <= now => (0.0, None),
        _ => (used_percent, resets_at),
    }
}

/// `sessionUsageWindow`: the window this session's own agent last reported.
fn session_usage_window(
    status: &ContextDetailStatus,
    kind: UsageWindowKind,
    now: f64,
) -> Option<UsageWindowSample> {
    if kind == UsageWindowKind::Model || kind == UsageWindowKind::Spend {
        return None;
    }
    let fallback_label = if kind == UsageWindowKind::FiveHour {
        "5h"
    } else {
        "7d"
    };
    let claude = status.rate_limits.and_then(|limits| {
        if kind == UsageWindowKind::FiveHour {
            limits.five_hour
        } else {
            limits.seven_day
        }
    });
    if let Some(claude) = claude {
        if claude.used_percentage.is_some_and(f64::is_finite) {
            let (used_percent, resets_at) = expire_reading(
                claude.used_percentage.unwrap_or_default(),
                claude.resets_at,
                now,
            );
            return Some(UsageWindowSample {
                label: fallback_label.to_string(),
                used_percent,
                resets_at,
            });
        }
    }
    let codex = status.codex.as_ref().and_then(|codex| {
        [(codex.primary, true), (codex.secondary, false)]
            .into_iter()
            .find_map(|(window, primary)| {
                window.filter(|window| codex_window_kind(window, primary) == kind)
            })
    });
    if let Some(codex) = codex {
        if codex.used_percentage.is_some_and(f64::is_finite) {
            let (used_percent, resets_at) = expire_reading(
                codex.used_percentage.unwrap_or_default(),
                codex.resets_at,
                now,
            );
            return Some(UsageWindowSample {
                // `window.windowMinutes` is read as a truthy check, so zero is no duration.
                label: match codex.window_minutes {
                    Some(minutes) if minutes != 0.0 => format_window_duration(minutes),
                    _ => fallback_label.to_string(),
                },
                used_percent,
                resets_at,
            });
        }
    }
    None
}

/// `accountUsageSamples`.
pub fn account_usage_samples(
    account: &AgentAccount,
    kind: UsageWindowKind,
) -> Vec<UsageWindowSample> {
    account
        .usage
        .iter()
        .filter(|window| account_window_kind(window) == kind && window.used_percent.is_finite())
        .map(|window| {
            let resets_at = window
                .resets_at
                .as_deref()
                .filter(|stamp| !stamp.is_empty())
                .and_then(date_parse)
                .map(|millis| millis / 1000.0)
                .filter(|seconds| seconds.is_finite());
            UsageWindowSample {
                label: account_usage_label(window),
                used_percent: window.used_percent,
                resets_at,
            }
        })
        .collect()
}

/// `usageWindows`.
///
/// CDXC:AgentProviders 2026-09-09 WHY:
/// Rate limits belong to the account, including before a draft has made a request or written
/// transcript usage. A linked session reads the same usage windows as Accounts; only an unlinked
/// session reads the windows its agent last reported.
pub fn usage_windows(
    status: &ContextDetailStatus,
    kind: UsageWindowKind,
    now: f64,
) -> Vec<UsageWindowSample> {
    match status.account.as_ref() {
        Some(account) => account_usage_samples(account, kind),
        None => session_usage_window(status, kind, now)
            .into_iter()
            .collect(),
    }
}

/// `usagePercentText`.
///
/// CDXC:AgentProviders 2026-09-09 DECISION:
/// User: usage-window labels throughout the app use a colon (7d: 50%, 5h: 50%), without "used" in
/// the chat popover or status line.
pub fn usage_percent_text(window: &UsageWindowSample) -> Option<String> {
    Some(format!(
        "{}: {}%",
        window.label,
        js_number(js_round(window.used_percent))
    ))
}

/// `usageResetCountdown`.
pub fn usage_reset_countdown(window: &UsageWindowSample, now: f64) -> Option<String> {
    let resets_at = window.resets_at.filter(|value| value.is_finite())?;
    Some(if resets_at * 1000.0 > now {
        format!(
            "resets {}",
            format_reset_countdown(resets_at * 1000.0 - now)
        )
    } else {
        "reset due".to_string()
    })
}

/// `usageResetText`: the reset rows stand alone in the status line, so each carries its window
/// label (5h resets 2h 47m).
pub fn usage_reset_text(window: &UsageWindowSample, now: f64) -> Option<String> {
    let reset = usage_reset_countdown(window, now)?;
    Some(format!("{} {reset}", window.label))
}

/// `join`: the non-empty parts on one line, or `null`.
pub fn join(values: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let parts: Vec<String> = values
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}
