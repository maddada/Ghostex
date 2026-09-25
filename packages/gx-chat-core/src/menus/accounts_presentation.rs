//! Account copy and figures for the account panel and the account-switch card.
//!
//! Port of `packages/shared/session-chat-presentation/accounts.ts`. Labels are returned unmasked;
//! each renderer applies Hide emails itself (here, `native_accounts.rs`).

use serde::Serialize;

use crate::menus::accounts_data::{
    account_headline_windows, fable_window, format_reset_countdown, is_five_hour_window,
    is_weekly_window, js_number_text, js_number_value, js_round, Account, AccountSession,
    UsageWindow,
};
use crate::menus::time::parse_iso_millis;

/// One of the two small figures stacked beside an account's logo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountFigure {
    pub label: Option<String>,
    pub value: String,
}

/// `accountFigureWindows`, as indexes into `account.usage`.
///
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// User: Codex account badges show the five-hour percentage on the second line when that limit
/// exists; otherwise show available resets as "2rs" or "0rs". Use the main account windows so
/// Spark's separate five-hour limit does not stand in for an absent account limit. Claude figures
/// are the two tightest of weekly, five-hour, and Fable.
pub fn account_figure_windows(account: &Account) -> [Option<usize>; 2] {
    let headline = account_headline_windows(account);
    [headline.first().copied(), headline.get(1).copied()]
}

/// `accountFigures`.
pub fn account_figures(account: &Account) -> [AccountFigure; 2] {
    let [first, second] = account_figure_windows(account);
    let percent = |index: usize| {
        let window = &account.usage[index];
        format!(
            "{}%",
            js_number_text(js_round(window.used_percent.unwrap_or(f64::NAN)))
        )
    };
    let first_figure = match first {
        Some(index) => AccountFigure {
            label: account.usage[index].label.clone(),
            value: percent(index),
        },
        None => AccountFigure {
            label: None,
            value: "\u{b7}".to_string(),
        },
    };
    let second_figure = match second {
        Some(index) => AccountFigure {
            label: account.usage[index].label.clone(),
            value: percent(index),
        },
        None if account.provider.as_deref() == Some("codex") && account.reset_credits.is_some() => {
            AccountFigure {
                label: Some("Available usage resets".to_string()),
                value: format!("{}rs", account.reset_credits.clone().unwrap_or_default()),
            }
        }
        None => AccountFigure {
            label: None,
            value: "\u{b7}".to_string(),
        },
    };
    [first_figure, second_figure]
}

/// `accountResetLabel`.
pub fn account_reset_label(value: Option<&str>, now_ms: i64) -> String {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return "Reset time unavailable".to_string();
    };
    let Some(time) = parse_iso_millis(value) else {
        return "Reset time unavailable".to_string();
    };
    let remaining = time - now_ms;
    if remaining > 0 {
        format!("Resets {}", format_reset_countdown(remaining as f64))
    } else {
        "Reset due".to_string()
    }
}

/// `accountResetsLine`: one "Resets 2h 14m · 3d 6h" line for several limits, in the order given,
/// skipping limits without a reset time.
pub fn account_resets_line(windows: &[&UsageWindow], now_ms: i64) -> String {
    let remaining: Vec<i64> = windows
        .iter()
        .filter_map(|window| {
            window
                .resets_at
                .as_deref()
                .and_then(parse_iso_millis)
                .map(|at| at - now_ms)
        })
        .collect();
    if remaining.is_empty() {
        return "Reset time unavailable".to_string();
    }
    let parts: Vec<String> = remaining
        .into_iter()
        .map(|ms| {
            if ms > 0 {
                format_reset_countdown(ms as f64)
            } else {
                "due".to_string()
            }
        })
        .collect();
    format!("Resets {}", parts.join(" \u{b7} "))
}

/// `switchAccountRowDetail`.
///
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: the chat panel must not print an account's email twice. The account name is the email
/// whenever the login helper has no alias, so the second line of a Switch account row is the email
/// only when the account has a name of its own; otherwise it shows the reset countdowns of the two
/// limits in the badge, in the same order. A usage error takes the line instead, as in Settings.
/// The current-account block above drops the line because the usage bars beneath it already show
/// each reset.
pub fn switch_account_row_detail(account: &Account, now_ms: i64) -> Option<String> {
    if let Some(error) = &account.usage_error {
        return Some(error.clone());
    }
    if let Some(email) = account.email.as_deref().filter(|email| !email.is_empty()) {
        if email != account.name {
            return Some(email.to_string());
        }
    }
    let windows: Vec<&UsageWindow> = account_figure_windows(account)
        .into_iter()
        .flatten()
        .map(|index| &account.usage[index])
        .collect();
    if windows.is_empty() {
        None
    } else {
        Some(account_resets_line(&windows, now_ms))
    }
}

/// `sessionAccountPolicySummary`.
pub fn session_account_policy_summary(session: &AccountSession) -> String {
    if session.has_override() {
        return "Custom settings \u{b7} This session only".to_string();
    }
    let policy = &session.policy;
    let enabled = policy
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let at_limit = policy
        .get("atLimit")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let tail = if enabled {
        if at_limit == "wait" {
            "Wait for reset"
        } else {
            "Switch when eligible"
        }
    } else {
        "Automatic continuation off"
    };
    format!("Session defaults \u{b7} {tail}")
}

/// One row of the account policy's priority dropdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyPriorityOption {
    pub value: &'static str,
    pub label: &'static str,
}

/// `ACCOUNT_POLICY_PRIORITY_OPTIONS`.
pub const ACCOUNT_POLICY_PRIORITY_OPTIONS: [PolicyPriorityOption; 4] = [
    PolicyPriorityOption {
        value: "leastUsed",
        label: "Lowest usage first",
    },
    PolicyPriorityOption {
        value: "mostUsed",
        label: "Highest usage first",
    },
    PolicyPriorityOption {
        value: "soonestReset",
        label: "Earliest reset first",
    },
    PolicyPriorityOption {
        value: "latestReset",
        label: "Latest reset first",
    },
];

/// `accountPolicyAtLimitDescription`.
pub fn account_policy_at_limit_description(policy: &serde_json::Value) -> String {
    if policy.get("atLimit").and_then(serde_json::Value::as_str) == Some("wait") {
        "Pick up on this account when its usage resets.".to_string()
    } else {
        "Use another eligible login for this model. Wait when every account is at its limit."
            .to_string()
    }
}

/// `ACCOUNT_POLICY_RETRY_DESCRIPTION`.
pub const ACCOUNT_POLICY_RETRY_DESCRIPTION: &str = "Retry after 5, 10, 20, 40, then every 60 minutes. Login and permission requests need your attention. Stop cancels recovery for the current task.";

/// How close one limit is to running out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageLevel {
    Unknown,
    Low,
    Moderate,
    High,
    Exhausted,
}

/// One percentage card in the account-switch card.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchUsageCard {
    pub label: String,
    /// 0 to 100, or absent when the limit has no reading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<serde_json::Value>,
    pub level: UsageLevel,
    /// Countdown to the reset, `Due` once passed, or `null` without a reset time.
    pub reset: Option<String>,
}

/// One of the two accounts the switch card shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCardAccount {
    pub role: String,
    pub label: String,
    pub target: bool,
    pub usage: Vec<SwitchUsageCard>,
}

/// One numbered step of the switch.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCardStep {
    pub label: String,
    /// `done`, `active` or `pending`.
    pub state: String,
}

/// The whole card.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCardPresentation {
    /// The card's `data-phase`: gxserver's phase, or `finishing` while a success waits for `ready`.
    pub phase: String,
    pub heading: String,
    pub lede: String,
    /// The target account is bound: roles read Previous and Active and the target shows a check.
    pub verified: bool,
    pub from: SwitchCardAccount,
    pub to: SwitchCardAccount,
    /// `null` when the switch failed; the failure text replaces the steps.
    pub steps: Option<Vec<SwitchCardStep>>,
    pub failure: Option<String>,
}

/// The `accountSwitch` progress gxserver reports, as the card reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwitchProgress {
    pub id: String,
    pub provider: String,
    /// `manual` or `automatic`.
    pub source: String,
    /// `switching`, `resuming`, `continuing`, `success`, `failed` or `cancelled`.
    pub phase: String,
    pub account_ready: Option<bool>,
    pub from_account_id: Option<String>,
    pub to_account_id: Option<String>,
    pub updated_at: String,
    pub reason: Option<String>,
}

impl SwitchProgress {
    /// Reads the folded `accountSwitch` value, or `None` when there is none.
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let text = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        Some(Self {
            id: text("id").unwrap_or_default(),
            provider: text("provider").unwrap_or_default(),
            source: text("source").unwrap_or_default(),
            phase: text("phase").unwrap_or_default(),
            account_ready: object
                .get("accountReady")
                .and_then(serde_json::Value::as_bool),
            from_account_id: text("fromAccountId"),
            to_account_id: text("toAccountId"),
            updated_at: text("updatedAt").unwrap_or_default(),
            reason: text("reason").filter(|reason| !reason.is_empty()),
        })
    }
}

fn switch_usage_card(label: &str, usage: Option<&UsageWindow>, now_ms: i64) -> SwitchUsageCard {
    let used = usage
        .and_then(|window| window.used_percent)
        .filter(|percent| percent.is_finite())
        .map(|percent| percent.clamp(0.0, 100.0));
    let level = match used {
        None => UsageLevel::Unknown,
        Some(used) if used >= 100.0 => UsageLevel::Exhausted,
        Some(used) if used >= 80.0 => UsageLevel::High,
        Some(used) if used >= 50.0 => UsageLevel::Moderate,
        Some(_) => UsageLevel::Low,
    };
    let reset = usage
        .and_then(|window| window.resets_at.as_deref())
        .and_then(parse_iso_millis)
        .map(|at| {
            let remaining = at - now_ms;
            if remaining > 0 {
                format_reset_countdown(remaining as f64)
            } else {
                "Due".to_string()
            }
        });
    SwitchUsageCard {
        label: label.to_string(),
        used: used.map(js_number_value),
        level,
        reset,
    }
}

fn switch_card_account(
    account: Option<&Account>,
    provider: &str,
    role: &str,
    target: bool,
    now_ms: i64,
) -> SwitchCardAccount {
    let empty: Vec<UsageWindow> = Vec::new();
    let usage = account.map_or(&empty, |account| &account.usage);
    let main: Vec<&UsageWindow> = usage
        .iter()
        .filter(|window| window.scoped_model().is_none())
        .collect();
    let mut cards = vec![
        switch_usage_card(
            "5h limit",
            main.iter()
                .copied()
                .find(|window| is_five_hour_window(window)),
            now_ms,
        ),
        switch_usage_card(
            "7d limit",
            main.iter().copied().find(|window| is_weekly_window(window)),
            now_ms,
        ),
    ];
    if provider == "claude" {
        cards.push(switch_usage_card(
            "Fable",
            fable_window(usage).map(|index| &usage[index]),
            now_ms,
        ));
    }
    let label = match account.and_then(|account| account.email.as_deref()) {
        Some(email) if !email.is_empty() => email.to_string(),
        _ => if target {
            "Selected account"
        } else {
            "Current CLI login"
        }
        .to_string(),
    };
    SwitchCardAccount {
        role: role.to_string(),
        label,
        target,
        usage: cards,
    }
}

/// `accountSwitchCardPresentation`.
///
/// CDXC:AgentProviders 2026-09-12 DECISION:
/// User: center the account-switch card in chat until the switch completes; only add this card and
/// leave the Switch Account menu unchanged. Show both accounts with their three usage windows,
/// including Fable. Omit "used" and "limit reached" captions. The steps keep one animated line on
/// the active step, never a spinner. The automatic third step is "Continue Session"; manual
/// switches wait for the user's next message. 2026-09-23: the proximity colours (red at 100%) and
/// the large tiles were replaced by the compact-tiles design the user picked, with usage in
/// neutral ink; see packages/core-ui/accounts/account-switch-card.css. No heading spinner, repeated status
/// above the composer, View terminal button, draft reassurance, or bottom bar. Show plain provider
/// logos in this card, without the account's two-character indicator inside them. Identify each
/// account by its real email on one line, respecting Hide emails, rather than account names or the
/// preview's former invented aliases.
///
/// `ready` is the chat's confirmation that the second account is bound and usable. A `success`
/// phase without it keeps the last step active and the "Switching" heading, so the card never
/// announces completion before the account is ready.
pub fn switch_card_presentation(
    progress: &SwitchProgress,
    accounts: &[Account],
    ready: bool,
    now_ms: i64,
) -> Option<SwitchCardPresentation> {
    let phase = progress.phase.as_str();
    if phase == "cancelled" {
        return None;
    }
    let settled = phase == "success" && ready;
    let finishing = phase == "success" && !ready;
    let verified =
        progress.account_ready == Some(true) || phase == "success" || phase == "continuing";
    let provider_name = if progress.provider == "claude" {
        "Claude"
    } else {
        "Codex"
    };
    let mut labels = vec![
        "Switch account".to_string(),
        "Resume conversation".to_string(),
    ];
    if progress.source == "automatic" {
        labels.push("Continue Session".to_string());
    }
    let current: i64 = if phase == "switching" {
        0
    } else if phase == "resuming" {
        1
    } else if finishing {
        labels.len() as i64 - 1
    } else {
        2
    };
    let heading = if phase == "failed" {
        if verified {
            "Couldn\u{2019}t continue the session"
        } else {
            "Couldn\u{2019}t complete the switch"
        }
        .to_string()
    } else if settled {
        "Account switched".to_string()
    } else {
        format!("Switching {provider_name} account")
    };
    let lede = if phase == "failed" {
        if verified {
            "The account is ready, but continuation needs attention."
        } else {
            "The new account isn\u{2019}t ready yet."
        }
    } else if settled {
        if progress.source == "automatic" {
            "Your task is continuing on the new account."
        } else {
            "Ready whenever you are. Send your next message."
        }
    } else if finishing {
        "Loading your conversation on the new account."
    } else if progress.source == "automatic" {
        "Usage limit reached. Continuing on an available account."
    } else {
        "Your conversation will be ready for your next message."
    }
    .to_string();
    let find = |id: Option<&String>| {
        id.and_then(|id| {
            accounts
                .iter()
                .find(|account| account.id.as_deref() == Some(id.as_str()))
        })
    };
    let steps = if phase == "failed" {
        None
    } else {
        Some(
            labels
                .iter()
                .enumerate()
                .map(|(index, label)| {
                    let done = settled || (index as i64) < current;
                    SwitchCardStep {
                        label: label.clone(),
                        state: if done {
                            "done"
                        } else if index as i64 == current {
                            "active"
                        } else {
                            "pending"
                        }
                        .to_string(),
                    }
                })
                .collect(),
        )
    };
    let failure = if phase == "failed" {
        Some(progress.reason.clone().unwrap_or_else(|| {
            "We couldn\u{2019}t confirm the new login. Retry this switch or use Switch Account to choose another account.".to_string()
        }))
    } else {
        None
    };
    Some(SwitchCardPresentation {
        phase: if finishing {
            "finishing".to_string()
        } else {
            progress.phase.clone()
        },
        heading,
        lede,
        verified,
        from: switch_card_account(
            find(progress.from_account_id.as_ref()),
            &progress.provider,
            if verified {
                "Previous account"
            } else {
                "Current account"
            },
            false,
            now_ms,
        ),
        to: switch_card_account(
            find(progress.to_account_id.as_ref()),
            &progress.provider,
            if verified {
                "Active account"
            } else {
                "Switching to"
            },
            true,
            now_ms,
        ),
        steps,
        failure,
    })
}
