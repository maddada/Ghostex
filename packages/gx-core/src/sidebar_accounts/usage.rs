//! The account text rules the menus share with the rest of the app: the usage label, which windows
//! are a Claude or Codex account's headline, and the email mask.
//!
//! Each is a port of one shared TypeScript function, named on the function. They are the parity
//! risk of the account pages (rounding, the tightest-two choice, the mask's matching), so the
//! account gate enumerates them apart from the pages that use them.
//!
//! SEE-ALSO: packages/shared/account-usage-label.ts (deleted 2026-10-01), packages/shared/account-usage-windows.ts (deleted 2026-10-01),
//! packages/shared/account-display.ts (deleted 2026-10-01), apps/desktop/src/app/titlebar/account_usage.rs
//! (`claude_headline_windows`, the titlebar's own copy of the headline rule).

use crate::presentation_store::js_number;
use crate::sidebar_view::text::is_js_whitespace;

use super::data::{AccountUsageWindow, AgentAccount, ResetCredits};

/// `String(value)` for a number, including the two values `JSON.stringify` cannot write.
pub(crate) fn js_number_text(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        js_number(value)
    }
}

/// `Math.round`: halves go up, towards positive infinity. `f64::round` takes them away from zero,
/// and `floor(x + 0.5)` is wrong for the largest value below one half.
pub fn js_round(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
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
    let model = window.scoped_model();
    let duration = match window.limit_window_seconds {
        Some(seconds) if seconds > 0.0 => Some(if seconds % 86400.0 == 0.0 {
            format!("{}d", js_number_text(seconds / 86400.0))
        } else if seconds % 3600.0 == 0.0 {
            format!("{}h", js_number_text(seconds / 3600.0))
        } else {
            format!("{}m", js_number_text((seconds / 60.0).floor()))
        }),
        _ if window.id.as_deref() == Some("fiveHour") => Some("5h".to_string()),
        _ if window.id.as_deref() == Some("sevenDay") || model.is_some() => Some("7d".to_string()),
        _ => None,
    };
    match duration {
        Some(duration) => match model {
            Some(model) => format!("{model} {duration}"),
            None => duration,
        },
        // `window.label`, which a template string writes as `undefined` when it is missing.
        None => window
            .label
            .clone()
            .unwrap_or_else(|| "undefined".to_string()),
    }
}

/// `isWeeklyWindow`.
pub fn is_weekly_window(window: &AccountUsageWindow) -> bool {
    window.id.as_deref() == Some("sevenDay")
        || window.limit_window_seconds.unwrap_or(0.0) >= 604800.0
}

/// `isFiveHourWindow`.
pub fn is_five_hour_window(window: &AccountUsageWindow) -> bool {
    window.id.as_deref() == Some("fiveHour") || window.limit_window_seconds == Some(18000.0)
}

/// `fableWindow`, as an index into `usage`: the Fable model window, else the first model-scoped one.
fn fable_window(usage: &[AccountUsageWindow]) -> Option<usize> {
    let scoped = || {
        usage
            .iter()
            .enumerate()
            .filter(|(_, window)| window.scoped_model().is_some())
    };
    scoped()
        .find(|(_, window)| {
            window
                .model
                .as_deref()
                .is_some_and(|model| model.to_lowercase().contains("fable"))
        })
        .or_else(|| scoped().next())
        .map(|(index, _)| index)
}

/// `accountHeadlineWindows`, as indexes into `account.usage` so a window picked twice (one that is
/// both weekly and five-hour) keeps the identity the TypeScript's `Set` compares by.
///
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: for Claude accounts the Fable limit is the most important number and must never be
/// hidden. Wherever a Claude account shows two percentages, show the two tightest of the weekly,
/// five-hour, and Fable limits, in that fixed order. Codex keeps its weekly window and five-hour
/// window.
pub fn account_headline_windows(account: &AgentAccount) -> Vec<usize> {
    let main = || {
        account
            .usage
            .iter()
            .enumerate()
            .filter(|(_, window)| window.scoped_model().is_none())
    };
    let weekly = main()
        .find(|(_, window)| is_weekly_window(window))
        .map(|(index, _)| index);
    let five_hour = main()
        .find(|(_, window)| is_five_hour_window(window))
        .map(|(index, _)| index);
    if account.provider.as_deref() != Some("claude") {
        return [weekly, five_hour].into_iter().flatten().collect();
    }
    let candidates: Vec<usize> = [weekly, five_hour, fable_window(&account.usage)]
        .into_iter()
        .flatten()
        .collect();
    // `toSorted((left, right) => right.usedPercent - left.usedPercent)`, which is stable; a NaN
    // difference reads as equal, as the spec's comparator coercion makes it.
    let mut sorted = candidates.clone();
    sorted.sort_by(|left, right| {
        let difference = account.usage[*right].used_percent - account.usage[*left].used_percent;
        difference
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let tightest: Vec<usize> = sorted.into_iter().take(2).collect();
    candidates
        .into_iter()
        .filter(|index| tightest.contains(index))
        .collect()
}

/// `` `${accountUsageLabel(window)}: ${Math.round(window.usedPercent)}%` ``.
///
/// CDXC:AgentProviders 2026-09-09 DECISION:
/// User: usage labels use the compact 7d: 50% and 5h: 50% format throughout the app, without "used" after percentages.
pub(crate) fn usage_percent(window: &AccountUsageWindow) -> String {
    format!(
        "{}: {}%",
        account_usage_label(window),
        js_number_text(js_round(window.used_percent))
    )
}

/// The second line of a launcher account row: Claude's headline windows, or Codex's weekly window
/// and reset credits, joined with ` · `.
pub fn account_usage_detail(account: &AgentAccount, provider: &str) -> String {
    let parts: Vec<String> = if provider == "claude" {
        // A Claude account names its limit resets once it has any (CDXC:AgentProviders 2026-11-21 in
        // apps/desktop/src/app/titlebar/account_usage.rs); Codex always names its count.
        let resets = match &account.reset_credits {
            Some(ResetCredits::Number(number)) if *number > 0.0 => {
                Some(format!("{}rs", js_number_text(*number)))
            }
            _ => None,
        };
        account_headline_windows(account)
            .into_iter()
            .map(|index| usage_percent(&account.usage[index]))
            .chain(resets)
            .collect()
    } else {
        let weekly = account
            .usage
            .iter()
            .filter(|window| window.scoped_model().is_none())
            .find(|window| is_weekly_window(window));
        [
            weekly.map(usage_percent),
            account.reset_credits.as_ref().map(|credits| match credits {
                ResetCredits::Number(number) => format!("{}rs", js_number_text(*number)),
                ResetCredits::Text(text) => format!("{text}rs"),
            }),
        ]
        .into_iter()
        .flatten()
        .collect()
    };
    // `filter(Boolean)`: none of the parts can be empty, so this is every part.
    parts.join(" · ")
}

/// `maskAccountText`: every `local@domain` run keeps the first and last characters before the `@`
/// and replaces the rest with the fixed mask, including email-shaped account names.
///
/// CDXC:AgentProviders 2026-09-10 DECISION:
/// Hide emails keeps the first and last characters before @ and replaces every domain with the same
/// characters followed by .•••, including email-shaped account names.
pub fn mask_account_text(text: &str) -> String {
    // `/([^\s@]+)@[^\s@]+/gu`, scanned left to right. A run of token characters either ends at an
    // `@` followed by another token character (a match that starts at the run's first character,
    // since the quantifier is greedy) or cannot match from any of its positions.
    let token = |character: char| character != '@' && !is_js_whitespace(character);
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        if !token(characters[index]) {
            out.push(characters[index]);
            index += 1;
            continue;
        }
        let mut end = index;
        while end < characters.len() && token(characters[end]) {
            end += 1;
        }
        let matched = characters.get(end) == Some(&'@')
            && characters
                .get(end + 1)
                .is_some_and(|character| token(*character));
        if !matched {
            out.extend(&characters[index..end]);
            index = end;
            continue;
        }
        let mut domain_end = end + 1;
        while domain_end < characters.len() && token(characters[domain_end]) {
            domain_end += 1;
        }
        out.push(characters[index]);
        out.push_str("•••");
        if end - index > 1 {
            out.push(characters[end - 1]);
        }
        out.push_str("@•••••.•••");
        index = domain_end;
    }
    out
}
