use super::{endpoint::account_id, model::*};
use chrono::{DateTime, Utc};
use std::{cmp::Ordering, collections::BTreeMap};
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: each provider's account for new sessions follows one rule, with Auto (recommended) as the default and top option, then Most limit remaining, Soonest reset, Most used first, Same as last session, and finally any one pinned account. Auto combines remaining limit and reset time into one number: for every usage window, the remaining percent divided by the hours until that window resets, keeping the smallest across the account's windows as its sustainable rate; the account with the highest sustainable rate wins, so capacity about to refresh is spent first and an account whose weekly limit is nearly gone drops down even when its short window is fresh. The automatic rules consider only accounts marked Automatic, never Manual ones; a pinned account and the last-session account are explicit choices and may be Manual. Every automatic rule ranks accounts with usage data before those without, and accounts that still have capacity before exhausted ones, then applies its own order: lowest highest-window usage (most remaining), highest usage (most used first), or earliest reset time (soonest reset), with ties falling to the lower slot. Same as last session reuses the account of the most recent launch or switch for that provider, and ranks by Auto until one exists. When the rule yields no account the launch keeps the current CLI login.
/// SEE-ALSO: launch::apply_new_session, the `defaultAccounts` and `newSessionAccounts` fields built in endpoint::state_value, packages/shared/agent-accounts.ts `quickLaunchAccountId` and `NEW_SESSION_ACCOUNT_RULES`, packages/core-ui/accounts/manager.tsx (deleted 2026-10-01), apps/desktop/src/app/window/new_thread_picker.rs.
/// CDXC:AgentProviders 2026-11-18 WHY:
/// GitHub #664: with account 1 at 7d 100% (Fable 7d 34%) and account 2 at 7d 85% (Fable 7d 100%), a new non-Fable Claude session started on account 1, whose first prompt then failed with the weekly limit and was moved to account 2 by the at-limit switch. The picker judged an account by every usage window it reports, so account 2's Fable 7d window, which a non-Fable session never uses, made it look full too, and Soonest reset then picked account 1 because it resets first. The picker now counts only the windows the new session's model uses, by the at-limit switch's own rule (`recovery::relevant`): the account-wide 5h and 7d windows always, a model's window (`Fable 7d`) only for a session on that model, never Extra usage, and no window whose reset time has passed (a stale reading of a window that already refreshed). Every rule (Auto, Most limit remaining, Soonest reset, Most used first) ranks on those windows. The model is the one the launch starts on: its `--model`, else Ghostex's remembered default for new sessions, else the model catalog's default row.
/// SEE-ALSO: recovery.rs `relevant` and `trusted_room` (the switch), cli.rs `limited_windows` (`ghostex accounts` readiness, which leaves out model windows and Extra usage the same way).
pub(crate) fn quick_launch_account<'a>(
    registry: &'a Registry,
    snapshot: &Snapshot,
    provider: Provider,
    model: &str,
) -> Option<&'a SavedAccount> {
    pick(registry, snapshot, provider, model, Utc::now())
}
/// The model a new session of `provider` starts on when launched with `command`: the command's own `--model`, else the default Ghostex pins on new sessions (agent_model_pins.rs), else the model catalog's default row. Empty when none is known, which counts every window.
pub(crate) fn launch_model(provider: Provider, command: Option<&str>) -> String {
    command
        .and_then(|command| super::wake_account::command_model(command, provider.id()))
        .or_else(|| crate::agent_model_pins::launch_default(provider.id()).map(|pin| pin.model))
        .or_else(|| {
            crate::agent_model_catalog::catalog_agent(provider.id())?["models"]
                .as_array()?
                .iter()
                .find(|row| row["default"].as_bool() == Some(true))?["value"]
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_default()
}
fn pick<'a>(
    registry: &'a Registry,
    snapshot: &Snapshot,
    provider: Provider,
    model: &str,
    now: DateTime<Utc>,
) -> Option<&'a SavedAccount> {
    // An account the user turned off, or whose login a session reported expired, is never
    // picked, pinned or not.
    let registered = |id: &str| {
        registry.accounts.iter().find(|a| {
            a.id == id && a.provider == provider && !a.user_disabled && a.login_expired_at.is_none()
        })
    };
    let rule = registry
        .new_session_accounts
        .get(&provider)
        .cloned()
        .unwrap_or(NewSessionAccount::Auto);
    let rule = match rule {
        // A hub (CDXC:AgentProviders 2026-11-19 in hub/launch.rs) is no Swap account: the launch
        // starts On the hub instead. A hub that can't be used any more falls back to Auto.
        NewSessionAccount::Pinned { id } if id.starts_with(super::hub::launch::ACCOUNT_PREFIX) => {
            if super::hub::launch::default_hub(registry, provider).is_some() {
                return None;
            }
            NewSessionAccount::Auto
        }
        NewSessionAccount::Pinned { id } => match registered(&id) {
            Some(account) => return Some(account),
            // A pinned account the user turned off (or that needs signing in again) falls back to
            // Auto; a removed one keeps the CLI login as before.
            None if registry.accounts.iter().any(|a| a.id == id) => NewSessionAccount::Auto,
            None => return None,
        },
        NewSessionAccount::LastUsed => {
            if let Some(account) = registry
                .last_used_accounts
                .get(&provider)
                .and_then(|id| registered(id))
                .filter(|account| account.disabled.is_none())
            {
                return Some(account);
            }
            NewSessionAccount::Auto
        }
        rule => rule,
    };
    let usage = |account: &SavedAccount| {
        snapshot
            .accounts
            .iter()
            .find(|d| account_id(d) == account.id)
            .filter(|d| d.status == "ready" && d.usage_error.is_none() && !d.usage.is_empty())
            .map(|d| Reading {
                windows: d
                    .usage
                    .iter()
                    .filter(|w| super::recovery::relevant(w, model) && w.id != "spend")
                    .map(|w| (w, reset_at(w).filter(|reset| *reset <= now).is_some()))
                    .collect(),
            })
    };
    let slot = |account: &SavedAccount| account.selector.parse::<u32>().unwrap_or(u32::MAX);
    registry
        .accounts
        .iter()
        .filter(|a| a.provider == provider && a.auto_switchable())
        .map(|a| (a, usage(a)))
        .min_by(|(a, left), (b, right)| {
            let by_usage = match (left, right) {
                (Some(left), Some(right)) => left
                    .exhausted()
                    .cmp(&right.exhausted())
                    .then_with(|| rank(&rule, left, right, now)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };
            by_usage
                .then_with(|| slot(a).cmp(&slot(b)))
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|(a, _)| a)
}
fn reset_at(window: &UsageWindow) -> Option<DateTime<Utc>> {
    window
        .resets_at
        .as_deref()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
}
/// The usage windows that apply to the new session, each with whether its reset time has passed.
struct Reading<'a> {
    windows: Vec<(&'a UsageWindow, bool)>,
}
impl Reading<'_> {
    /// The windows still running; one that already reset no longer limits anything.
    fn current(&self) -> impl Iterator<Item = &UsageWindow> {
        self.windows
            .iter()
            .filter(|(_, reset)| !reset)
            .map(|(w, _)| *w)
    }
    fn highest_used(&self) -> f64 {
        self.current().map(|w| w.used_percent).fold(0f64, f64::max)
    }
    fn exhausted(&self) -> bool {
        self.highest_used() >= 100.
    }
    fn soonest_reset(&self) -> Option<i64> {
        self.current()
            .filter_map(reset_at)
            .map(|t| t.timestamp())
            .min()
    }
    /// Remaining percent per hour until reset, taken over the tightest window. Windows without a reset time use their window length; a window that already reset is a fresh one, all of it left over its full length.
    fn sustainable_rate(&self, now: DateTime<Utc>) -> Option<f64> {
        self.windows
            .iter()
            .filter_map(|(w, reset)| {
                let length = || Some(w.limit_window_seconds.filter(|s| *s > 0)? as f64 / 3600.);
                if *reset {
                    return Some(100. / length()?);
                }
                let hours = match reset_at(w) {
                    Some(reset) => (reset - now).num_seconds().max(60) as f64 / 3600.,
                    None => length()?,
                };
                Some((100. - w.used_percent).max(0.) / hours)
            })
            .min_by(f64::total_cmp)
    }
}
/// Less is better.
fn rank(rule: &NewSessionAccount, left: &Reading, right: &Reading, now: DateTime<Utc>) -> Ordering {
    match rule {
        NewSessionAccount::Auto => {
            match (left.sustainable_rate(now), right.sustainable_rate(now)) {
                (Some(left), Some(right)) => right.total_cmp(&left),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
        NewSessionAccount::MostRemaining => left.highest_used().total_cmp(&right.highest_used()),
        NewSessionAccount::MostUsed => right.highest_used().total_cmp(&left.highest_used()),
        NewSessionAccount::SoonestReset => match (left.soonest_reset(), right.soonest_reset()) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        NewSessionAccount::LastUsed | NewSessionAccount::Pinned { .. } => Ordering::Equal,
    }
}
/// The account a new session of each provider starts on, as the launchers show it: for the model a launch without a model of its own starts on.
pub(crate) fn quick_launch_accounts(
    registry: &Registry,
    snapshot: &Snapshot,
) -> BTreeMap<Provider, String> {
    [Provider::Claude, Provider::Codex]
        .into_iter()
        .filter_map(|provider| {
            if let Some(hub) = super::hub::launch::default_hub(registry, provider) {
                return Some((
                    provider,
                    format!("{}{hub}", super::hub::launch::ACCOUNT_PREFIX),
                ));
            }
            quick_launch_account(registry, snapshot, provider, &launch_model(provider, None))
                .map(|a| (provider, a.id.clone()))
        })
        .collect()
}
/// For each provider whose New sessions start on names a CLIProxyAPI hub: the Swap account Auto
/// would pick, for what can't run on a hub (a Claude cloud session needs a claude.ai login).
pub(crate) fn hub_fallback_accounts(
    registry: &Registry,
    snapshot: &Snapshot,
) -> BTreeMap<Provider, String> {
    let mut auto = registry.clone();
    [Provider::Claude, Provider::Codex]
        .into_iter()
        .filter(|provider| super::hub::launch::default_hub(registry, *provider).is_some())
        .filter_map(|provider| {
            auto.new_session_accounts.remove(&provider);
            quick_launch_account(&auto, snapshot, provider, &launch_model(provider, None))
                .map(|a| (provider, a.id.clone()))
        })
        .collect()
}
pub(crate) fn record_last_used(
    db: &rusqlite::Connection,
    registry: &Registry,
    provider: Provider,
    id: &str,
) -> Result<(), crate::domain::DomainStateError> {
    if registry
        .last_used_accounts
        .get(&provider)
        .map(String::as_str)
        == Some(id)
    {
        return Ok(());
    }
    let mut updated = registry.clone();
    updated.last_used_accounts.insert(provider, id.to_string());
    super::store::write(db, &updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: &str, used: f64, resets_at: &str, model: Option<&str>) -> UsageWindow {
        UsageWindow {
            id: id.into(),
            label: model.map_or("7d".into(), |m| format!("{m} 7d")),
            used_percent: used,
            limit_window_seconds: Some(604_800),
            resets_at: Some(resets_at.into()),
            model: model.map(str::to_string),
        }
    }

    fn account(selector: &str, usage: Vec<UsageWindow>) -> (SavedAccount, DiscoveredAccount) {
        let found = DiscoveredAccount {
            provider: Provider::Claude,
            selector: selector.into(),
            identity: format!("identity-{selector}"),
            name: format!("Account {selector}"),
            email: String::new(),
            status: "ready".into(),
            shared_history: true,
            usage,
            reset_credits: None,
            reset_credit_details: None,
            reset_credits_error: None,
            usage_updated_at: Some("2026-11-18T00:16:00Z".into()),
            usage_error: None,
            usage_stale: false,
            plan: None,
        };
        let saved = SavedAccount {
            id: account_id(&found),
            provider: Provider::Claude,
            selector: selector.into(),
            identity: found.identity.clone(),
            name: found.name.clone(),
            color: "coral".into(),
            indicator: String::new(),
            show_in_titlebar: true,
            eligible: true,
            shared_history: true,
            disabled: None,
            user_disabled: false,
            login_expired_at: None,
            login_confirmed_at: None,
            synced: None,
            sign_in_ignored: false,
        };
        (saved, found)
    }

    fn at(time: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(time)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// GitHub #664: account 1 at 7d 100% (Fable 7d 34%), account 2 at 7d 85% (Fable 7d 100%).
    #[test]
    fn a_full_model_window_only_counts_for_a_session_on_that_model() {
        let (one, one_found) = account(
            "1",
            vec![
                window("sevenDay", 100., "2026-11-18T03:00:00Z", None),
                window("Fable", 34., "2026-11-20T12:00:00Z", Some("Fable")),
            ],
        );
        let (two, two_found) = account(
            "2",
            vec![
                window("sevenDay", 85., "2026-11-18T21:00:00Z", None),
                window("Fable", 100., "2026-11-19T09:00:00Z", Some("Fable")),
            ],
        );
        let snapshot = Snapshot {
            accounts: vec![one_found, two_found],
            errors: BTreeMap::new(),
            fetched_at: None,
        };
        let mut registry = Registry {
            accounts: vec![one.clone(), two.clone()],
            ..Registry::default()
        };
        let before_reset = at("2026-11-18T00:21:02Z");
        for rule in [
            NewSessionAccount::Auto,
            NewSessionAccount::SoonestReset,
            NewSessionAccount::MostRemaining,
            NewSessionAccount::MostUsed,
        ] {
            registry
                .new_session_accounts
                .insert(Provider::Claude, rule.clone());
            for model in ["opus[1m]", "sonnet", "claude-opus-4-8"] {
                let picked = pick(&registry, &snapshot, Provider::Claude, model, before_reset);
                assert_eq!(picked.map(|a| &a.id), Some(&two.id), "{rule:?} on {model}");
            }
        }
        // Once account 1's 7d window has reset, its stale 100% no longer makes it full.
        registry
            .new_session_accounts
            .insert(Provider::Claude, NewSessionAccount::MostRemaining);
        let after_reset = at("2026-11-18T03:05:00Z");
        let picked = pick(
            &registry,
            &snapshot,
            Provider::Claude,
            "opus[1m]",
            after_reset,
        );
        assert_eq!(picked.map(|a| &a.id), Some(&one.id));
    }
}
