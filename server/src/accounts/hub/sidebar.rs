//! Hub accounts in the sidebar's usage meters: the `hubAccounts` of the `titlebar` answer, and the
//! Show usage in the sidebar switch of a hub account (`hubAccountSidebar`).
//!
//! CDXC:AgentProviders 2026-11-19 DECISION:
//! User: "i wanna still see usage of each account in bottom of sidebar as i do now without having to login to each one in cswap or xswap". Every account a hub pools shows a meter at the bottom of the sidebar, beside the Claude Swap and Codex Swap ones, until the user turns it off in that account's pane. The user's hubs hold the management key on the hub's own computer only (Q4202 "1A"): the gxserver there reads the hub, and the other computers show its meters through their connection to it, so a row's id names the login (`hub-<provider>-<hash of email and workspace>`), not the hub, and the same login read by two computers shows once.
//! SEE-ALSO: apps/desktop/src/app/titlebar/account_usage.rs (`refresh_titlebar_accounts`, `distinct_accounts`), apps/desktop/src/app/window/settings_modal/tabs/accounts/hubs.rs.

use serde_json::{Map, Value, json};

use super::{AccountReading, FRESH_FOR, reading, store};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// The meter id of a hub account: its login (provider, email and, for Codex, workspace), hashed
/// into the letters, digits and dashes a sidebar meter's key allows, so every computer and hub that
/// reads the login agrees on it.
fn meter_id(hub: &store::Hub, account: &AccountReading) -> String {
    use sha2::{Digest, Sha256};
    let login = if account.name.contains('@') {
        format!(
            "{}|{}",
            account.name.to_ascii_lowercase(),
            account.workspace.as_deref().unwrap_or_default()
        )
    } else {
        format!("{}|{}", hub.id, account.auth_index)
    };
    let digest = hex::encode(Sha256::digest(login.as_bytes()));
    format!("hub-{}-{}", account.provider, &digest[..16])
}

/// The `hubAccounts` rows: each Claude and Codex account of every hub this computer reads (one
/// with a management key), shaped like the sidebar's other account rows. Never waits on a hub: a
/// missing or old reading is refreshed in the background and shows on the next poll.
pub(crate) fn titlebar_rows(paths: &GxserverPaths) -> Vec<Value> {
    if !super::enabled(paths) {
        return Vec::new();
    }
    let hubs = {
        let _lock = store::lock();
        store::read(paths).unwrap_or_default()
    };
    let mut rows = Vec::new();
    for hub in hubs.iter().filter(|hub| !hub.management_key.is_empty()) {
        let cached = reading(&hub.id);
        if cached
            .as_ref()
            .is_none_or(|c| c.fetched_at.elapsed() >= FRESH_FOR)
        {
            let (paths, hub) = (paths.clone(), hub.clone());
            std::thread::spawn(move || super::refresh(&paths, &hub));
        }
        let Some(cached) = cached else {
            continue;
        };
        let label = if hub.label.trim().is_empty() {
            super::host(&hub.url)
        } else {
            hub.label.clone()
        };
        let mut numbers = std::collections::HashMap::<&str, usize>::new();
        for account in &cached.accounts {
            if !matches!(account.provider.as_str(), "claude" | "codex")
                || account.status == "disabled"
            {
                continue;
            }
            let number = numbers.entry(account.provider.as_str()).or_insert(0);
            *number += 1;
            let shown = !hub.hidden_in_sidebar.contains(&account.auth_index);
            let usage_error = account.usage_error.clone().or_else(|| {
                (account.status == "error").then(|| {
                    account.status_message.clone().unwrap_or_else(|| {
                        "The hub can't use this account. Sign it in again on the hub.".into()
                    })
                })
            });
            rows.push(json!({
                "id": meter_id(hub, account),
                "hub": true,
                "hubAccountId": format!("{}:{}", hub.id, account.auth_index),
                "hubLabel": label,
                "provider": account.provider,
                "selector": format!("H{number}"),
                "indicator": "",
                "name": account.name,
                "email": if account.name.contains('@') { account.name.as_str() } else { "" },
                "workspace": account.workspace,
                "plan": account.plan,
                "registered": true,
                "showInTitlebar": shown,
                "status": "ready",
                "usage": account.usage,
                "usageError": usage_error,
                "usageUpdatedAt": cached.updated_at,
                "usageStale": cached.error.is_some(),
            }));
        }
    }
    rows
}

/// `hubAccountSidebar`: shows (`shown: true`) or hides hub account `accountId`
/// (`<hub id>:<auth_index>`) in the sidebar's usage meters; the hub as saved.
pub(crate) fn set_shown(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<store::Hub, DomainStateError> {
    let id = params
        .get("accountId")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::bad_request("accountId is required"))?;
    let (hub_id, auth_index) = id
        .split_once(':')
        .ok_or_else(|| DomainStateError::bad_request("That isn't a hub account."))?;
    let shown = params
        .get("shown")
        .and_then(Value::as_bool)
        .ok_or_else(|| DomainStateError::bad_request("shown is required"))?;
    let _lock = store::lock();
    let mut hubs = store::read(paths).map_err(super::io_error)?;
    let hub = hubs
        .iter_mut()
        .find(|hub| hub.id == hub_id)
        .ok_or_else(|| DomainStateError::not_found("That hub was already removed."))?;
    if shown {
        hub.hidden_in_sidebar.remove(auth_index);
    } else {
        hub.hidden_in_sidebar.insert(auth_index.to_string());
    }
    let saved = hub.clone();
    store::write(paths, &hubs).map_err(super::io_error)?;
    Ok(saved)
}
