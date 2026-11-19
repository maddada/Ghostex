//! CLIProxyAPI hubs (the CLIProxyAPI built-in extension): a hub the user runs themselves, added in
//! Settings > Accounts with its address, a client key and, optionally, its management key. New
//! Claude and Codex sessions can start On the hub (launch.rs), the accounts the hub pools are
//! listed under it with read-only usage (client.rs), and a hub session that every hub account
//! refuses waits for the first one to have room (cooldown.rs, `pool_free_at`).
//!
//! CDXC:AgentProviders 2026-11-18 DECISION:
//! User picked option B on the CLIProxyAPI page ("A, plus the hub's accounts"): the hub is a launch target and its accounts show with read-only meters, "and make the ui for it not show unless user picks to enable cliproxyapi in extensions (build in extension)". While the extension is off gxserver answers no hubs, refuses hub edits and hub launches, so no client can show any of it. Ghostex only points at a hub the user runs: it never installs or starts CLIProxyAPI, never copies Claude Swap or Codex Swap logins into it, never changes the hub's accounts and never redeems a Codex reset through it.
//! SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (`cliProxyApi`), server/src/accounts/endpoint.rs (`cliproxyHubs`), server/src/agents/launch_plan.rs (On the hub), server/src/accounts/recovery.rs (the wait), apps/desktop/src/app/window/settings_modal/tabs/accounts/hubs.rs.

mod client;
pub(crate) mod cooldown;
pub(crate) mod launch;
pub(crate) mod sidebar;
mod store;

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use rand::Rng;
use serde_json::{json, Map, Value};

use super::model::{Provider, UsageWindow};
use super::usage_only::Wait;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use ghostex_settings_catalog::built_in_extensions::{self, CLI_PROXY_API};

/// Whether the CLIProxyAPI extension is on, as this computer's settings file says (off when it
/// says nothing).
pub(crate) fn enabled(paths: &GxserverPaths) -> bool {
    built_in_extensions::enabled_in_value(
        crate::shared_settings_file::read_shared_settings(paths).as_ref(),
        CLI_PROXY_API,
    )
}

fn turned_off() -> DomainStateError {
    DomainStateError::bad_request(built_in_extensions::turned_off_message(CLI_PROXY_API))
}

/// One hub account as last read.
#[derive(Clone)]
struct AccountReading {
    auth_index: String,
    provider: String,
    name: String,
    plan: Option<String>,
    /// A Codex login's ChatGPT account (workspace), which tells two workspaces of one email apart.
    workspace: Option<String>,
    /// `ready`, `coolingDown`, `disabled` or `error`, from the hub's own state.
    status: &'static str,
    status_message: Option<String>,
    /// When the hub uses the account again after a cooldown.
    next_retry_after: Option<DateTime<Utc>>,
    usage: Vec<UsageWindow>,
    usage_error: Option<String>,
}

/// A hub's last reading, or why there is none.
#[derive(Clone)]
struct Reading {
    error: Option<String>,
    accounts: Vec<AccountReading>,
    updated_at: Option<String>,
    fetched_at: Instant,
}

fn readings() -> &'static Mutex<HashMap<String, Reading>> {
    static READINGS: OnceLock<Mutex<HashMap<String, Reading>>> = OnceLock::new();
    READINGS.get_or_init(Default::default)
}

fn reading(id: &str) -> Option<Reading> {
    readings()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
}

fn forget_reading(id: &str) {
    readings()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(id);
}

fn reading_now() -> &'static Mutex<HashSet<String>> {
    static NOW: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    NOW.get_or_init(Default::default)
}

/// A reading is refreshed after two minutes, like the other accounts' usage.
const FRESH_FOR: Duration = Duration::from_secs(120);

fn time(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|at| at.with_timezone(&Utc))
}

/// One `auth-files` entry as an account row, before its usage is read.
fn account_from_file(file: &Value, now: DateTime<Utc>) -> Option<AccountReading> {
    let auth_index = file.get("auth_index").and_then(Value::as_str)?.to_string();
    let text = |key: &str| {
        file.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let provider = text("provider")
        .or_else(|| text("type"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let next_retry_after = time(file.get("next_retry_after")).filter(|at| *at > now);
    let disabled = file.get("disabled").and_then(Value::as_bool) == Some(true)
        || text("status").as_deref() == Some("disabled");
    let status = if disabled {
        "disabled"
    } else if text("status").as_deref() == Some("error") {
        "error"
    } else if file.get("unavailable").and_then(Value::as_bool) == Some(true)
        || next_retry_after.is_some()
    {
        "coolingDown"
    } else {
        "ready"
    };
    Some(AccountReading {
        auth_index,
        name: text("email")
            .or_else(|| text("label"))
            .or_else(|| text("account"))
            .or_else(|| text("name"))
            .unwrap_or_else(|| provider.clone()),
        provider,
        plan: file
            .pointer("/id_token/plan_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        workspace: file
            .pointer("/id_token/chatgpt_account_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        status,
        status_message: text("status_message"),
        next_retry_after,
        usage: Vec::new(),
        usage_error: None,
    })
}

/// Reads a hub's accounts and each one's usage, and keeps the result.
fn refresh(paths: &GxserverPaths, hub: &store::Hub) {
    if !reading_now()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(hub.id.clone())
    {
        return;
    }
    let previous = reading(&hub.id);
    let result = (|| -> Result<Vec<AccountReading>, String> {
        let key = store::open_key(paths, &hub.management_key)?;
        let files = client::auth_files(&hub.url, &key)?;
        let now = Utc::now();
        let reads: Vec<_> = files
            .into_iter()
            .filter_map(|file| Some((account_from_file(&file, now)?, file)))
            .map(|(mut account, file)| {
                let (url, key) = (hub.url.clone(), key.clone());
                std::thread::spawn(move || {
                    if account.status != "disabled" {
                        match client::usage(&url, &key, &file) {
                            Ok(Some(usage)) => account.usage = usage,
                            Ok(None) => {}
                            Err(error) => account.usage_error = Some(error),
                        }
                    }
                    account
                })
            })
            .collect();
        Ok(reads
            .into_iter()
            .filter_map(|read| read.join().ok())
            .collect())
    })();
    let next = match result {
        Ok(mut accounts) => {
            // A failed usage read keeps the account's last meters, marked by its error.
            for account in &mut accounts {
                if account.usage.is_empty() && account.usage_error.is_some() {
                    if let Some(old) = previous.as_ref().and_then(|p| {
                        p.accounts
                            .iter()
                            .find(|a| a.auth_index == account.auth_index)
                    }) {
                        account.usage = old.usage.clone();
                    }
                }
            }
            Reading {
                error: None,
                accounts,
                updated_at: Some(Utc::now().to_rfc3339()),
                fetched_at: Instant::now(),
            }
        }
        Err(error) => Reading {
            error: Some(error),
            accounts: previous
                .as_ref()
                .map(|p| p.accounts.clone())
                .unwrap_or_default(),
            updated_at: previous.and_then(|p| p.updated_at),
            fetched_at: Instant::now(),
        },
    };
    readings()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(hub.id.clone(), next);
    reading_now()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&hub.id);
}

fn host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?.to_string();
            Some(match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            })
        })
        .unwrap_or_else(|| url.to_string())
}

/// The rows for `cliproxyHubs`: none while the extension is off. Keys never leave gxserver; a row
/// only says whether the hub has a management key.
pub(crate) fn rows(paths: &GxserverPaths, wait: Wait) -> Vec<Value> {
    if !enabled(paths) {
        return Vec::new();
    }
    let hubs = {
        let _lock = store::lock();
        store::read(paths).unwrap_or_default()
    };
    let mut now = Vec::new();
    for hub in hubs.iter().filter(|hub| !hub.management_key.is_empty()) {
        match reading(&hub.id) {
            None if wait != Wait::Never => now.push(hub.clone()),
            Some(_) if wait == Wait::Always => now.push(hub.clone()),
            Some(cached) if cached.fetched_at.elapsed() < FRESH_FOR => {}
            _ => {
                let (paths, hub) = (paths.clone(), hub.clone());
                std::thread::spawn(move || refresh(&paths, &hub));
            }
        }
    }
    let threads: Vec<_> = now
        .into_iter()
        .map(|hub| {
            let paths = paths.clone();
            std::thread::spawn(move || refresh(&paths, &hub))
        })
        .collect();
    for thread in threads {
        let _ = thread.join();
    }
    hubs.iter()
        .map(|hub| {
            let has_management_key = !hub.management_key.is_empty();
            let cached = has_management_key.then(|| reading(&hub.id)).flatten();
            let accounts: Vec<Value> = cached
                .as_ref()
                .map(|c| c.accounts.as_slice())
                .unwrap_or(&[])
                .iter()
                .map(|account| {
                    json!({
                        "id": format!("{}:{}", hub.id, account.auth_index),
                        "provider": account.provider,
                        "name": account.name,
                        "plan": account.plan,
                        "status": account.status,
                        "statusMessage": account.status_message,
                        "nextRetryAfter": account.next_retry_after.map(|at| at.to_rfc3339()),
                        "usage": account.usage,
                        "usageError": account.usage_error,
                        "showInSidebar": !hub.hidden_in_sidebar.contains(&account.auth_index),
                    })
                })
                .collect();
            let label = if hub.label.trim().is_empty() {
                host(&hub.url)
            } else {
                hub.label.clone()
            };
            json!({
                "id": hub.id,
                "label": label,
                "customLabel": hub.label,
                "url": hub.url,
                "host": host(&hub.url),
                "hasManagementKey": has_management_key,
                "status": match (&cached, has_management_key) {
                    (_, false) => "noManagementKey",
                    (None, true) => "loading",
                    (Some(c), true) if c.error.is_some() => "unavailable",
                    _ => "ready",
                },
                "error": cached.as_ref().and_then(|c| c.error.clone()),
                "updatedAt": cached.as_ref().and_then(|c| c.updated_at.clone()),
                "accounts": accounts,
            })
        })
        .collect()
}

/// When the first of hub `hub_id`'s `provider` accounts has room again, from the last reading: an
/// account the hub is cooling down frees up at its retry time, one with a used-up window when that
/// window resets. `None` when an account looks free already or nothing says when.
pub(crate) fn pool_free_at(hub_id: &str, provider: Provider) -> Option<DateTime<Utc>> {
    // A limit wait may come long after Settings last read the hub: read it again first.
    if reading(hub_id).is_none_or(|r| r.fetched_at.elapsed() >= FRESH_FOR) {
        let paths = crate::paths::get_gxserver_paths(None);
        if let Some(hub) = store::find(&paths, hub_id).filter(|hub| !hub.management_key.is_empty())
        {
            refresh(&paths, &hub);
        }
    }
    let reading = reading(hub_id)?;
    let now = Utc::now();
    reading
        .accounts
        .iter()
        .filter(|account| account.provider == provider.id() && account.status != "disabled")
        .map(|account| {
            let full: Vec<_> = account
                .usage
                .iter()
                .filter(|w| w.used_percent >= 100.)
                .filter_map(|w| {
                    time(
                        w.resets_at
                            .as_ref()
                            .map(|t| Value::String(t.clone()))
                            .as_ref(),
                    )
                })
                .collect();
            [account.next_retry_after, full.into_iter().max()]
                .into_iter()
                .flatten()
                .filter(|at| *at > now)
                .max()
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min()
}

/// A hub address as saved: `http(s)://host[:port]`, without a path the user may have copied
/// (`/v1`, `/v0/management`, `/management.html`) or a trailing slash.
fn normalize_url(text: &str) -> Result<String, DomainStateError> {
    let invalid =
        || DomainStateError::bad_request("Enter the hub's address, like http://127.0.0.1:8317.");
    let trimmed = text.trim();
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let url = url::Url::parse(&with_scheme).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid());
    }
    let mut path = url.path().trim_end_matches('/').to_string();
    // The address of the hub's own management page (`…/management.html`) works too.
    for suffix in ["/management.html", "/v0/management", "/v1"] {
        if let Some(stripped) = path.strip_suffix(suffix) {
            path = stripped.to_string();
        }
    }
    let mut base = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        base.push_str(&format!(":{port}"));
    }
    Ok(format!("{base}{path}"))
}

fn text_param<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.len() < 4096)
}

fn seal(paths: &GxserverPaths, key: &str) -> Result<String, DomainStateError> {
    crate::vault::seal(paths, key).map_err(DomainStateError::bad_request)
}

fn io_error(error: std::io::Error) -> DomainStateError {
    super::store::error(error)
}

/// Checks a management key against the hub before it is saved.
fn check_management_key(url: &str, key: &str) -> Result<(), DomainStateError> {
    client::auth_files(url, key)
        .map(|_| ())
        .map_err(DomainStateError::bad_request)
}

/// The hub operations Settings sends (`hubAdd`, `hubUpdate`, `hubRemove`). `Ok(false)` when the
/// operation is not one of them; `Ok(true)` when a hub changed and the caller answers with the
/// accounts list.
pub(crate) fn handle(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<bool, DomainStateError> {
    let operation = params
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        operation,
        "hubAdd" | "hubUpdate" | "hubRemove" | "hubAccountSidebar"
    ) {
        return Ok(false);
    }
    if !enabled(paths) {
        return Err(turned_off());
    }
    if operation == "hubAccountSidebar" {
        sidebar::set_shown(paths, params)?;
        return Ok(true);
    }
    let label = params
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|label| label.len() < 120)
        .map(str::to_string);
    match operation {
        "hubAdd" => {
            let url = normalize_url(text_param(params, "url").unwrap_or_default())?;
            let client_key = text_param(params, "clientKey").ok_or_else(|| {
                DomainStateError::bad_request(
                    "Enter a client key: one of the keys under api-keys in the hub's config.",
                )
            })?;
            client::check_client_key(&url, client_key).map_err(DomainStateError::bad_request)?;
            let management_key = text_param(params, "managementKey");
            if let Some(key) = management_key {
                check_management_key(&url, key)?;
            }
            let mut bytes = [0u8; 6];
            rand::rng().fill_bytes(&mut bytes);
            let hub = store::Hub {
                id: format!("hub-{}", hex::encode(bytes)),
                label: label.unwrap_or_default(),
                url,
                client_key: seal(paths, client_key)?,
                management_key: management_key
                    .map(|key| seal(paths, key))
                    .transpose()?
                    .unwrap_or_default(),
                hidden_in_sidebar: Default::default(),
            };
            let _lock = store::lock();
            let mut hubs = store::read(paths).map_err(io_error)?;
            hubs.push(hub);
            store::write(paths, &hubs).map_err(io_error)?;
        }
        "hubUpdate" | "hubRemove" => {
            let id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?
                .to_string();
            let _lock = store::lock();
            let mut hubs = store::read(paths).map_err(io_error)?;
            let index = hubs
                .iter()
                .position(|hub| hub.id == id)
                .ok_or_else(|| DomainStateError::not_found("That hub was already removed."))?;
            if operation == "hubRemove" {
                hubs.remove(index);
                store::write(paths, &hubs).map_err(io_error)?;
                forget_reading(&id);
                launch::forget_key(paths.agent_config_home_dir(), &id);
                return Ok(true);
            }
            let hub = &mut hubs[index];
            let url = match text_param(params, "url") {
                Some(url) => normalize_url(url)?,
                None => hub.url.clone(),
            };
            let client_key = match text_param(params, "clientKey") {
                Some(key) => Some(key.to_string()),
                None if url != hub.url => Some(
                    store::open_key(paths, &hub.client_key)
                        .map_err(DomainStateError::bad_request)?,
                ),
                None => None,
            };
            if let Some(key) = &client_key {
                client::check_client_key(&url, key).map_err(DomainStateError::bad_request)?;
            }
            let clear_management_key =
                params.get("clearManagementKey").and_then(Value::as_bool) == Some(true);
            let management_key = match text_param(params, "managementKey") {
                Some(key) => Some(key.to_string()),
                None if url != hub.url
                    && !hub.management_key.is_empty()
                    && !clear_management_key =>
                {
                    Some(
                        store::open_key(paths, &hub.management_key)
                            .map_err(DomainStateError::bad_request)?,
                    )
                }
                None => None,
            };
            if let Some(key) = &management_key {
                check_management_key(&url, key)?;
            }
            if let Some(label) = label {
                hub.label = label;
            }
            hub.url = url;
            if let Some(key) = client_key {
                hub.client_key = seal(paths, &key)?;
            }
            if let Some(key) = management_key {
                hub.management_key = seal(paths, &key)?;
            } else if clear_management_key {
                hub.management_key.clear();
            }
            store::write(paths, &hubs).map_err(io_error)?;
            forget_reading(&id);
        }
        _ => unreachable!(),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hub_address_keeps_only_its_origin() {
        assert_eq!(
            normalize_url("127.0.0.1:8317").unwrap(),
            "http://127.0.0.1:8317"
        );
        assert_eq!(
            normalize_url("https://hub.example.ts.net:8318/v1/").unwrap(),
            "https://hub.example.ts.net:8318"
        );
        assert_eq!(
            normalize_url("http://localhost:8317/v0/management").unwrap(),
            "http://localhost:8317"
        );
        assert!(normalize_url("ftp://x").is_err());
    }
}
