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
    account_usage_label, js_number_text, js_number_value, js_round, mask_account_text, Account,
    AccountsState,
};
use crate::menus::accounts_presentation::{
    account_figures, account_policy_at_limit_description, account_reset_label,
    session_account_policy_summary, switch_account_row_detail, switch_card_presentation,
    PolicyPriorityOption, SwitchCardAccount, SwitchCardPresentation, SwitchProgress,
    ACCOUNT_POLICY_PRIORITY_OPTIONS, ACCOUNT_POLICY_RETRY_DESCRIPTION,
};
use crate::menus::context_usage::{context_meter_usage, format_context_tokens};

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

/// One limit bar under the current account.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelUsageBar {
    pub label: String,
    /// `Math.max(0, Math.min(100, usedPercent))`, written as the engine writes a number.
    pub percent: Value,
    pub reset: String,
}

/// The context meter shown under the account's own limits.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelContext {
    pub percent: Value,
    pub value: String,
    pub tokens: String,
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
    pub usage_error: Option<String>,
    pub usage: Vec<PanelUsageBar>,
    pub context: Option<PanelContext>,
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
    context_usage: Option<&Value>,
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
    let context = context_meter_usage(context_usage, false);
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
            usage_error: current
                .and_then(|account| account.usage_error.as_deref())
                .map(text),
            usage: current
                .map(|account| account.usage.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|window| PanelUsageBar {
                    label: format!(
                        "{}: {}%",
                        account_usage_label(window),
                        js_number_text(js_round(window.used_percent.unwrap_or(f64::NAN)))
                    ),
                    percent: js_number_value(
                        window.used_percent.unwrap_or(f64::NAN).clamp(0.0, 100.0),
                    ),
                    reset: account_reset_label(window.resets_at.as_deref(), now_ms),
                })
                .collect(),
            context: context.map(|context| PanelContext {
                percent: js_number_value(context.used_percentage.unwrap_or(0.0)),
                value: match context.used_percentage {
                    None => "Usage unavailable".to_string(),
                    Some(percent) => format!("{}%", js_number_text(js_round(percent))),
                },
                tokens: format!(
                    "{} / {}",
                    format_context_tokens(context.used_tokens),
                    format_context_tokens(context.window_size)
                ),
            }),
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
