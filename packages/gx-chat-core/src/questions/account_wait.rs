//! The usage-limit card while every account is full.
//!
//! CDXC:AgentProviders 2026-10-11 DECISION:
//! User, about a session that sat quietly at its usage limit while every account was full: "dont know what u mean but fix this best way i trust u". While gxserver's recovery waits with no account that has room, it carries `accountWait` (`limit`, `resetsAt`, `account`, each optional) on the session's usage-limit notice, and the card turns into a warning that says which limit was hit, that every account is full, and when work continues: at the soonest reset among the accounts, or as soon as an account has room. The card keeps the usage-limit notice's own Switch account button, and it goes away with the notice when the session switches or resumes.
//! SEE-ALSO: server/src/accounts/limit_wait.rs (the facts and the bell row), apps/desktop/src/app/native_chat/notice.rs and apps/mobile/app/src/chat/native/cards/NoticeCard.tsx (both draw the decorated notice as is).

use serde_json::{json, Map, Value};

use crate::extras::save_markdown_paths::civil_from_days;
use crate::jstime::parse_iso_millis;
use crate::menus::accounts_data::mask_account_text;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Rewrites a usage-limit notice that carries `accountWait` into the every-account-full warning.
pub fn decorate(
    object: &mut Map<String, Value>,
    hide_account_emails: bool,
    now_ms: f64,
    utc_offset_minutes: i32,
) {
    if object.get("kind").and_then(Value::as_str) != Some("usageLimit") {
        return;
    }
    let Some(wait) = object
        .get("accountWait")
        .and_then(Value::as_object)
        .cloned()
    else {
        return;
    };
    let words = wait_words(&wait, hide_account_emails, now_ms, utc_offset_minutes);
    object.insert("title".to_string(), json!(words.title));
    object.insert("detail".to_string(), json!(words.detail));
    object.insert("tone".to_string(), json!("warning"));
    // The card's main button (Q2977/Q2978 in model_fallback.rs): ahead of the notice's own
    // actions, and `primary` so the renderers draw it filled and Switch account beside it.
    if let Some(action) = words.action {
        let mut actions = object
            .get("actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        actions.insert(
            0,
            json!({
                "id": "continueWithModel",
                "label": action.label,
                "kind": "continueWithModel",
                "primary": true,
                "answer": action.answer,
            }),
        );
        object.insert("actions".to_string(), Value::Array(actions));
    }
}

/// What the card and the Accounts & limits panel's waiting box say while every account is full.
#[derive(Clone, Debug, PartialEq)]
pub struct WaitWords {
    pub title: String,
    pub detail: String,
    pub action: Option<ContinueAction>,
}

/// "Continue with Opus 5.5 on ST": the model and account gxserver found room on.
#[derive(Clone, Debug, PartialEq)]
pub struct ContinueAction {
    pub label: String,
    /// `answerSessionChatPrompt`'s answer; with `operation` set, `agentAccounts`' request.
    pub answer: Value,
}

/// CDXC:AgentProviders 2026-11-13 DECISION:
/// User picked "one sentence" (Q2977 A) and the main button (Q2978 A) after a Fable 5.2 chat said
/// "7d limit reached on every account" while ST showed 86% (its Fable window was full and SP was
/// manual only): the title names the model no account has room for, one sentence names why each
/// other account is out, the next says when work continues, and a model with room on an account
/// becomes the card's main button. Facts from a gxserver that predates `model` keep the old words.
/// SEE-ALSO: server/src/accounts/model_fallback.rs, server/src/accounts/limit_wait.rs (`describe`).
pub fn wait_words(
    wait: &Map<String, Value>,
    hide_account_emails: bool,
    now_ms: f64,
    utc_offset_minutes: i32,
) -> WaitWords {
    let text = |key: &str| {
        wait.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let name = |value: &str| {
        if hide_account_emails {
            mask_account_text(value)
        } else {
            value.to_string()
        }
    };
    let model = text("model");
    let limit = text("limit").unwrap_or("Usage limit");
    let reset = text("resetsAt")
        .and_then(|at| parse_iso_millis(at, utc_offset_minutes))
        .filter(|at| *at > now_ms);
    let when = match (reset, text("account"), model) {
        (Some(at), Some(account), Some(_)) => format!(
            "It continues {}, when {}\u{2019}s {}resets.",
            local_time(at, now_ms, utc_offset_minutes),
            name(account),
            text("accountLimit")
                .map_or_else(|| "limit ".to_string(), |limit| format!("{limit} limit "))
        ),
        (Some(at), Some(account), None) => format!(
            "It continues automatically {}, when {}\u{2019}s limit resets.",
            local_time(at, now_ms, utc_offset_minutes),
            name(account)
        ),
        (_, _, Some(_)) => "It continues as soon as an account has room.".to_string(),
        _ => "It continues automatically as soon as an account has room.".to_string(),
    };
    let Some(model) = model else {
        return WaitWords {
            title: format!("{limit} reached on every account"),
            detail: format!(
                "Every account this session can switch to is full, so it waits instead of switching. {when}"
            ),
            action: None,
        };
    };
    let reasons: Vec<String> = wait
        .get("blocked")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let account = name(entry.get("account")?.as_str()?.trim());
            match entry.get("why")?.as_str()? {
                "manual" => Some(format!("{account} is manual only")),
                "signIn" => Some(format!("{account} needs to sign in again")),
                "full" => Some(match entry.get("limit").and_then(Value::as_str) {
                    Some(limit) => format!("{account}\u{2019}s {limit} limit is full"),
                    None => format!("{account} is full"),
                }),
                _ => None,
            }
        })
        .collect();
    let why = match reasons.as_slice() {
        [] => "No other account can take it.".to_string(),
        [only] => format!("{only}."),
        [rest @ .., last] => format!("{} and {last}.", rest.join(", ")),
    };
    let action = wait
        .get("alternative")
        .and_then(Value::as_object)
        .and_then(|alternative| {
            let field = |key: &str| {
                alternative
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            };
            let label = field("modelLabel")?;
            let account = field("account")?;
            Some(ContinueAction {
                label: format!("Continue with {label} on {}", name(account)),
                answer: json!({
                    "kind": "continueWithModel",
                    "accountId": field("accountId")?,
                    "model": field("model")?,
                    "effort": field("effort").unwrap_or_default(),
                    "modelLabel": label,
                    "fromModel": model,
                }),
            })
        });
    WaitWords {
        title: format!("No account has room for {model}"),
        detail: format!("{why} {when}"),
        action,
    }
}

/// `at 12:00 PM`, `tomorrow at 12:00 PM` or `on Oct 18 at 2:00 PM`, in the user's own timezone.
fn local_time(at_ms: f64, now_ms: f64, utc_offset_minutes: i32) -> String {
    let offset = f64::from(utc_offset_minutes) * 60_000.0;
    let local = at_ms + offset;
    let day = (local / 86_400_000.0).floor() as i64;
    let today = ((now_ms + offset) / 86_400_000.0).floor() as i64;
    let minute_of_day = ((local - day as f64 * 86_400_000.0) / 60_000.0).floor() as i64;
    let (hour, minute) = (minute_of_day / 60, minute_of_day % 60);
    let clock = format!(
        "{}:{minute:02} {}",
        if hour % 12 == 0 { 12 } else { hour % 12 },
        if hour < 12 { "AM" } else { "PM" }
    );
    if day == today {
        format!("at {clock}")
    } else if day == today + 1 {
        format!("tomorrow at {clock}")
    } else {
        let (_, month, date) = civil_from_days(day);
        let month = MONTHS
            .get(usize::try_from(month - 1).unwrap_or_default())
            .copied()
            .unwrap_or_default();
        format!("on {month} {date} at {clock}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(wait: Value) -> Map<String, Value> {
        json!({
            "kind": "usageLimit",
            "severity": "warning",
            "title": "Usage limit reached",
            "accountWait": wait,
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    // 2026-10-11T11:52:21Z, in Asia/Dubai (UTC+4).
    const NOW: f64 = 1_791_719_541_000.0;
    const DUBAI: i32 = 240;

    #[test]
    fn names_the_limit_the_account_and_the_local_reset() {
        let mut object = notice(json!({
            "limit": "7d limit",
            "resetsAt": "2026-10-12T08:00:00+00:00",
            "account": "sharptabs.com@gmail.com",
        }));
        decorate(&mut object, false, NOW, DUBAI);
        assert_eq!(object["title"], "7d limit reached on every account");
        assert_eq!(object["tone"], "warning");
        assert_eq!(
            object["detail"],
            "Every account this session can switch to is full, so it waits instead of switching. It continues automatically tomorrow at 12:00 PM, when sharptabs.com@gmail.com\u{2019}s limit resets."
        );
    }

    #[test]
    fn falls_back_without_a_known_reset_and_masks_emails() {
        let mut object = notice(json!({ "limit": null, "resetsAt": null, "account": null }));
        decorate(&mut object, true, NOW, DUBAI);
        assert_eq!(object["title"], "Usage limit reached on every account");
        assert!(object["detail"]
            .as_str()
            .unwrap()
            .ends_with("as soon as an account has room."));

        let mut object = notice(json!({
            "limit": "5h limit",
            "resetsAt": "2026-10-11T12:30:00Z",
            "account": "sharptabs.com@gmail.com",
        }));
        decorate(&mut object, true, NOW, DUBAI);
        let detail = object["detail"].as_str().unwrap();
        assert!(detail.contains("at 4:30 PM"));
        assert!(!detail.contains("sharptabs.com@gmail.com"));
    }

    #[test]
    fn leaves_other_notices_alone() {
        let mut object = notice(json!({ "limit": "7d limit" }));
        object.insert("kind".to_string(), json!("loginExpired"));
        decorate(&mut object, false, NOW, DUBAI);
        assert_eq!(object["title"], "Usage limit reached");
        let mut object = notice(Value::Null);
        decorate(&mut object, false, NOW, DUBAI);
        assert_eq!(object["title"], "Usage limit reached");
        assert!(object.get("tone").is_none());
    }
}
