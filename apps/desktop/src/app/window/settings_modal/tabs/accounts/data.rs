//! The account data the Accounts page reads, from gxserver's `AgentAccountsState`
//! (packages/shared/agent-accounts.ts), with the presentation helpers of accounts/presentation.ts (deleted 2026-10-01),
//! shared/account-usage-windows.ts (deleted 2026-10-01), shared/account-display.ts (deleted 2026-10-01) (Hide emails) and
//! shared/reset-countdown.ts (deleted 2026-10-01).
use super::super::super::catalog::{module, settings_catalog};
use serde_json::Value;

/// The providers whose accounts run sessions: they have rules and a helper tool.
pub(crate) const PROVIDERS: [&str; 2] = ["claude", "codex"];

/// The providers Ghostex only shows the usage of (server/src/accounts/usage_only/).
pub(crate) const USAGE_PROVIDERS: [&str; 3] = ["cursor", "gemini", "copilot"];

/// `providerLabel`.
pub(crate) fn provider_label(provider: &str) -> &'static str {
    match provider {
        "claude" => "Claude",
        "cursor" => "Cursor",
        "gemini" => "Gemini",
        "copilot" => "Copilot",
        _ => "Codex",
    }
}

/// `helperLabel`.
pub(crate) fn helper_label(provider: &str) -> &'static str {
    if provider == "claude" {
        "Claude Swap"
    } else {
        "Codex Swap"
    }
}

/// `HELPER_NAMES[provider].command`.
pub(crate) fn helper_command(provider: &str) -> &'static str {
    if provider == "claude" {
        "cswap"
    } else {
        "xswap"
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `AccountUsageWindow`.
#[derive(Clone, Debug)]
pub(crate) struct UsageWindow {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) used_percent: f64,
    pub(crate) limit_window_seconds: Option<f64>,
    pub(crate) model: Option<String>,
    pub(crate) resets_at: Option<String>,
}

impl UsageWindow {
    fn from_value(value: &Value) -> Self {
        Self {
            id: text(value, "id"),
            label: text(value, "label"),
            used_percent: value["usedPercent"].as_f64().unwrap_or(0.0),
            limit_window_seconds: value["limitWindowSeconds"].as_f64(),
            model: value
                .get("model")
                .and_then(Value::as_str)
                .filter(|model| !model.is_empty())
                .map(str::to_string),
            resets_at: value
                .get("resetsAt")
                .and_then(Value::as_str)
                .filter(|reset| !reset.is_empty())
                .map(str::to_string),
        }
    }

    /// "resets in 2h 14m" within a day, otherwise the local weekday and time ("resets Thu 9:00").
    pub(crate) fn reset_text(&self) -> Option<String> {
        let reset = chrono::DateTime::parse_from_rfc3339(self.resets_at.as_deref()?).ok()?;
        let seconds = reset.timestamp() - chrono::Utc::now().timestamp();
        if seconds <= 0 {
            return Some("resets now".into());
        }
        let minutes = seconds / 60;
        Some(if minutes < 60 {
            format!("resets in {}m", minutes.max(1))
        } else if minutes < 24 * 60 {
            format!("resets in {}h {}m", minutes / 60, minutes % 60)
        } else {
            let local = reset.with_timezone(&chrono::Local);
            format!("resets {}", local.format("%a %-H:%M"))
        })
    }

    /// The window's short name: `5h`, `7d`, `Fable`, `<Model> 7d`, or `<n>h` / `<n>d` from its
    /// length, whatever label an older gxserver sent (gxserver's `CDXC:AgentProviders 2026-10-14`
    /// on `duration_label`; Fable stays "Fable" per gx-core's 2026-09-12 decision on
    /// `account_usage_label`).
    pub(crate) fn short_label(&self) -> String {
        if self
            .model
            .as_deref()
            .is_some_and(|model| model.eq_ignore_ascii_case("fable"))
        {
            return "Fable".into();
        }
        let duration = match self.limit_window_seconds {
            Some(seconds) if seconds >= 604_800.0 => Some("7d".to_string()),
            Some(seconds) if seconds > 0.0 && seconds % 86_400.0 == 0.0 => {
                Some(format!("{}d", (seconds / 86_400.0) as i64))
            }
            Some(seconds) if seconds > 0.0 && seconds % 3_600.0 == 0.0 => {
                Some(format!("{}h", (seconds / 3_600.0) as i64))
            }
            _ if self.is_five_hour() => Some("5h".into()),
            _ if self.is_weekly() || self.model.is_some() => Some("7d".into()),
            _ => None,
        };
        match (self.model.as_deref(), duration) {
            (Some(model), Some(duration)) => format!("{model} {duration}"),
            (None, Some(duration)) => duration,
            _ => self.label.clone(),
        }
    }
    fn is_weekly(&self) -> bool {
        self.id == "sevenDay" || self.limit_window_seconds.unwrap_or(0.0) >= 604_800.0
    }

    fn is_five_hour(&self) -> bool {
        self.id == "fiveHour" || self.limit_window_seconds == Some(18_000.0)
    }
}

/// `AgentAccount`.
#[derive(Clone, Debug)]
pub(crate) struct Account {
    pub(crate) raw: Value,
}

impl Account {
    pub(crate) fn id(&self) -> String {
        text(&self.raw, "id")
    }

    pub(crate) fn provider(&self) -> String {
        text(&self.raw, "provider")
    }

    pub(crate) fn selector(&self) -> String {
        text(&self.raw, "selector")
    }

    pub(crate) fn name(&self) -> String {
        text(&self.raw, "name")
    }

    pub(crate) fn email(&self) -> String {
        text(&self.raw, "email")
    }

    pub(crate) fn color(&self) -> String {
        let color = text(&self.raw, "color");
        if color.is_empty() {
            "neutral".into()
        } else {
            color
        }
    }

    pub(crate) fn indicator(&self) -> String {
        text(&self.raw, "indicator")
    }

    pub(crate) fn eligible(&self) -> bool {
        self.raw["eligible"].as_bool() == Some(true)
    }

    /// Why gxserver took this login out of automatic switching (server/src/accounts/disabled.rs).
    pub(crate) fn disabled_reason(&self) -> Option<String> {
        self.raw
            .get("disabledReason")
            .and_then(Value::as_str)
            .filter(|reason| !reason.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn registered(&self) -> bool {
        self.raw["registered"].as_bool() == Some(true)
    }

    pub(crate) fn status(&self) -> String {
        text(&self.raw, "status")
    }
    /// The saved login stopped working (or is missing): the account needs Reconnect. An account
    /// the user turned off never does.
    pub(crate) fn needs_sign_in(&self) -> bool {
        self.status() != "ready" && !self.user_disabled()
    }
    /// The user turned the account off (the Enabled switch; gxserver's `CDXC:AgentProviders
    /// 2026-10-14` on `SavedAccount::user_disabled`).
    pub(crate) fn user_disabled(&self) -> bool {
        self.raw["userDisabled"].as_bool() == Some(true)
    }
    /// The figure the account list shows: the most used of its headline windows.
    pub(crate) fn tightest_percent(&self) -> Option<f64> {
        if self.user_disabled() {
            return None;
        }
        self.headline_windows()
            .iter()
            .map(|window| window.used_percent)
            .fold(None, |most, used| {
                Some(most.map_or(used, |most: f64| most.max(used)))
            })
    }
    /// The windows the detail pane draws as bars: five-hour first, then the others.
    pub(crate) fn usage_bars(&self) -> Vec<UsageWindow> {
        let mut windows = self.headline_windows();
        windows.sort_by_key(|window| !window.is_five_hour());
        windows
    }

    /// A Cursor, Gemini or Copilot account, which only shows usage.
    pub(crate) fn usage_only(&self) -> bool {
        self.raw["usageOnly"].as_bool() == Some(true)
    }

    /// The plan the provider reported ("Pro", "Google AI Ultra").
    pub(crate) fn plan(&self) -> Option<String> {
        self.raw
            .get("plan")
            .and_then(Value::as_str)
            .filter(|plan| !plan.is_empty())
            .map(str::to_string)
    }

    /// The labelled values a usage-only account shows under its bars.
    pub(crate) fn details(&self) -> Vec<(String, String)> {
        self.raw["details"]
            .as_array()
            .map(|details| {
                details
                    .iter()
                    .map(|detail| (text(detail, "label"), text(detail, "value")))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn show_in_titlebar(&self) -> bool {
        self.raw["showInTitlebar"].as_bool() == Some(true)
    }

    pub(crate) fn usage_error(&self) -> Option<String> {
        self.raw
            .get("usageError")
            .and_then(Value::as_str)
            .filter(|error| !error.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn session_count(&self) -> u64 {
        self.raw["sessionCount"].as_u64().unwrap_or(0)
    }

    pub(crate) fn reset_credits(&self) -> Option<i64> {
        self.raw["resetCredits"].as_i64()
    }

    pub(crate) fn usage(&self) -> Vec<UsageWindow> {
        self.raw["usage"]
            .as_array()
            .map(|windows| windows.iter().map(UsageWindow::from_value).collect())
            .unwrap_or_default()
    }

    /// `selector` as a number for sorting (`Number(a.selector)`).
    pub(crate) fn selector_number(&self) -> f64 {
        self.selector().trim().parse::<f64>().unwrap_or(f64::NAN)
    }

    /// `accountHeadlineWindows`: Claude shows the two tightest of weekly, five-hour and Fable (in
    /// that order); Codex its weekly and five-hour windows.
    pub(crate) fn headline_windows(&self) -> Vec<UsageWindow> {
        let usage = self.usage();
        if self.usage_only() {
            // Cursor and Copilot have one month window; Gemini its 5h and 7d.
            return usage;
        }
        let main: Vec<&UsageWindow> = usage
            .iter()
            .filter(|window| window.model.is_none())
            .collect();
        let weekly = main
            .iter()
            .find(|window| window.is_weekly())
            .map(|window| (*window).clone());
        let five_hour = main
            .iter()
            .find(|window| window.is_five_hour())
            .map(|window| (*window).clone());
        if self.provider() != "claude" {
            return [weekly, five_hour].into_iter().flatten().collect();
        }
        let scoped: Vec<&UsageWindow> = usage
            .iter()
            .filter(|window| window.model.is_some())
            .collect();
        let fable = scoped
            .iter()
            .find(|window| {
                window
                    .model
                    .as_deref()
                    .is_some_and(|model| model.to_lowercase().contains("fable"))
            })
            .or_else(|| scoped.first())
            .map(|window| (*window).clone());
        let candidates: Vec<UsageWindow> =
            [weekly, five_hour, fable].into_iter().flatten().collect();
        let mut ranked: Vec<(usize, f64)> = candidates
            .iter()
            .enumerate()
            .map(|(index, window)| (index, window.used_percent))
            .collect();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let tightest: Vec<usize> = ranked.into_iter().take(2).map(|(index, _)| index).collect();
        candidates
            .into_iter()
            .enumerate()
            .filter(|(index, _)| tightest.contains(index))
            .map(|(_, window)| window)
            .collect()
    }

    /// `accountFigures`: the two small figures beside the logo, each with its tooltip label.
    ///
    /// CDXC:AgentProviders 2026-09-08 DECISION:
    /// User: Codex account badges show the five-hour percentage on the second line when that limit exists; otherwise show available resets as "2rs" or "0rs". Use the main account windows so Spark's separate five-hour limit does not stand in for an absent account limit. Claude figures are the two tightest of weekly, five-hour, and Fable (see `headline_windows`).
    pub(crate) fn figures(&self) -> [(Option<String>, String); 2] {
        let windows = self.headline_windows();
        let first = windows.first();
        let second = windows.get(1);
        [
            (
                first.map(|window| window.label.clone()),
                first
                    .map(|window| format!("{}%", window.used_percent.round() as i64))
                    .unwrap_or_else(|| "·".into()),
            ),
            match (second, self.provider() == "codex", self.reset_credits()) {
                (Some(window), _, _) => (
                    Some(window.label.clone()),
                    format!("{}%", window.used_percent.round() as i64),
                ),
                (None, true, Some(credits)) => (
                    Some("Available usage resets".into()),
                    format!("{credits}rs"),
                ),
                _ => (None, "·".into()),
            },
        ]
    }
}

/// `AgentAccountsState`.
#[derive(Clone, Debug)]
pub(crate) struct AccountsState {
    pub(crate) raw: Value,
}

impl AccountsState {
    /// The Claude and Codex accounts, then the usage-only ones (`usageAccounts`).
    pub(crate) fn accounts(&self) -> Vec<Account> {
        ["accounts", "usageAccounts"]
            .into_iter()
            .filter_map(|key| self.raw[key].as_array())
            .flatten()
            .map(|raw| Account { raw: raw.clone() })
            .collect()
    }

    /// The saved accounts of `provider`, by slot.
    pub(crate) fn registered(&self, provider: &str) -> Vec<Account> {
        let mut accounts: Vec<Account> = self
            .accounts()
            .into_iter()
            .filter(|account| account.provider() == provider && account.registered())
            .collect();
        accounts.sort_by(|left, right| {
            left.selector_number()
                .partial_cmp(&right.selector_number())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        accounts
    }

    /// `data.helpers.find(h => h.provider === provider)`.
    pub(crate) fn helper(&self, provider: &str) -> Option<Value> {
        self.raw["helpers"]
            .as_array()
            .and_then(|helpers| helpers.iter().find(|helper| helper["provider"] == provider))
            .cloned()
    }

    pub(crate) fn defaults(&self, provider: &str) -> Value {
        let policy = &self.raw["defaults"][provider];
        if policy.is_object() {
            policy.clone()
        } else {
            serde_json::json!({ "enabled": true, "atLimit": "switch", "priority": "soonestReset", "retryErrors": true })
        }
    }

    pub(crate) fn default_account(&self, provider: &str) -> Option<String> {
        self.raw["defaultAccounts"][provider]
            .as_str()
            .map(str::to_string)
    }

    /// `data.newSessionAccounts?.[provider] ?? { rule: 'auto' }`.
    pub(crate) fn new_session_choice(&self, provider: &str) -> Value {
        let choice = &self.raw["newSessionAccounts"][provider];
        if choice.is_object() {
            choice.clone()
        } else {
            serde_json::json!({ "rule": "auto" })
        }
    }
}

/// `NEW_SESSION_ACCOUNT_RULES`.
pub(crate) fn new_session_rules() -> Vec<(String, String)> {
    settings_catalog()
        .module_value(module::AGENT_ACCOUNTS, "NEW_SESSION_ACCOUNT_RULES")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .map(|rule| (text(rule, "rule"), text(rule, "label")))
                .collect()
        })
        .unwrap_or_default()
}

/// `POLICY_PRIORITY_OPTIONS`.
pub(crate) const POLICY_PRIORITY_OPTIONS: [(&str, &str); 4] = [
    ("leastUsed", "Lowest usage first"),
    ("mostUsed", "Highest usage first"),
    ("soonestReset", "Earliest reset first"),
    ("latestReset", "Latest reset first"),
];

/// `maskAccountText`: every email-shaped run keeps its first and last address characters.
pub(crate) fn mask_account_text(text: &str) -> String {
    let mut out = String::new();
    let characters: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        // Find an `@` whose left and right runs are non-empty and contain no whitespace or `@`.
        if characters[index] == '@' {
            let start = {
                let mut start = index;
                while start > 0
                    && !characters[start - 1].is_whitespace()
                    && characters[start - 1] != '@'
                {
                    start -= 1;
                }
                start
            };
            let end = {
                let mut end = index + 1;
                while end < characters.len()
                    && !characters[end].is_whitespace()
                    && characters[end] != '@'
                {
                    end += 1;
                }
                end
            };
            let local_len = index - start;
            if local_len > 0 && end > index + 1 {
                // The local part was already copied to `out`; replace it.
                for _ in 0..local_len {
                    out.pop();
                }
                let local: Vec<char> = characters[start..index].to_vec();
                out.push(local[0]);
                out.push_str("•••");
                if local.len() > 1 {
                    out.push(*local.last().unwrap_or(&local[0]));
                }
                out.push_str("@•••••.•••");
                index = end;
                continue;
            }
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

/// `normalizeAccountIndicatorInput`: up to two letters or digits; a lone hyphen hides it.
pub(crate) fn normalize_account_indicator_input(value: &str) -> String {
    if value == "-" {
        return "-".into();
    }
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .take(2)
        .collect()
}

/// `AccountHelperTool`.
#[derive(Clone, Debug)]
pub(crate) struct HelperTool {
    pub(crate) raw: Value,
}

impl HelperTool {
    pub(crate) fn provider(&self) -> String {
        text(&self.raw, "provider")
    }

    pub(crate) fn installed(&self) -> bool {
        self.raw["installed"].as_bool() == Some(true)
    }

    pub(crate) fn path(&self) -> Option<String> {
        self.raw
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn version(&self) -> Option<String> {
        self.raw
            .get("version")
            .and_then(Value::as_str)
            .filter(|version| !version.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn latest_version(&self) -> Option<String> {
        self.raw
            .get("latestVersion")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    pub(crate) fn update_available(&self) -> Option<bool> {
        self.raw["updateAvailable"].as_bool()
    }

    pub(crate) fn check_error(&self) -> Option<String> {
        self.raw
            .get("checkError")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    pub(crate) fn can(&self, action: &str) -> bool {
        self.raw["actions"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|candidate| candidate == action))
    }

    /// The running job's action, or `None`.
    pub(crate) fn running_action(&self) -> Option<String> {
        let job = &self.raw["job"];
        (job["status"] == "running").then(|| text(job, "action"))
    }

    pub(crate) fn job(&self) -> Option<&Value> {
        self.raw.get("job").filter(|job| job.is_object())
    }

    /// Tooltip for Install: how Ghostex installs the helper.
    pub(crate) fn install_plan(&self) -> Option<String> {
        self.raw
            .get("installPlan")
            .and_then(Value::as_str)
            .filter(|plan| !plan.is_empty())
            .map(str::to_string)
    }

    /// `pendingSwitch.message`: a Windows Claude Swap update, reinstall or uninstall that waits
    /// for the running Claude sessions to close (server/src/accounts/claude_swap_switch.rs).
    pub(crate) fn pending_switch_message(&self) -> Option<String> {
        self.raw["pendingSwitch"]["message"]
            .as_str()
            .filter(|message| !message.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn unavailable_reason(&self) -> Option<String> {
        self.raw
            .get("unavailableReason")
            .and_then(Value::as_str)
            .filter(|reason| !reason.is_empty())
            .map(str::to_string)
    }

    /// `offersInstall`: gxserver can install the missing helper itself (a remote computer on an
    /// older gxserver cannot).
    pub(crate) fn offers_install(&self) -> bool {
        !self.installed()
            && (self.can("install")
                || self.unavailable_reason().is_some()
                || self.running_action().is_some())
    }
}

/// `ACTION_WORDS`.
pub(crate) fn action_words(action: &str) -> (&'static str, &'static str, &'static str) {
    match action {
        "install" => ("Installing", "installed", "install failed"),
        "update" => ("Updating", "updated", "update failed"),
        "reinstall" => ("Reinstalling", "reinstalled", "reinstall failed"),
        _ => ("Uninstalling", "uninstalled", "uninstall failed"),
    }
}
