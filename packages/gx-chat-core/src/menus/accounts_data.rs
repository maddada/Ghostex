//! The `/api/agentAccounts` answer, as the chat's account panel and switch card read it.
//!
//! The document publishes the answer verbatim (`accounts`), so this is a reader, not a schema: a
//! field the daemon leaves out is absent here exactly as it is `undefined` in the TypeScript, and
//! the policy objects are kept as the raw value they arrived as so re-publishing them cannot
//! reorder a key.
//!
//! SEE-ALSO: packages/shared/agent-accounts.ts, packages/gx-core/src/sidebar_accounts/data.rs
//! (the sidebar's own reader of the same answer).

use serde_json::Value;

use crate::jsnum::js_safe_integer;

/// One usage window of an account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageWindow {
    pub id: Option<String>,
    pub label: Option<String>,
    pub used_percent: Option<f64>,
    pub limit_window_seconds: Option<f64>,
    pub resets_at: Option<String>,
    pub model: Option<String>,
}

impl UsageWindow {
    /// `window.model` as a truthy value: an empty model is no model.
    pub fn scoped_model(&self) -> Option<&str> {
        self.model.as_deref().filter(|model| !model.is_empty())
    }

    fn from_value(value: &Value) -> Self {
        let object = value.as_object();
        let text = |key: &str| {
            object
                .and_then(|object| object.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let number = |key: &str| {
            object
                .and_then(|object| object.get(key))
                .and_then(Value::as_f64)
        };
        Self {
            id: text("id"),
            label: text("label"),
            used_percent: number("usedPercent"),
            limit_window_seconds: number("limitWindowSeconds"),
            resets_at: text("resetsAt"),
            model: text("model"),
        }
    }
}

/// One account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub id: Option<String>,
    pub provider: Option<String>,
    pub selector: Option<String>,
    pub indicator: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub registered: bool,
    /// The user turned the account off in Settings > Accounts: it is never offered as a switch
    /// target (gxserver's `CDXC:AgentProviders 2026-10-14` on `SavedAccount::user_disabled`).
    pub user_disabled: bool,
    pub status: Option<String>,
    pub usage: Vec<UsageWindow>,
    /// `resetCredits` as the template string writes it, or absent.
    pub reset_credits: Option<String>,
    pub usage_error: Option<String>,
    /// Why gxserver stopped offering this login (its organization disabled subscription access).
    pub disabled_reason: Option<String>,
}

impl Account {
    pub(crate) fn from_value(value: &Value) -> Self {
        let object = value.as_object();
        let text = |key: &str| {
            object
                .and_then(|object| object.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Self {
            id: text("id"),
            provider: text("provider"),
            selector: text("selector"),
            indicator: text("indicator"),
            name: text("name").unwrap_or_default(),
            email: text("email"),
            registered: truthy(object.and_then(|object| object.get("registered"))),
            user_disabled: truthy(object.and_then(|object| object.get("userDisabled"))),
            status: text("status"),
            usage: object
                .and_then(|object| object.get("usage"))
                .and_then(Value::as_array)
                .map(|entries| entries.iter().map(UsageWindow::from_value).collect())
                .unwrap_or_default(),
            reset_credits: object
                .and_then(|object| object.get("resetCredits"))
                .and_then(js_number_or_text),
            usage_error: text("usageError").filter(|error| !error.is_empty()),
            disabled_reason: text("disabledReason").filter(|reason| !reason.is_empty()),
        }
    }
}

/// The session's own half of the answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountSession {
    pub provider: Option<String>,
    pub account_id: Option<String>,
    /// The session defaults, kept raw so republishing cannot reorder its keys.
    pub policy: Value,
    /// The per-session override, or `null`.
    pub policy_override: Value,
    pub recovery: Option<AccountRecovery>,
}

impl AccountSession {
    /// `session.override ?? session.policy`.
    pub fn effective_policy(&self) -> &Value {
        if self.policy_override.is_null() {
            &self.policy
        } else {
            &self.policy_override
        }
    }

    /// Whether the session carries an override, which the panel shows as "Custom settings".
    pub fn has_override(&self) -> bool {
        !self.policy_override.is_null()
    }
}

/// The automatic-continuation recovery the session is in, when it is in one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountRecovery {
    pub reason: String,
    pub attempt: i64,
    pub next_attempt_at: Option<String>,
}

/// The whole answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountsState {
    pub accounts: Vec<Account>,
    pub session: Option<AccountSession>,
}

impl AccountsState {
    /// Reads the answer. A shape the TypeScript would read as empty reads as empty here.
    pub fn from_value(value: &Value) -> Self {
        let object = value.as_object();
        let accounts = object
            .and_then(|object| object.get("accounts"))
            .and_then(Value::as_array)
            .map(|entries| entries.iter().map(Account::from_value).collect())
            .unwrap_or_default();
        let session = object
            .and_then(|object| object.get("session"))
            .and_then(Value::as_object)
            .map(|session| AccountSession {
                provider: session
                    .get("provider")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                account_id: session
                    .get("accountId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                policy: session.get("policy").cloned().unwrap_or(Value::Null),
                policy_override: session.get("override").cloned().unwrap_or(Value::Null),
                recovery: session
                    .get("recovery")
                    .and_then(Value::as_object)
                    .map(|recovery| AccountRecovery {
                        reason: recovery
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        // `Attempt ${attempt + 1}` over a JSON number: `2.0` is 2, where
                        // `Value::as_i64` refused it and the line read "Attempt 1".
                        attempt: js_safe_integer(recovery.get("attempt")).unwrap_or_default(),
                        next_attempt_at: recovery
                            .get("nextAttemptAt")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    }),
            });
        Self { accounts, session }
    }

    /// `data.accounts.find((account) => account.id === session.accountId)`.
    pub fn account(&self, account_id: Option<&str>) -> Option<&Account> {
        let account_id = account_id?;
        self.accounts
            .iter()
            .find(|account| account.id.as_deref() == Some(account_id))
    }
}

/// JavaScript truthiness of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|value| value != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// A number or a string as a template string would write it, `null` and `undefined` as absent.
fn js_number_or_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Number(number) => number.as_f64().map(js_number_text),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

/// `String(value)` for a number, which is `crate::jsnum::js_number` for every family.
///
/// This file kept its own copy until 2026-09-22; it saturated an integral double above
/// `i64::MAX` and never emitted an exponent, so a reset-credits figure or a used-percentage that
/// wandered out of range printed digits JavaScript does not print.
pub use crate::jsnum::js_number as js_number_text;

/// A number as `JSON.stringify` writes it.
pub use crate::jsnum::js_number_value;

/// `Math.round`: halves go up, towards positive infinity.
pub use crate::jsnum::js_round;

/// `isWeeklyWindow`.
pub fn is_weekly_window(window: &UsageWindow) -> bool {
    window.id.as_deref() == Some("sevenDay")
        || window.limit_window_seconds.unwrap_or(0.0) >= 604_800.0
}

/// `isFiveHourWindow`.
pub fn is_five_hour_window(window: &UsageWindow) -> bool {
    window.id.as_deref() == Some("fiveHour") || window.limit_window_seconds == Some(18_000.0)
}

/// `fableWindow`, as an index into `usage`: the Fable model window, else the first model-scoped
/// one.
pub fn fable_window(usage: &[UsageWindow]) -> Option<usize> {
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

/// `accountHeadlineWindows`, as indexes into `account.usage` so a window picked twice keeps the
/// identity the TypeScript's `Set` compares by.
///
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: for Claude accounts the Fable limit is the most important number and must never be
/// hidden. Wherever a Claude account shows two percentages, show the two tightest of the weekly,
/// five-hour, and Fable limits, in that fixed order. Codex keeps its weekly window and five-hour
/// window.
pub fn account_headline_windows(account: &Account) -> Vec<usize> {
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
    // difference reads as equal, as the specification's comparator coercion makes it.
    let mut sorted = candidates.clone();
    sorted.sort_by(|left, right| {
        let difference = account.usage[*right].used_percent.unwrap_or(f64::NAN)
            - account.usage[*left].used_percent.unwrap_or(f64::NAN);
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

/// `accountUsageLabel`: `7d`, `5h`, `Fable`, `<model> 7d`, or the window's own label.
///
/// CDXC:AgentProviders 2026-09-12 DECISION:
/// User: the Fable limit percentage says "Fable:" throughout the app, without "7d".
pub fn account_usage_label(window: &UsageWindow) -> String {
    if window
        .model
        .as_deref()
        .is_some_and(|model| model.to_lowercase() == "fable")
    {
        return "Fable".to_string();
    }
    let model = window.scoped_model();
    let duration = match window.limit_window_seconds {
        Some(seconds) if seconds > 0.0 => Some(if seconds % 86_400.0 == 0.0 {
            format!("{}d", js_number_text(seconds / 86_400.0))
        } else if seconds % 3_600.0 == 0.0 {
            format!("{}h", js_number_text(seconds / 3_600.0))
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

/// `formatResetCountdown`.
///
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// User: every reset stat shows days and hours at 24 hours or more (5d 12h), and only shows
/// minutes below 24 hours (19h 50m).
pub fn format_reset_countdown(ms: f64) -> String {
    let total_minutes = (ms / 60_000.0).floor().max(0.0) as i64;
    let hours = total_minutes / 60;
    if hours >= 24 {
        return format!("{}d {}h", hours / 24, hours % 24);
    }
    let minutes = total_minutes % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// `maskAccountText`: every `local@domain` run keeps the first and last characters before the `@`
/// and replaces the rest with the fixed mask, including email-shaped account names.
///
/// CDXC:AgentProviders 2026-09-10 DECISION:
/// Hide emails keeps the first and last characters before @ and replaces every domain with the
/// same characters followed by .•••, including email-shaped account names.
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
        out.push_str("\u{2022}\u{2022}\u{2022}");
        if end - index > 1 {
            out.push(characters[end - 1]);
        }
        out.push_str("@\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}.\u{2022}\u{2022}\u{2022}");
        index = domain_end;
    }
    out
}

/// The characters JavaScript's `\s` matches.
fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{9}'..='\u{d}'
            | '\u{20}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}
