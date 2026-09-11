use super::{endpoint::account_id, model::*};
use chrono::DateTime;
use std::{cmp::Ordering, collections::BTreeMap};
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: each provider's account for new sessions follows one rule, with Most limit remaining as the default and top option, then Soonest reset, Most used first, Same as last session, and finally any one pinned account. The automatic rules consider only accounts marked Automatic, never Manual ones; a pinned account and the last-session account are explicit choices and may be Manual. Every automatic rule ranks accounts with usage data before those without, and accounts that still have capacity before exhausted ones, then applies its own order: lowest highest-window usage (most remaining), highest usage (most used first), or earliest reset time (soonest reset), with ties falling to the lower slot. Same as last session reuses the account of the most recent launch or switch for that provider, and ranks by Most limit remaining until one exists. When the rule yields no account the launch keeps the current CLI login.
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
        .unwrap_or(NewSessionAccount::MostRemaining);
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
            NewSessionAccount::MostRemaining
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
    registry
        .accounts
        .iter()
        .filter(|a| a.provider == provider && a.eligible)
        .map(|a| (a, usage(a)))
        .min_by(|(a, left), (b, right)| {
            let by_usage = match (left, right) {
                (Some(left), Some(right)) => exhausted(left)
                    .cmp(&exhausted(right))
                    .then_with(|| rank(&rule, left, right)),
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
/// Less is better.
fn rank(rule: &NewSessionAccount, left: &DiscoveredAccount, right: &DiscoveredAccount) -> Ordering {
    match rule {
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
