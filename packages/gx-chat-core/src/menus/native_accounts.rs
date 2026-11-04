//! GPUI chat's More actions > Switch Account panel and account-switch card, projected from the
//! same rules and copy the React `SessionAccountsPanel` and `AccountSwitchCard` used.
//!
//! Port of `packages/shared/session-chat-controller/native-accounts.ts`. Text arrives masked for
//! Hide emails, so the renderers only lay it out.
//!
//! SEE-ALSO: apps/desktop/src/app/native_chat/option_menu/accounts.rs,
//! apps/desktop/src/app/native_chat/account_switch_card.rs.

use serde::Serialize;
use serde_json::Value;

use crate::menus::accounts_data::{
    account_usage_label, format_reset_countdown, js_number_text, js_number_value, js_round,
    mask_account_text, Account, AccountsState, UsageWindow,
};
use crate::menus::accounts_presentation::{
    account_figures, account_policy_at_limit_description, account_reset_label,
    session_account_policy_summary, switch_account_row_detail, switch_card_presentation,
    PolicyPriorityOption, SwitchCardAccount, SwitchCardPresentation, SwitchProgress,
    ACCOUNT_POLICY_PRIORITY_OPTIONS, ACCOUNT_POLICY_RETRY_DESCRIPTION,
};

/// `nativeContextText`: masks account text when Hide emails is on.
pub fn account_text(hide_account_emails: bool, text: &str) -> String {
    if hide_account_emails {
        mask_account_text(text)
    } else {
        text.to_string()
    }
}

/// `identity(account)`: the provider logo and the two figures stacked beside it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountIdentity {
    /// `account.provider`, absent when the answer carries none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub figures: Vec<String>,
}

fn identity(account: &Account) -> AccountIdentity {
    AccountIdentity {
        provider: account.provider.clone(),
        figures: account_figures(account)
            .iter()
            .map(|figure| figure.value.clone())
            .collect(),
    }
}

/// One limit bar under an account.
///
/// CDXC:AgentProviders 2026-10-28 DECISION:
/// User: the Accounts & limits panel was too tall; show an account's usage as one row of small
/// columns (label and percent over a thin bar) instead of a full-width bar per limit. The compact
/// column writes `name` and `value` on one line and keeps the reset for its hover tooltip (desktop)
/// or as `resetIn` under the bar (phone). Compact places name a model's weekly window by the model
/// alone ("Fable"), and the account limits stay 5h and 7d.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelUsageBar {
    /// "5h: 16%", for older clients and the tooltip.
    pub label: String,
    /// The compact column's label: "5h", "7d", "Fable".
    pub name: String,
    /// "16%".
    pub value: String,
    /// `Math.max(0, Math.min(100, usedPercent))`, written as the engine writes a number.
    pub percent: Value,
    /// "Resets 2h 36m", "Reset due" or "Reset time unavailable".
    pub reset: String,
    /// The bare countdown ("2h 36m", "due"), absent without a reset time.
    pub reset_in: Option<String>,
}

/// `name` for a window: a model's weekly window is the model alone in compact places.
fn compact_usage_label(window: &UsageWindow) -> String {
    let label = account_usage_label(window);
    match window.scoped_model() {
        Some(model) => label
            .strip_prefix(model)
            .filter(|rest| *rest == " 7d")
            .map_or(label.clone(), |_| model.to_string()),
        None => label,
    }
}

/// The account's limits as compact columns, in the order the answer lists them.
pub fn panel_usage_bars(account: &Account, now_ms: i64) -> Vec<PanelUsageBar> {
    account
        .usage
        .iter()
        .map(|window| {
            let value = format!(
                "{}%",
                js_number_text(js_round(window.used_percent.unwrap_or(f64::NAN)))
            );
            PanelUsageBar {
                label: format!("{}: {value}", account_usage_label(window)),
                name: compact_usage_label(window),
                value,
                percent: js_number_value(window.used_percent.unwrap_or(f64::NAN).clamp(0.0, 100.0)),
                reset: account_reset_label(window.resets_at.as_deref(), now_ms),
                reset_in: window
                    .resets_at
                    .as_deref()
                    .and_then(crate::menus::time::parse_iso_millis)
                    .map(|at| at - now_ms)
                    .map(|remaining| {
                        if remaining > 0 {
                            format_reset_countdown(remaining as f64)
                        } else {
                            "due".to_string()
                        }
                    }),
            }
        })
        .collect()
}

/// What is wrong with the account the session is on, on its row in the panel.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelProblem {
    pub message: String,
    /// The button at the end of the row, which opens Settings > Accounts. Absent when Settings
    /// cannot fix it (a reading Ghostex keeps retrying, a limit that resets by itself).
    pub action: Option<String>,
}

/// CDXC:AgentProviders 2026-10-31 DECISION:
/// User, on the panel's current account: "please make the currently active account somehow say that this is active now. and show if there's an error with it here". gxserver's usage error covers most states in its own words, the words the Switch account rows show, and a login state without one takes the same words here. Limits are only named when a main window (not a model's, not Extra usage) is used up, the rule `limited_windows` in server/src/accounts/cli.rs uses; a used-up account has no action because the rows under it are the way out. `Reconnect` follows the Switch account rows' rule: any status but ready, except loading (still being read) and turned off in Settings, where signing in again is not the fix.
fn current_account_problem(account: &Account, now_ms: i64) -> Option<PanelProblem> {
    let status = account.status.as_deref().unwrap_or_default();
    let problem = |message: String, action: Option<&str>| {
        Some(PanelProblem {
            message,
            action: action.map(str::to_string),
        })
    };
    match status {
        "disabled" => problem(
            "This account is turned off in Settings.".to_string(),
            Some("Open Settings"),
        ),
        "ready" | "loading" | "" => {
            if let Some(reason) = &account.disabled_reason {
                problem(reason.clone(), None)
            } else if let Some(error) = &account.usage_error {
                problem(error.clone(), asks_to_sign_in(error).then_some("Reconnect"))
            } else {
                let limited = limited_windows(account, now_ms);
                if limited.is_empty() {
                    None
                } else {
                    problem(format!("{} limit reached.", limited.join(" and ")), None)
                }
            }
        }
        other => {
            let fallback = match other {
                "identityChanged" => {
                    "The saved login belongs to a different account. Reconnect this account."
                }
                "unavailable" => "This account isn't signed in on this computer. Reconnect it.",
                _ => "This account needs to sign in again. Reconnect it.",
            };
            problem(
                account
                    .usage_error
                    .clone()
                    .unwrap_or_else(|| fallback.to_string()),
                Some("Reconnect"),
            )
        }
    }
}

/// A ready account's usage error that says signing in again fixes it. gxserver keeps a Codex login
/// whose usage read failed this way as ready, so the words are all that says so.
fn asks_to_sign_in(error: &str) -> bool {
    error.contains("Reconnect") || error.contains("Sign in again")
}

/// `current_account_problem` for an account as `/api/agentAccounts` lists it, for a surface drawing
/// one account outside this panel (the sidebar's usage panel).
pub fn account_problem(account: &Value, now_ms: i64) -> Option<PanelProblem> {
    current_account_problem(&Account::from_value(account), now_ms)
}

/// The compact names of the account's used-up main windows that have not reset yet.
fn limited_windows(account: &Account, now_ms: i64) -> Vec<String> {
    account
        .usage
        .iter()
        .filter(|window| window.scoped_model().is_none() && window.id.as_deref() != Some("spend"))
        .filter(|window| {
            window.used_percent.is_some_and(|used| used >= 100.0)
                && window
                    .resets_at
                    .as_deref()
                    .and_then(crate::menus::time::parse_iso_millis)
                    .is_none_or(|at| at > now_ms)
        })
        .map(compact_usage_label)
        .collect()
}

/// The automatic-continuation recovery line.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelRecovery {
    pub reason: String,
    pub next: Option<String>,
}

/// One row of the Switch account list.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelOtherAccount {
    #[serde(flatten)]
    pub identity: AccountIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub detail: Option<String>,
    /// The email beside the name when the account has a name of its own.
    pub email: Option<String>,
    /// The usage error that takes the place of the usage row.
    pub usage_error: Option<String>,
    /// The account's limits, drawn as the same compact row as the current account's.
    pub usage: Vec<PanelUsageBar>,
    pub ready: bool,
    pub action: String,
}

/// The session's automatic-continuation settings.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelPolicy {
    pub summary: String,
    pub custom: bool,
    pub value: Value,
    pub at_limit_description: String,
    pub priority_label: String,
    pub priorities: [PolicyPriorityOption; 4],
    pub retry_description: &'static str,
}

/// The current session's half of the panel.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSession {
    pub recovery: Option<PanelRecovery>,
    pub current: Option<AccountIdentity>,
    pub name: String,
    pub email: Option<String>,
    /// The account's id, for the Reconnect that starts its sign-in.
    pub account_id: Option<String>,
    /// "Active" beside the name when the session is on a known account.
    pub active_label: Option<String>,
    pub problem: Option<PanelProblem>,
    pub usage: Vec<PanelUsageBar>,
    pub switch_heading: String,
    pub others: Vec<PanelOtherAccount>,
    pub policy: PanelPolicy,
}

/// What the Switch Account panel draws.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AccountPanel {
    /// The provider has no saved account yet, so the panel offers Add account.
    NoAccounts { kind: &'static str },
    /// The panel proper, with or without a session to show.
    Panel(Box<PanelBody>),
}

/// The panel's own body.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelBody {
    pub kind: &'static str,
    pub busy: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub session: Option<PanelSession>,
}

/// `nativeAccountPanel`.
pub fn native_account_panel(
    data: Option<&AccountsState>,
    error: Option<&str>,
    busy: bool,
    hide_account_emails: bool,
    chat_context: &crate::ChatContext,
) -> AccountPanel {
    let now_ms = chat_context.now_millis();
    let text = |value: &str| account_text(hide_account_emails, value);
    let session = data.and_then(|data| data.session.as_ref());
    // Same gate as SessionAccountsPanel: without a saved account for this provider, offer Add
    // account.
    if let (Some(data), Some(session)) = (data, session) {
        let has_registered = data.accounts.iter().any(|account| {
            account.registered && account.provider.as_deref() == session.provider.as_deref()
        });
        if !has_registered {
            return AccountPanel::NoAccounts { kind: "noAccounts" };
        }
    }
    let error_text = error.filter(|error| !error.is_empty()).map(text);
    let body = |message: Option<String>, session: Option<PanelSession>| {
        AccountPanel::Panel(Box::new(PanelBody {
            kind: "panel",
            busy,
            error: error_text.clone(),
            message,
            session,
        }))
    };
    let Some(data) = data else {
        return body(
            Some(
                if busy {
                    "Reading accounts and usage\u{2026}"
                } else {
                    "Could not load accounts."
                }
                .to_string(),
            ),
            None,
        );
    };
    let Some(session) = session else {
        return body(
            Some("Account management supports Claude and Codex sessions.".to_string()),
            None,
        );
    };
    let current = data.account(session.account_id.as_deref());
    let policy = session.effective_policy().clone();
    let priority = policy
        .get("priority")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let others: Vec<PanelOtherAccount> = data
        .accounts
        .iter()
        .filter(|account| {
            account.registered
                && !account.user_disabled
                && account.provider.as_deref() == session.provider.as_deref()
                && account.id.as_deref() != session.account_id.as_deref()
        })
        .map(|account| {
            let detail = switch_account_row_detail(account, now_ms);
            let ready = account.status.as_deref() == Some("ready");
            PanelOtherAccount {
                identity: identity(account),
                id: account.id.clone(),
                name: text(&account.name),
                detail: detail.map(|detail| text(&detail)),
                email: account
                    .email
                    .as_deref()
                    .filter(|email| !email.is_empty() && *email != account.name)
                    .map(text),
                usage_error: account.usage_error.as_deref().map(text),
                usage: panel_usage_bars(account, now_ms),
                ready,
                action: if ready {
                    "Use account \u{2192}"
                } else {
                    "Reconnect"
                }
                .to_string(),
            }
        })
        .collect();
    body(
        None,
        Some(PanelSession {
            recovery: session.recovery.as_ref().map(|recovery| PanelRecovery {
                reason: text(&recovery.reason),
                next: recovery.next_attempt_at.as_deref().map(|at| {
                    format!(
                        "Next attempt: {} \u{b7} Attempt {}",
                        locale_date_time(at, chat_context),
                        recovery.attempt + 1
                    )
                }),
            }),
            current: current.map(identity),
            name: text(
                current
                    .map(|account| account.name.as_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Choose an account"),
            ),
            email: current.and_then(|account| {
                account
                    .email
                    .as_deref()
                    .filter(|email| !email.is_empty() && *email != account.name)
                    .map(text)
            }),
            account_id: current.and_then(|account| account.id.clone()),
            active_label: current.map(|_| "Active".to_string()),
            problem: current
                .and_then(|account| current_account_problem(account, now_ms))
                .map(|problem| PanelProblem {
                    message: text(&problem.message),
                    action: problem.action,
                }),
            usage: current
                .map(|account| panel_usage_bars(account, now_ms))
                .unwrap_or_default(),
            switch_heading: if current.is_some() {
                "Switch account"
            } else {
                "Accounts"
            }
            .to_string(),
            others,
            policy: PanelPolicy {
                summary: session_account_policy_summary(session),
                custom: session.has_override(),
                at_limit_description: account_policy_at_limit_description(&policy),
                priority_label: ACCOUNT_POLICY_PRIORITY_OPTIONS
                    .iter()
                    .find(|option| option.value == priority)
                    .map_or_else(|| priority.to_string(), |option| option.label.to_string()),
                priorities: ACCOUNT_POLICY_PRIORITY_OPTIONS,
                retry_description: ACCOUNT_POLICY_RETRY_DESCRIPTION,
                value: policy,
            },
        }),
    )
}

/// The card drawn over the pane while `accountStatus.visible` holds.
///
/// A failed switch shows the account request's own error when there is one, and offers Retry for
/// its target account.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSwitchCard {
    #[serde(flatten)]
    pub card: SwitchCardPresentation,
    pub id: String,
    pub provider: String,
    pub retry: Option<SwitchRetry>,
    /// A failed switch shows a close button, which sends `accounts` / `dismissSwitch` so gxserver
    /// marks it cancelled for every client.
    pub dismissible: bool,
}

/// The Retry button on a failed switch.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchRetry {
    pub account_id: String,
    pub busy: bool,
}

/// `nativeAccountSwitchCard`.
#[allow(clippy::too_many_arguments)]
pub fn native_account_switch_card(
    progress: &SwitchProgress,
    data: Option<&AccountsState>,
    error: Option<&str>,
    ready: bool,
    busy: bool,
    hide_account_emails: bool,
    now_ms: i64,
) -> Option<AccountSwitchCard> {
    let text = |value: &str| account_text(hide_account_emails, value);
    let mut shown = progress.clone();
    if let Some(error) = error.filter(|error| !error.is_empty()) {
        if progress.phase == "failed" {
            shown.reason = Some(error.to_string());
        }
    }
    let empty: Vec<Account> = Vec::new();
    let accounts = data.map_or(&empty, |data| &data.accounts);
    let mut card = switch_card_presentation(&shown, accounts, ready, now_ms)?;
    let relabel = |account: &mut SwitchCardAccount| {
        account.label = text(&account.label);
    };
    relabel(&mut card.from);
    relabel(&mut card.to);
    card.failure = card.failure.as_deref().map(text);
    let retry = match (&progress.to_account_id, progress.phase.as_str()) {
        (Some(account_id), "failed") if !account_id.is_empty() => Some(SwitchRetry {
            account_id: account_id.clone(),
            busy,
        }),
        _ => None,
    };
    Some(AccountSwitchCard {
        card,
        id: progress.id.clone(),
        provider: progress.provider.clone(),
        retry,
        dismissible: progress.phase == "failed",
    })
}

/// `new Date(value).toLocaleString()` as the engine renders it with no locale argument.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// This is the one locale-dependent call in the whole chat brain (`native-accounts.ts:65`). The
/// core reads no locale, so it writes the `en-US` form QuickJS produced for the default locale
/// rather than guessing the user's. It reaches a document only on the recovery line of a session
/// whose automatic continuation is retrying.
fn locale_date_time(value: &str, context: &crate::ChatContext) -> String {
    use crate::menus::time::parse_iso_millis;
    let Some(millis) = parse_iso_millis(value) else {
        return "Invalid Date".to_string();
    };
    // The host's own locale rendering when it supplied one for this stamp.
    if let Some(text) = context.formatted_time(crate::FormattedTimeStyle::AccountDateTime, millis) {
        return text.to_string();
    }
    let iso = crate::menus::time::iso_from_millis(millis);
    // `M/D/YYYY, h:mm:ss AM` from the ISO parts, which are UTC; the host's offset is not applied
    // because the engine's own answer here is already environment dependent.
    let (date, time) = iso.split_at(10);
    let year: i64 = date[0..4].parse().unwrap_or_default();
    let month: u32 = date[5..7].parse().unwrap_or_default();
    let day: u32 = date[8..10].parse().unwrap_or_default();
    let hour: u32 = time[1..3].parse().unwrap_or_default();
    let minute = &time[4..6];
    let second = &time[7..9];
    let meridiem = if hour < 12 { "AM" } else { "PM" };
    let clock = match hour % 12 {
        0 => 12,
        other => other,
    };
    format!("{month}/{day}/{year}, {clock}:{minute}:{second} {meridiem}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000_000;

    fn window(
        id: &str,
        model: Option<&str>,
        seconds: f64,
        percent: f64,
        reset_ms: Option<i64>,
    ) -> UsageWindow {
        UsageWindow {
            id: Some(id.to_string()),
            label: None,
            used_percent: Some(percent),
            limit_window_seconds: Some(seconds),
            resets_at: reset_ms.map(crate::menus::time::iso_from_millis),
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn usage_bars_are_compact_columns_with_5h_7d_and_the_model_name() {
        let account = Account {
            provider: Some("claude".into()),
            usage: vec![
                window(
                    "fiveHour",
                    None,
                    18_000.0,
                    16.4,
                    Some(NOW + (2 * 60 + 36) * 60_000),
                ),
                window(
                    "sevenDay",
                    None,
                    604_800.0,
                    40.0,
                    Some(NOW + (5 * 24 + 4) * 3_600_000),
                ),
                window("sevenDayFable", Some("Fable"), 604_800.0, 0.0, None),
                window(
                    "sevenDaySonnet",
                    Some("Sonnet"),
                    604_800.0,
                    3.0,
                    Some(NOW - 1),
                ),
            ],
            ..Account::default()
        };
        let bars = panel_usage_bars(&account, NOW);
        let columns: Vec<(&str, &str, Option<&str>)> = bars
            .iter()
            .map(|bar| {
                (
                    bar.name.as_str(),
                    bar.value.as_str(),
                    bar.reset_in.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            columns,
            [
                ("5h", "16%", Some("2h 36m")),
                ("7d", "40%", Some("5d 4h")),
                ("Fable", "0%", None),
                ("Sonnet", "3%", Some("due")),
            ]
        );
        assert_eq!(bars[0].label, "5h: 16%");
        assert_eq!(bars[0].reset, "Resets 2h 36m");
        assert_eq!(bars[2].reset, "Reset time unavailable");
        assert_eq!(bars[3].label, "Sonnet 7d: 3%");
    }

    #[test]
    fn a_model_window_other_than_7d_keeps_its_duration() {
        let spark = window("codexSpark", Some("Spark"), 18_000.0, 12.0, None);
        assert_eq!(compact_usage_label(&spark), "Spark 5h");
    }

    fn current(status: &str) -> Account {
        Account {
            id: Some("claude-1".into()),
            provider: Some("claude".into()),
            name: "MS".into(),
            email: Some("someone@example.com".into()),
            registered: true,
            status: Some(status.into()),
            ..Account::default()
        }
    }

    fn panel(account: Account) -> Value {
        let accounts = AccountsState {
            accounts: vec![account],
            session: Some(crate::menus::accounts_data::AccountSession {
                provider: Some("claude".into()),
                account_id: Some("claude-1".into()),
                ..Default::default()
            }),
        };
        let panel = native_account_panel(
            Some(&accounts),
            None,
            false,
            true,
            &crate::ChatContext::default(),
        );
        serde_json::to_value(panel).unwrap()["session"].clone()
    }

    #[test]
    fn the_current_account_says_it_is_active_and_a_healthy_one_shows_no_problem() {
        let session = panel(current("ready"));
        assert_eq!(session["activeLabel"], "Active");
        assert_eq!(session["problem"], Value::Null);
        // The context row and its data left the panel.
        assert!(session.get("context").is_none());
    }

    #[test]
    fn a_session_without_a_known_account_is_not_called_active() {
        let mut account = current("ready");
        account.id = Some("another".into());
        let session = panel(account);
        assert_eq!(session["name"], "Choose an account");
        assert_eq!(session["activeLabel"], Value::Null);
        assert_eq!(session["problem"], Value::Null);
    }

    #[test]
    fn every_login_state_shows_its_words_and_reconnect() {
        let cases = [
            (
                "loginRequired",
                Some("The saved login expired. Reconnect this account."),
                "The saved login expired. Reconnect this account.",
            ),
            (
                "loginRequired",
                Some("No saved login was found for this account. Reconnect it."),
                "No saved login was found for this account. Reconnect it.",
            ),
            // Codex and an email-less Claude login have no usage error to borrow.
            (
                "loginRequired",
                None,
                "This account needs to sign in again. Reconnect it.",
            ),
            (
                "identityChanged",
                None,
                "The saved login belongs to a different account. Reconnect this account.",
            ),
            (
                "unavailable",
                Some("This account isn't signed in on this computer. Reconnect it."),
                "This account isn't signed in on this computer. Reconnect it.",
            ),
        ];
        for (status, error, message) in cases {
            let mut account = current(status);
            account.usage_error = error.map(str::to_string);
            let session = panel(account);
            assert_eq!(
                session["problem"],
                json!({"message": message, "action": "Reconnect"}),
                "{status}"
            );
            assert_eq!(session["activeLabel"], "Active");
        }
    }

    #[test]
    fn a_ready_account_shows_a_usage_error_without_an_action() {
        let mut account = current("ready");
        account.usage_error =
            Some("Usage could not be read yet. Ghostex keeps trying.".to_string());
        assert_eq!(
            panel(account)["problem"],
            json!({
                "message": "Usage could not be read yet. Ghostex keeps trying.",
                "action": null,
            })
        );
    }

    /// Codex reports these with the account still ready, and each asks for a sign-in.
    #[test]
    fn a_ready_account_whose_error_asks_for_a_sign_in_gets_reconnect() {
        for error in [
            "Sign in again to read usage.",
            "This account's login identity changed. Reconnect it.",
        ] {
            let mut account = current("ready");
            account.usage_error = Some(error.to_string());
            assert_eq!(
                panel(account)["problem"],
                json!({"message": error, "action": "Reconnect"})
            );
        }
    }

    #[test]
    fn the_usage_panel_reads_the_same_problem_from_the_listed_account() {
        let problem = account_problem(
            &json!({"status": "unavailable", "usageError": "This account isn't signed in on this computer. Reconnect it."}),
            NOW,
        )
        .unwrap();
        assert_eq!(problem.action.as_deref(), Some("Reconnect"));
        let turned_off = account_problem(&json!({"status": "disabled"}), NOW).unwrap();
        assert_eq!(turned_off.action.as_deref(), Some("Open Settings"));
        assert_eq!(account_problem(&json!({"status": "ready"}), NOW), None);
        assert_eq!(account_problem(&json!({"status": "loading"}), NOW), None);
    }

    #[test]
    fn a_turned_off_account_points_at_settings_and_a_refused_one_says_why() {
        assert_eq!(
            panel(current("disabled"))["problem"],
            json!({
                "message": "This account is turned off in Settings.",
                "action": "Open Settings",
            })
        );
        let mut refused = current("ready");
        refused.disabled_reason =
            Some("Its organization disabled Claude subscription access.".to_string());
        assert_eq!(
            panel(refused)["problem"]["message"],
            "Its organization disabled Claude subscription access."
        );
    }

    #[test]
    fn a_used_up_main_window_says_the_limit_is_reached() {
        let at = |model, id, percent| window(id, model, 604_800.0, percent, None);
        let mut account = current("ready");
        account.usage = vec![
            window("fiveHour", None, 18_000.0, 100.0, None),
            at(None, "sevenDay", 100.0),
        ];
        assert_eq!(
            panel(account)["problem"],
            json!({"message": "5h and 7d limit reached.", "action": null})
        );
        // A model's own window and Extra usage don't stop the account.
        let mut account = current("ready");
        account.usage = vec![at(Some("Fable"), "fable", 100.0), at(None, "spend", 100.0)];
        assert_eq!(panel(account)["problem"], Value::Null);
    }

    #[test]
    fn hide_emails_masks_a_problem_that_names_one() {
        let mut account = current("loginRequired");
        account.usage_error = Some("someone@example.com must sign in.".to_string());
        let message = panel(account)["problem"]["message"].clone();
        assert!(
            !message.as_str().unwrap().contains("someone@example.com"),
            "{message}"
        );
    }
}
