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

/// Rewrites a usage-limit notice that carries `accountWait` or `quotaWait` into its calm wait card.
pub fn decorate(
    object: &mut Map<String, Value>,
    hide_account_emails: bool,
    now_ms: f64,
    utc_offset_minutes: i32,
) {
    if object.get("kind").and_then(Value::as_str) == Some(USAGE_WARNING_KIND) {
        decorate_usage_warning(object, hide_account_emails);
        return;
    }
    if object.get("kind").and_then(Value::as_str) != Some("usageLimit") {
        return;
    }
    if let Some(quota_wait) = object.get("quotaWait").and_then(Value::as_object).cloned() {
        if object
            .get("accountWait")
            .and_then(Value::as_object)
            .is_none()
        {
            decorate_quota_wait(object, &quota_wait, now_ms, utc_offset_minutes);
            return;
        }
    }
    let Some(wait) = object
        .get("accountWait")
        .and_then(Value::as_object)
        .cloned()
    else {
        return;
    };
    let words = wait_words(&wait, hide_account_emails, now_ms, utc_offset_minutes);
    // The no-room card's one dropdown (`actionMenu`), built before `words` is taken apart below.
    let no_room = wait
        .get("provider")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        && wait.get("mode").and_then(Value::as_str).is_none();
    if no_room {
        let acknowledged = wait
            .get("acknowledgedAt")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        let reset = wait
            .get("resetsAt")
            .and_then(Value::as_str)
            .and_then(|at| parse_iso_millis(at, utc_offset_minutes))
            .filter(|at| *at > now_ms)
            .map(|at| local_clock_short(at, now_ms, utc_offset_minutes));
        object.insert(
            "actionMenu".to_string(),
            wait_action_menu(&words, acknowledged, reset.as_deref()),
        );
    }
    object.insert("title".to_string(), json!(words.title));
    object.insert("detail".to_string(), json!(words.detail));
    let has_provider = wait
        .get("provider")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let mode = wait.get("mode").and_then(Value::as_str);
    let acknowledged = wait
        .get("acknowledgedAt")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let tone = if has_provider && mode.is_none() {
        if acknowledged {
            "wait"
        } else {
            "error"
        }
    } else {
        "warning"
    };
    object.insert("tone".to_string(), json!(tone));
    // The card's main button (Q2977/Q2978 in model_fallback.rs): ahead of the notice's own
    // actions, and `primary` so the renderers draw it filled and Switch account beside it.
    // A limit that already reset has the agent's own Continue on the card (it presses Enter), so a
    // paused wait adds no second one.
    let own_continue = object
        .get("actions")
        .and_then(Value::as_array)
        .is_some_and(|actions| actions.iter().any(|action| action["id"] == "continueNow"));
    let paused = mode == Some("afterRestart");
    let has_primary_action = words
        .action
        .as_ref()
        .filter(|_| !(paused && own_continue))
        .is_some();
    if let Some(action) = words.action.filter(|_| !(paused && own_continue)) {
        let mut actions = object
            .get("actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        actions.insert(
            0,
            json!({
                "id": action.answer["kind"].clone(),
                "label": action.label,
                "kind": action.answer["kind"].clone(),
                "primary": true,
                "answer": action.answer,
            }),
        );
        object.insert("actions".to_string(), Value::Array(actions));
    }
    // Q4472: while an exhausted account pool is unacknowledged (red blocked dot showing), offer
    // Wait so the user can clear the red dot into the calm amber waiting clock while keeping
    // automatic resume on reset active.
    if has_provider && mode.is_none() && !acknowledged {
        let mut actions = object
            .get("actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let insert_at = if has_primary_action { 1 } else { 0 };
        actions.insert(
            insert_at,
            json!({
                "id": "acknowledgeLimitWait",
                "label": "Wait",
                "kind": "acknowledgeLimitWait",
                "primary": !has_primary_action,
                "answer": { "kind": "acknowledgeLimitWait" },
            }),
        );
        object.insert("actions".to_string(), Value::Array(actions));
    }
    // The fixes for the accounts the sentence names (Sign in, Turn on, Use it for this session),
    // after the card's main button and Wait and ahead of the notice's own actions; two at most,
    // so the card's buttons stay short.
    if !words.fixes.is_empty() {
        let mut actions = object
            .get("actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let lead = usize::from(has_primary_action)
            + usize::from(has_provider && mode.is_none() && !acknowledged);
        for (index, fix) in words.fixes.iter().take(2).enumerate() {
            actions.insert(
                (lead + index).min(actions.len()),
                json!({
                    "id": format!("{}:{}", fix.kind, fix.account_id),
                    "label": fix.label,
                    "kind": fix.kind,
                    "accountId": fix.account_id,
                    "answer": fix.answer(),
                }),
            );
        }
        object.insert("actions".to_string(), Value::Array(actions));
    }
    // The Compact first checkbox on a wait a restart paused (CDXC:AgentProviders 2026-11-14 in
    // server/src/carry_on_compaction.rs, Q3238): ticked for a chat at or above the compaction size,
    // with the (i) tooltip saying why.
    if paused {
        if let Some(compact) = wait.get("compact").and_then(Value::as_object) {
            let checked = compact.get("checked").and_then(Value::as_bool) == Some(true);
            let label = match compact.get("tokens").and_then(Value::as_u64) {
                Some(tokens) => format!("Compact first ({} of context)", short_tokens(tokens)),
                None => "Compact first".to_string(),
            };
            object.insert(
                "checkbox".to_string(),
                json!({
                    "id": "compactFirst",
                    "label": label,
                    "checked": checked,
                    "hint": compact.get("hint").cloned().unwrap_or(Value::Null),
                    "answer": { "kind": "compactFirst", "checked": !checked },
                }),
            );
        }
    }
    // Every wait can be ended from its card: Stop while Ghostex waits to continue, Not now on a
    // wait a restart paused (CDXC:AgentProviders 2026-11-14 in server/src/accounts/recovery.rs).
    let mut actions = object
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    actions.push(json!({
        "id": "stopRecovery",
        "label": if paused { "Not now" } else { "Stop" },
        "kind": "stopRecovery",
        "answer": { "kind": "stopRecovery" },
    }));
    object.insert("actions".to_string(), Value::Array(actions));
}

/// CDXC:AgentProviders 2026-11-22 DECISION (Q4460, Q4461, Q4462):
/// Decorates an Antigravity (or other non-account-switched) `usageLimit` notice carrying
/// `quotaWait` with plain wording, reset countdown, Wait for reset, Switch model, Terminal view,
/// and Stop / Dismiss.
/// The notice kind of the early warning before an account runs out (server/src/accounts/usage_warning.rs).
pub const USAGE_WARNING_KIND: &str = "usageWarning";

/// The words on the one dropdown an account card folds its choices into.
pub const ACTION_MENU_LABEL: &str = "Choose what to do";

/// CDXC:AgentProviders 2026-11-27 DECISION:
/// User, answering the account-switch page (Q5296 A, the full card that says why, and the note on
/// every choice): "I want the options there shown as one dropdown under choices, not as individual
/// buttons ... so we can fit this even if the screen is narrower." The no-room limit card and the
/// early-warning card keep their sentence on why, then carry `actionMenu`: `{ label, items }`, one
/// control that opens every choice. Each item has `id`, `label`, `kind` and, for the ones the chat
/// answers, `answer`. The renderers handle four kinds themselves: `accountSignIn` (Settings >
/// Accounts on `accountId`'s sign-in), `openAccountsSettings` (Settings > Accounts), `pickAccount`
/// (the chat's account panel) and `dismiss` (the card's own Dismiss); every other item sends its
/// `answer` as the card's buttons did. A renderer that draws the menu draws no other buttons on
/// the card, so the card fits a narrow chat column and the phone.
/// SEE-ALSO: apps/desktop/src/app/native_chat/notice.rs (`notice_action_menu`),
/// apps/mobile/app/src/chat/native/cards/NoticeCard.tsx.
fn menu_item(id: &str, label: &str, kind: &str, answer: Option<Value>) -> Value {
    let mut item = json!({ "id": id, "label": label, "kind": kind });
    if let Some(answer) = answer {
        item["answer"] = answer;
    }
    item
}

fn fix_item(fix: &AccountFix) -> Value {
    let mut item = menu_item(
        &format!("{}:{}", fix.kind, fix.account_id),
        &fix.label,
        &fix.kind,
        Some(fix.answer()),
    );
    item["accountId"] = json!(fix.account_id);
    item
}

/// The no-room limit card's dropdown: a model with room first, the sign-ins and turn-ons, the
/// account panel, an account the user may pick by hand, then waiting and stopping.
fn wait_action_menu(words: &WaitWords, acknowledged: bool, reset: Option<&str>) -> Value {
    let mut items = Vec::new();
    if let Some(action) = words.action.as_ref() {
        let kind = action
            .answer
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("continueWithModel");
        items.push(menu_item(
            kind,
            &action.label,
            kind,
            Some(action.answer.clone()),
        ));
    }
    let (fixes_first, fixes_last): (Vec<&AccountFix>, Vec<&AccountFix>) =
        words.fixes.iter().partition(|fix| fix.kind != "accountUse");
    items.extend(fixes_first.into_iter().map(fix_item));
    items.push(menu_item(
        "pickAccount",
        "Pick an account",
        "pickAccount",
        None,
    ));
    items.extend(fixes_last.into_iter().map(fix_item));
    if !acknowledged {
        let label = match reset {
            Some(at) => format!("Wait until {at}"),
            None => "Wait for an account with room".to_string(),
        };
        items.push(menu_item(
            "acknowledgeLimitWait",
            &label,
            "acknowledgeLimitWait",
            Some(json!({ "kind": "acknowledgeLimitWait" })),
        ));
    }
    items.push(menu_item(
        "stopRecovery",
        "Stop, I'll continue it myself",
        "stopRecovery",
        Some(json!({ "kind": "stopRecovery" })),
    ));
    if acknowledged {
        items.push(menu_item("dismiss", "Not now", "dismiss", None));
    }
    json!({ "label": ACTION_MENU_LABEL, "items": items })
}

/// The early warning before an account runs out (`usageWarning`, server/src/accounts/usage_warning.rs):
/// gxserver writes the title and the sentence; the chat adds the dropdown from its facts
/// (`blocked`, the same reasons the no-room card names, and `coordinator`).
///
/// CDXC:AgentProviders 2026-11-27 DECISION:
/// User: "i want a warning when you're about to run out of usage on the coordinator and the account
/// for it isn't logged in". From the account-switch page: the card in the coordinator's chat is one
/// of four places it shows (Q5294), and an account with room that is manual only is offered as
/// "Use #2 for this coordinator", which never happens on its own (Q5298).
fn decorate_usage_warning(object: &mut Map<String, Value>, hide_account_emails: bool) {
    let Some(facts) = object
        .get("usageWarning")
        .and_then(Value::as_object)
        .cloned()
    else {
        return;
    };
    let name = |value: &str| {
        if hide_account_emails {
            mask_account_text(value)
        } else {
            value.to_string()
        }
    };
    let (_, fixes) = blocked_reasons(&facts, &name);
    let coordinator = facts.get("coordinator").and_then(Value::as_bool) == Some(true);
    let mut items: Vec<Value> = fixes
        .iter()
        .filter(|fix| fix.kind != "accountUse")
        .map(fix_item)
        .collect();
    items.push(menu_item(
        "openAccountsSettings",
        "Open Accounts settings",
        "openAccountsSettings",
        None,
    ));
    for fix in fixes.iter().filter(|fix| fix.kind == "accountUse") {
        let mut item = fix_item(fix);
        if coordinator {
            let account = facts
                .get("blocked")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|entry| {
                    entry.get("accountId").and_then(Value::as_str) == Some(fix.account_id.as_str())
                })
                .and_then(|entry| entry.get("account").and_then(Value::as_str))
                .map(|account| name(account.trim()))
                .unwrap_or_default();
            if !account.is_empty() {
                item["label"] = json!(format!("Use {account} for this coordinator"));
            }
        }
        items.push(item);
    }
    items.push(menu_item(
        "dismissUsageWarning",
        "Not now",
        "restartAgent",
        Some(json!({ "kind": "restartAgent", "actionId": "dismissUsageWarning" })),
    ));
    object.insert("tone".to_string(), json!("warning"));
    object.insert(
        "actionMenu".to_string(),
        json!({ "label": ACTION_MENU_LABEL, "items": items }),
    );
}

fn decorate_quota_wait(
    object: &mut Map<String, Value>,
    wait: &Map<String, Value>,
    now_ms: f64,
    utc_offset_minutes: i32,
) {
    let text = |key: &str| {
        wait.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let mode = text("mode").unwrap_or("quotaBlocked");
    let resets_at = text("resetsAt")
        .and_then(|at| parse_iso_millis(at, utc_offset_minutes))
        .filter(|at| *at > now_ms);
    let countdown = resets_at.map(|at| short_countdown_ms(at - now_ms));
    let when = resets_at.map(|at| local_time(at, now_ms, utc_offset_minutes));
    let (title, detail) = match mode {
        "quotaWaiting" => {
            let title = match countdown.as_deref() {
                Some(cd) => format!("Waiting for reset · {cd}"),
                None => "Waiting for quota reset".to_string(),
            };
            let detail = match when.as_deref() {
                Some(when) => format!(
                    "Ghostex will continue automatically {when} after the limit resets. Switch models now to keep going sooner."
                ),
                None => "Ghostex will continue automatically after the limit resets. Switch models now to keep going sooner.".to_string(),
            };
            (title, detail)
        }
        _ => {
            let title = match countdown.as_deref() {
                Some(cd) => format!("Quota reached · resets in {cd}"),
                None => "Quota reached".to_string(),
            };
            let detail = match when.as_deref() {
                Some(when) => format!(
                    "Resets {when}. Choose Wait for reset to continue automatically, or switch models now."
                ),
                None => "This model reached its quota. Choose Wait for reset to continue automatically, or switch models now.".to_string(),
            };
            (title, detail)
        }
    };
    object.insert("title".to_string(), json!(title));
    object.insert("detail".to_string(), json!(detail));
    object.insert("tone".to_string(), json!("wait"));
    // Rebuild actions cleanly in the exact order:
    // - quotaBlocked: Wait for reset (primary), Switch model, Terminal view (+ Dismiss in renderer)
    // - quotaWaiting: Switch model (primary), Terminal view, Cancel wait (+ Dismiss in renderer)
    let mut actions = Vec::new();
    if mode == "quotaWaiting" {
        actions.push(json!({
            "id": "switchModel",
            "label": "Switch model",
            "kind": "openModelPicker",
            "primary": true,
            "answer": Value::Null,
        }));
        actions.push(json!({
            "id": "switchToTerminal",
            "label": "Terminal view",
            "kind": "switchToTerminal",
            "answer": Value::Null,
        }));
        actions.push(json!({
            "id": "dismissUsageLimit",
            "label": "Cancel wait",
            "kind": "restartAgent",
            "answer": { "kind": "restartAgent", "actionId": "dismissUsageLimit" },
        }));
    } else {
        actions.push(json!({
            "id": "waitForQuotaReset",
            "label": "Wait for reset",
            "kind": "restartAgent",
            "primary": true,
            "answer": { "kind": "restartAgent", "actionId": "waitForQuotaReset" },
        }));
        actions.push(json!({
            "id": "switchModel",
            "label": "Switch model",
            "kind": "openModelPicker",
            "answer": Value::Null,
        }));
        actions.push(json!({
            "id": "switchToTerminal",
            "label": "Terminal view",
            "kind": "switchToTerminal",
            "answer": Value::Null,
        }));
    }
    object.insert("actions".to_string(), Value::Array(actions));
}

fn short_countdown_ms(ms: f64) -> String {
    let secs = (ms / 1000.0).ceil().max(0.0) as i64;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", (secs + 59) / 60)
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {mins}m")
        }
    }
}

/// What the card and the Accounts & limits panel's waiting box say while every account is full.
#[derive(Clone, Debug, PartialEq)]
pub struct WaitWords {
    pub title: String,
    pub detail: String,
    pub action: Option<ContinueAction>,
    /// One button per account the sentence names that a click can make usable: Sign in, Turn on,
    /// Use it for this session.
    pub fixes: Vec<AccountFix>,
}

/// "Turn on x•••0@•••••.•••": an account that could take the session once fixed.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountFix {
    pub label: String,
    /// `accountSignIn` (the app opens Settings > Accounts on its sign-in), `accountTurnOn` or
    /// `accountUse` (answered by gxserver, server/src/session_chat_send/answer_http.rs).
    pub kind: String,
    pub account_id: String,
}

impl AccountFix {
    /// The card's answer: gxserver answers `accountTurnOn` and `accountUse`; the app turns
    /// `accountSignIn` into Settings > Accounts.
    pub fn answer(&self) -> Value {
        json!({ "kind": self.kind, "accountId": self.account_id })
    }

    /// The chat command the Accounts & limits panel's waiting box sends for this fix.
    pub fn command(&self) -> Value {
        match self.kind.as_str() {
            // The desktop reads `reconnectAccountId`, the phone `accountId`.
            "accountSignIn" => json!({
                "type": "openAccountsSettings",
                "reconnectAccountId": self.account_id,
                "accountId": self.account_id,
            }),
            "accountTurnOn" => json!({
                "type": "accounts",
                "request": { "operation": "setEnabled", "id": self.account_id, "enabled": true },
            }),
            _ => json!({
                "type": "accounts",
                "request": { "operation": "select", "accountId": self.account_id },
            }),
        }
    }
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
    // CDXC:AgentProviders 2026-11-14 DECISION in server/src/accounts/recovery.rs (GitHub #608,
    // Q3228 and Q3229): a wait for the reset says when Ghostex continues the session, and a wait a
    // restart paused asks before anything is sent.
    match text("mode") {
        Some("afterRestart") => {
            return WaitWords {
                title: "Continue after the limit reset?".to_string(),
                detail: "This session was waiting for its usage limit when Ghostex closed, so Ghostex won\u{2019}t continue it on its own.".to_string(),
                action: Some(ContinueAction {
                    label: "Continue".to_string(),
                    answer: json!({ "kind": "resumeRecovery" }),
                }),
                fixes: Vec::new(),
            };
        }
        Some("reset") => {
            let at = text("resetsAt")
                .and_then(|at| parse_iso_millis(at, utc_offset_minutes))
                .filter(|at| *at > now_ms);
            return WaitWords {
                title: "Continuing after the limit reset".to_string(),
                detail: match at {
                    Some(at) => format!(
                        "Ghostex will continue this session {}. Stop if you\u{2019}d rather continue it yourself.",
                        local_time(at, now_ms, utc_offset_minutes)
                    ),
                    None => "Ghostex will continue this session when the limit resets. Stop if you\u{2019}d rather continue it yourself.".to_string(),
                },
                action: None,
                fixes: Vec::new(),
            };
        }
        _ => {}
    }
    let provider = text("provider");
    let acknowledged = text("acknowledgedAt").is_some();
    let model = text("model");
    let limit = text("limit").unwrap_or("Usage limit");
    let reset = text("resetsAt")
        .and_then(|at| parse_iso_millis(at, utc_offset_minutes))
        .filter(|at| *at > now_ms);
    if let Some(provider) = provider {
        let provider_label = match provider {
            "claude" => "Claude",
            "codex" => "Codex",
            other => other,
        };
        let action = alternative_action(wait, model.unwrap_or_default(), &name);
        if acknowledged {
            let title = match reset {
                Some(at) => format!(
                    "Waiting until {}",
                    local_clock_short(at, now_ms, utc_offset_minutes)
                ),
                None => "Waiting for limit reset".to_string(),
            };
            let detail = match (reset, text("account")) {
                (Some(at), Some(account)) => format!(
                    "Work continues automatically {}, when {}\u{2019}s limit resets. Switch accounts now to keep going sooner.",
                    local_time(at, now_ms, utc_offset_minutes),
                    name(account)
                ),
                (Some(at), None) => format!(
                    "Work continues automatically {}. Switch accounts now to keep going sooner.",
                    local_time(at, now_ms, utc_offset_minutes)
                ),
                (None, _) => "Work continues automatically as soon as an account has room. Switch accounts now to keep going sooner.".to_string(),
            };
            return WaitWords {
                title,
                detail,
                action,
                fixes: Vec::new(),
            };
        }
        let when = match (reset, text("account"), model) {
            (Some(at), Some(account), Some(_)) => format!(
                "Work continues automatically {}, when {}\u{2019}s {}resets.",
                local_time(at, now_ms, utc_offset_minutes),
                name(account),
                text("accountLimit")
                    .map_or_else(|| "limit ".to_string(), |limit| format!("{limit} limit "))
            ),
            (Some(at), Some(account), None) => format!(
                "Work continues automatically {}, when {}\u{2019}s limit resets.",
                local_time(at, now_ms, utc_offset_minutes),
                name(account)
            ),
            (Some(at), None, _) => format!(
                "Work continues automatically {}.",
                local_time(at, now_ms, utc_offset_minutes)
            ),
            (None, _, _) => {
                "Work continues automatically as soon as an account has room.".to_string()
            }
        };
        let (reasons, fixes) = blocked_reasons(wait, &name);
        let why = match reasons.as_slice() {
            [] => "Every automatic account is full.".to_string(),
            [only] => format!("{only}."),
            [rest @ .., last] => format!("{} and {last}.", rest.join(", ")),
        };
        let title = match model {
            Some(model) => format!("No account has room for {model}"),
            None => format!("{provider_label} accounts at limit"),
        };
        return WaitWords {
            title,
            detail: format!("{why} {when}"),
            action,
            fixes,
        };
    }
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
            fixes: Vec::new(),
        };
    };
    let (reasons, fixes) = blocked_reasons(wait, &name);
    let why = match reasons.as_slice() {
        [] => "No other account can take it.".to_string(),
        [only] => format!("{only}."),
        [rest @ .., last] => format!("{} and {last}.", rest.join(", ")),
    };
    let action = alternative_action(wait, model, &name);
    WaitWords {
        title: format!("No account has room for {model}"),
        detail: format!("{why} {when}"),
        action,
        fixes,
    }
}

/// Why each other account can't take the session, in the card's one sentence, and the fixes for
/// the ones a click can make usable.
///
/// CDXC:AgentProviders 2026-11-22 DECISION in server/src/accounts/limit_wait.rs (`describe`): an
/// account turned off here, one never signed in on this computer and one with room that is manual
/// only are named too, each with its one-click fix.
fn blocked_reasons(
    wait: &Map<String, Value>,
    name: &impl Fn(&str) -> String,
) -> (Vec<String>, Vec<AccountFix>) {
    let mut fixes = Vec::new();
    // (account, what is said of one account, of several): accounts with the same reason are
    // named together ("#4 and #5 need to sign in again"), in the order they first appear.
    let mut said: Vec<(Vec<String>, String, String)> = Vec::new();
    for entry in wait
        .get("blocked")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(account) = entry
            .get("account")
            .and_then(Value::as_str)
            .map(|account| name(account.trim()))
        else {
            continue;
        };
        let id = entry
            .get("accountId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let mut fix = |kind: &str, label: String| {
            if let Some(id) = id {
                fixes.push(AccountFix {
                    label,
                    kind: kind.to_string(),
                    account_id: id.to_string(),
                });
            }
        };
        let (one, many) = match entry.get("why").and_then(Value::as_str).unwrap_or_default() {
            "manual" if entry.get("room").and_then(Value::as_bool) == Some(true) => {
                fix("accountUse", format!("Use {account} for this session"));
                (
                    "has room but is manual only".to_string(),
                    "have room but are manual only".to_string(),
                )
            }
            "manual" => ("is manual only".to_string(), "are manual only".to_string()),
            "off" => {
                fix("accountTurnOn", format!("Turn on {account}"));
                (
                    "is turned off here".to_string(),
                    "are turned off here".to_string(),
                )
            }
            "signIn" if entry.get("here").and_then(Value::as_bool) == Some(false) => {
                fix("accountSignIn", format!("Sign in {account}"));
                (
                    "isn\u{2019}t signed in on this computer".to_string(),
                    "aren\u{2019}t signed in on this computer".to_string(),
                )
            }
            "signIn" => {
                fix("accountSignIn", format!("Sign in {account}"));
                (
                    "needs to sign in again".to_string(),
                    "need to sign in again".to_string(),
                )
            }
            "full" => match entry.get("limit").and_then(Value::as_str) {
                Some(limit) => (
                    format!("\u{2019}s {limit} limit is full"),
                    format!("\u{2019}s {limit} limits are full"),
                ),
                None => ("is full".to_string(), "are full".to_string()),
            },
            _ => continue,
        };
        match said.iter_mut().find(|(_, said_one, _)| *said_one == one) {
            Some((accounts, _, _)) => accounts.push(account),
            None => said.push((vec![account], one, many)),
        }
    }
    let reasons = said
        .into_iter()
        .map(|(accounts, one, many)| {
            let tail = if accounts.len() == 1 { one } else { many };
            let names = match accounts.split_last() {
                Some((last, [])) => last.clone(),
                Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
                None => String::new(),
            };
            // "#1's 7d limit is full" has no space before its apostrophe.
            if tail.starts_with('\u{2019}') {
                format!("{names}{tail}")
            } else {
                format!("{names} {tail}")
            }
        })
        .collect();
    (reasons, fixes)
}

fn alternative_action(
    wait: &Map<String, Value>,
    model: &str,
    name: &impl Fn(&str) -> String,
) -> Option<ContinueAction> {
    wait.get("alternative")
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
        })
}

/// `12:00 PM`, `tomorrow 12:00 PM` or `Oct 18 2:00 PM`, for compact card titles.
fn local_clock_short(at_ms: f64, now_ms: f64, utc_offset_minutes: i32) -> String {
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
        clock
    } else if day == today + 1 {
        format!("tomorrow {clock}")
    } else {
        let (_, month, date) = civil_from_days(day);
        let month = MONTHS
            .get(usize::try_from(month - 1).unwrap_or_default())
            .copied()
            .unwrap_or_default();
        format!("{month} {date} {clock}")
    }
}

/// "312K" for 312,400 tokens (server/src/carry_on_compaction.rs `short_tokens`).
fn short_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else {
        format!("{}K", (tokens + 500) / 1_000)
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
