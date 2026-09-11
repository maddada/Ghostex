use super::{endpoint::account_id, model::*};
use chrono::{DateTime, Utc};
use std::{cmp::Ordering, collections::BTreeMap};
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: each provider's account for new sessions follows one rule, with Auto (recommended) as the default and top option, then Most limit remaining, Soonest reset, Most used first, Same as last session, and finally any one pinned account. Auto combines remaining limit and reset time into one number: for every usage window, the remaining percent divided by the hours until that window resets, keeping the smallest across the account's windows as its sustainable rate; the account with the highest sustainable rate wins, so capacity about to refresh is spent first and an account whose weekly limit is nearly gone drops down even when its short window is fresh. The automatic rules consider only accounts marked Automatic, never Manual ones; a pinned account and the last-session account are explicit choices and may be Manual. Every automatic rule ranks accounts with usage data before those without, and accounts that still have capacity before exhausted ones, then applies its own order: lowest highest-window usage (most remaining), highest usage (most used first), or earliest reset time (soonest reset), with ties falling to the lower slot. Same as last session reuses the account of the most recent launch or switch for that provider, and ranks by Auto until one exists. When the rule yields no account the launch keeps the current CLI login.
/// SEE-ALSO: launch::apply_new_session, the `defaultAccounts` and `newSessionAccounts` fields built in endpoint::state_value, packages/shared/agent-accounts.ts `quickLaunchAccountId` and `NEW_SESSION_ACCOUNT_RULES`, packages/core-ui/accounts/manager.tsx, apps/desktop/src/app/window/new_thread_picker.rs.
pub(crate) fn quick_launch_account<'a>(
    registry: &'a Registry,
    snapshot: &Snapshot,
    provider: Provider,
) -> Option<&'a SavedAccount> {
    let registered = |id: &str| {
        registry
            .accounts
            .iter()
            .find(|a| a.id == id && a.provider == provider)
    };
    let rule = registry
        .new_session_accounts
        .get(&provider)
        .cloned()
        .unwrap_or(NewSessionAccount::Auto);
    let rule = match rule {
        NewSessionAccount::Pinned { id } => return registered(&id),
        NewSessionAccount::LastUsed => {
            if let Some(account) = registry
                .last_used_accounts
                .get(&provider)
                .and_then(|id| registered(id))
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
    };
    let slot = |account: &SavedAccount| account.selector.parse::<u32>().unwrap_or(u32::MAX);
    let now = Utc::now();
    registry
        .accounts
        .iter()
        .filter(|a| a.provider == provider && a.eligible)
        .map(|a| (a, usage(a)))
        .min_by(|(a, left), (b, right)| {
            let by_usage = match (left, right) {
                (Some(left), Some(right)) => exhausted(left)
                    .cmp(&exhausted(right))
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
fn highest_used(account: &DiscoveredAccount) -> f64 {
    account
        .usage
        .iter()
        .map(|w| w.used_percent)
        .fold(0f64, f64::max)
}
fn exhausted(account: &DiscoveredAccount) -> bool {
    highest_used(account) >= 100.
}
fn soonest_reset(account: &DiscoveredAccount) -> Option<i64> {
    account
        .usage
        .iter()
        .filter_map(|w| w.resets_at.as_deref())
        .filter_map(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp())
        .min()
}
/// Remaining percent per hour until reset, taken over the account's tightest window. Windows without a reset time use their window length; a window that already reset counts as one minute away so its capacity is spent first.
fn sustainable_rate(account: &DiscoveredAccount, now: DateTime<Utc>) -> Option<f64> {
    account
        .usage
        .iter()
        .filter_map(|w| {
            let hours = match w
                .resets_at
                .as_deref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            {
                Some(reset) => {
                    (reset.with_timezone(&Utc) - now).num_seconds().max(60) as f64 / 3600.
                }
                None => w.limit_window_seconds.filter(|s| *s > 0)? as f64 / 3600.,
            };
            Some((100. - w.used_percent).max(0.) / hours)
        })
        .min_by(f64::total_cmp)
}
/// Less is better.
fn rank(
    rule: &NewSessionAccount,
    left: &DiscoveredAccount,
    right: &DiscoveredAccount,
    now: DateTime<Utc>,
) -> Ordering {
    match rule {
        NewSessionAccount::Auto => {
            match (sustainable_rate(left, now), sustainable_rate(right, now)) {
                (Some(left), Some(right)) => right.total_cmp(&left),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
        NewSessionAccount::MostRemaining => highest_used(left).total_cmp(&highest_used(right)),
        NewSessionAccount::MostUsed => highest_used(right).total_cmp(&highest_used(left)),
        NewSessionAccount::SoonestReset => match (soonest_reset(left), soonest_reset(right)) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        NewSessionAccount::LastUsed | NewSessionAccount::Pinned { .. } => Ordering::Equal,
    }
}
pub(crate) fn quick_launch_accounts(
    registry: &Registry,
    snapshot: &Snapshot,
) -> BTreeMap<Provider, String> {
    [Provider::Claude, Provider::Codex]
        .into_iter()
        .filter_map(|provider| {
            quick_launch_account(registry, snapshot, provider).map(|a| (provider, a.id.clone()))
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
