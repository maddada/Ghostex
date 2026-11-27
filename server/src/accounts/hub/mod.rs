//! CLIProxyAPI hubs (the CLIProxyAPI built-in extension): a hub the user runs themselves, added in
//! Settings > Accounts with its address, a client key and, optionally, its management key. New
//! Claude and Codex sessions can start On the hub (launch.rs), the accounts the hub pools are
//! listed under it with read-only usage (client.rs), and a hub session that every hub account
//! refuses waits for the first one to have room (cooldown.rs, `pool_free_at`).
//!
//! CDXC:AgentProviders 2026-11-18 DECISION:
//! User picked option B on the CLIProxyAPI page ("A, plus the hub's accounts"): the hub is a launch target and its accounts show with read-only meters, "and make the ui for it not show unless user picks to enable cliproxyapi in extensions (build in extension)". While the extension is off gxserver answers no hubs, refuses hub edits and hub launches, so no client can show any of it. Ghostex only points at a hub the user runs: it never installs or starts CLIProxyAPI, never copies Claude Swap or Codex Swap logins into it, never changes the hub's accounts and never redeems a Codex reset through it.
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! Amended to allow one sign-in flow (`signin.rs`): when the user clicks Add account for a configured hub in Settings > Accounts, Ghostex on the browser's computer briefly listens on `127.0.0.1:54545` (Claude `/callback`) or `127.0.0.1:1455` (Codex `/auth/callback`), asks the hub's management API for the OAuth URL, catches the single callback, forwards code and state to `/v0/management/oauth-callback`, and refreshes the hub's account list. Ghostex still never installs, starts or reconfigures CLIProxyAPI, never copies Claude Swap or Codex Swap logins into it, never downloads/deletes/edits hub credentials, and never redeems a Codex reset through it.
//! SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (`cliProxyApi`), server/src/accounts/endpoint.rs (`cliproxyHubs`), server/src/agents/launch_plan.rs (On the hub), server/src/accounts/recovery.rs (the wait), apps/desktop/src/app/window/settings_modal/tabs/accounts/hubs.rs.

mod client;
pub(crate) mod cooldown;
pub(crate) mod launch;
pub(crate) mod sidebar;
mod signin;
mod store;
mod usage_reads;

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
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
    DomainStateError::bad_request(built_in_extensions::turned_off_on_message(
        CLI_PROXY_API,
        &crate::connected_computers::this_computer_name(),
    ))
}

/// One hub account as last read.
#[derive(Clone)]
struct AccountReading {
    auth_index: String,
    file_name: String,
    provider: String,
    name: String,
    plan: Option<String>,
    /// A Codex login's ChatGPT account (workspace), which tells two workspaces of one email apart.
    workspace: Option<String>,
    /// `ready`, `coolingDown`, `disabled` or `error`, from the hub's own state.
    status: &'static str,
    status_message: Option<String>,
    requires_sign_in: bool,
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
    session_affinity: Option<bool>,
    routing_strategy: Option<String>,
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

/// One `auth-files` / `credentials` entry as an account row, before its usage is read.
/// Only Claude and Codex accounts are shown or managed.
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
    if !matches!(provider.as_str(), "claude" | "codex") {
        return None;
    }
    let file_name = text("name").or_else(|| text("id")).unwrap_or_default();
    let requires_sign_in = file.get("requires_sign_in").and_then(Value::as_bool) == Some(true);
    let next_retry_after = time(file.get("next_retry_after")).filter(|at| *at > now);
    let disabled = file.get("disabled").and_then(Value::as_bool) == Some(true)
        || text("status").as_deref() == Some("disabled");
    let status = if disabled {
        "disabled"
    } else if requires_sign_in || text("status").as_deref() == Some("error") {
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
        file_name,
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
        status_message: text("status_message")
            .map(|message| client::masked(&message))
            .filter(|message| !message.is_empty()),
        requires_sign_in,
        next_retry_after,
        usage: Vec::new(),
        usage_error: None,
    })
}

/// Reads a hub's accounts, routing settings, and each account's usage, and keeps the result.
fn refresh(paths: &GxserverPaths, hub: &store::Hub) {
    if !reading_now()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(hub.id.clone())
    {
        return;
    }
    let previous = reading(&hub.id);
    let store_reading = |reading: Reading| {
        readings()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(hub.id.clone(), reading);
    };
    let listed = (|| -> Result<(String, Vec<Value>, client::RoutingInfo), String> {
        let key = store::open_key(paths, &hub.management_key)?;
        let files = client::auth_files(&hub.url, &key)?;
        let routing = client::read_routing(&hub.url, &key).unwrap_or_default();
        Ok((key, files, routing))
    })();
    match listed {
        Ok((key, files, routing)) => {
            let now = Utc::now();
            let mut accounts: Vec<AccountReading> = files
                .iter()
                .filter_map(|file| account_from_file(file, now))
                .collect();
            let updated_at = Some(Utc::now().to_rfc3339());
            // Usage comes from each account's last reading, and due ones are read one at a time
            // (usage_reads.rs); the list shows at once and each new reading as it lands.
            usage_reads::fill(paths, hub, &key, &mut accounts, &files, |accounts| {
                store_reading(Reading {
                    error: None,
                    accounts: accounts.to_vec(),
                    session_affinity: routing.session_affinity,
                    routing_strategy: routing.strategy.clone(),
                    updated_at: updated_at.clone(),
                    fetched_at: Instant::now(),
                });
            });
            store_reading(Reading {
                error: None,
                accounts,
                session_affinity: routing.session_affinity,
                routing_strategy: routing.strategy,
                updated_at,
                fetched_at: Instant::now(),
            });
        }
        Err(error) => store_reading(Reading {
            error: Some(error),
            accounts: previous
                .as_ref()
                .map(|p| p.accounts.clone())
                .unwrap_or_default(),
            session_affinity: previous.as_ref().and_then(|p| p.session_affinity),
            routing_strategy: previous.as_ref().and_then(|p| p.routing_strategy.clone()),
            updated_at: previous.and_then(|p| p.updated_at),
            fetched_at: Instant::now(),
        }),
    }
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
                        "authIndex": account.auth_index,
                        "fileName": account.file_name,
                        "provider": account.provider,
                        "name": account.name,
                        "plan": account.plan,
                        "workspace": account.workspace,
                        "status": account.status,
                        "statusMessage": account.status_message,
                        "requiresSignIn": account.requires_sign_in,
                        "nextRetryAfter": account.next_retry_after.map(|at| at.to_rfc3339()),
                        "usage": account.usage,
                        "usageError": account.usage_error,
                        "showInSidebar": !hub.hidden_in_sidebar.contains(&account.auth_index),
                        "canManage": has_management_key && !account.file_name.is_empty(),
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
                "sessionAffinity": cached.as_ref().and_then(|c| c.session_affinity),
                "routingStrategy": cached.as_ref().and_then(|c| c.routing_strategy.clone()),
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
    for suffix in [
        "/management.html",
        "/v8/management",
        "/v0/management",
        "/v1",
    ] {
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
        .filter(|text| !text.is_empty() && text.chars().count() <= 2048)
}

fn seal(paths: &GxserverPaths, key: &str) -> Result<String, DomainStateError> {
    crate::vault::seal(paths, key).map_err(DomainStateError::bad_request)
}

fn io_error(error: std::io::Error) -> DomainStateError {
    super::store::error(error)
}

/// A hub's id from its address, so a hub removed and added again at the same address gets its id
/// back, and with it its profile folders and the sessions that ran there.
fn hub_id_for(url: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("hub-{}", &hex::encode(Sha256::digest(url.as_bytes()))[..12])
}

/// Checks a management key against the hub before it is saved.
fn check_management_key(url: &str, key: &str) -> Result<(), DomainStateError> {
    client::auth_files(url, key)
        .map(|_| ())
        .map_err(DomainStateError::bad_request)
}

fn managed_hub(
    paths: &GxserverPaths,
    hub_id: &str,
) -> Result<(store::Hub, String), DomainStateError> {
    let hub = {
        let _lock = store::lock();
        store::read(paths)
            .map_err(io_error)?
            .into_iter()
            .find(|hub| hub.id == hub_id)
            .ok_or_else(|| DomainStateError::not_found("That hub was already removed."))?
    };
    if hub.management_key.is_empty() {
        return Err(DomainStateError::bad_request(
            "Add this hub's management key first to manage its accounts.",
        ));
    }
    let key = store::open_key(paths, &hub.management_key).map_err(DomainStateError::bad_request)?;
    Ok((hub, key))
}

fn parse_hub_account_id(params: &Map<String, Value>) -> Result<(String, String), DomainStateError> {
    let id = text_param(params, "accountId")
        .ok_or_else(|| DomainStateError::bad_request("accountId is required"))?;
    let (hub_id, auth_index) = id
        .split_once(':')
        .filter(|(h, a)| !h.is_empty() && !a.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("That isn't a hub account."))?;
    Ok((hub_id.to_string(), auth_index.to_string()))
}

fn is_supported_provider(file: &Value) -> bool {
    let provider = file
        .get("provider")
        .or_else(|| file.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(provider.as_str(), "claude" | "codex")
}

fn find_account_file_name(
    hub: &store::Hub,
    key: &str,
    auth_index: &str,
) -> Result<String, DomainStateError> {
    if let Some(name) = reading(&hub.id).and_then(|r| {
        r.accounts
            .into_iter()
            .find(|a| {
                a.auth_index == auth_index
                    && matches!(a.provider.as_str(), "claude" | "codex")
                    && !a.file_name.is_empty()
            })
            .map(|a| a.file_name)
    }) {
        return Ok(name);
    }
    let files = client::auth_files(&hub.url, key).map_err(DomainStateError::bad_request)?;
    files
        .iter()
        .find(|file| {
            file.get("auth_index").and_then(Value::as_str) == Some(auth_index)
                && is_supported_provider(file)
        })
        .and_then(|file| {
            file.get("name")
                .or_else(|| file.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| {
            DomainStateError::not_found("That Claude or Codex account is no longer on the hub.")
        })
}

fn ensure_supported_account(
    hub: &store::Hub,
    key: &str,
    auth_index: &str,
) -> Result<(), DomainStateError> {
    if reading(&hub.id).is_some_and(|r| {
        r.accounts.iter().any(|a| {
            a.auth_index == auth_index && matches!(a.provider.as_str(), "claude" | "codex")
        })
    }) {
        return Ok(());
    }
    let files = client::auth_files(&hub.url, key).map_err(DomainStateError::bad_request)?;
    if files.iter().any(|file| {
        file.get("auth_index").and_then(Value::as_str) == Some(auth_index)
            && is_supported_provider(file)
    }) {
        return Ok(());
    }
    Err(DomainStateError::not_found(
        "That Claude or Codex account is no longer on the hub.",
    ))
}

/// The hub operations Settings sends (`hubAdd`, `hubUpdate`, `hubRemove`, `hubAccountSidebar`,
/// plus v8 management operations for OAuth, account removal/toggle, cooldown reset, and session
/// affinity).
pub(crate) fn handle(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Handled, DomainStateError> {
    let operation = params
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        operation,
        "hubAdd"
            | "hubUpdate"
            | "hubRemove"
            | "hubAccountSidebar"
            | "hubOAuthStart"
            | "hubOAuthStatus"
            | "hubOAuthCallback"
            | "hubOAuthCancel"
            | "hubAccountRemove"
            | "hubAccountSetEnabled"
            | "hubAccountResetCooldown"
            | "hubSetSessionAffinity"
    ) {
        return Ok(Handled::NotMine);
    }
    if !enabled(paths) {
        return Err(turned_off());
    }
    match operation {
        "hubAccountSidebar" => {
            let hub = sidebar::set_shown(paths, params)?;
            refresh_soon(paths, &hub);
            return Ok(Handled::Changed);
        }
        "hubOAuthStart" => {
            let hub_id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?;
            let provider = text_param(params, "provider")
                .ok_or_else(|| DomainStateError::bad_request("provider is required"))?
                .to_ascii_lowercase();
            let (hub, key) = managed_hub(paths, hub_id)?;
            // When the caller's browser runs on this computer (`localCallback: true`), Ghostex
            // itself binds 127.0.0.1:54545 (Claude) or 127.0.0.1:1455 (Codex), catches the single
            // callback, validates state and forwards it to the hub (local or remote over Tailscale).
            let caller_local = params
                .get("localCallback")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let started = if caller_local {
                signin::start_local_relay(paths, &hub, &key, &provider)?
            } else {
                client::oauth_start(&hub.url, &key, &provider, false)
                    .map_err(DomainStateError::bad_request)?
            };
            return Ok(Handled::Answer(json!({
                "status": "ok",
                "hubId": hub.id,
                "provider": provider,
                "url": started.url,
                "state": started.state,
                "isLocal": started.local_callback,
            })));
        }
        "hubOAuthStatus" => {
            let hub_id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?;
            let state = text_param(params, "state")
                .ok_or_else(|| DomainStateError::bad_request("state is required"))?;
            let (hub, key) = managed_hub(paths, hub_id)?;
            match signin::relay_outcome(state) {
                Some(signin::RelayOutcome::Complete) => {
                    return Ok(Handled::Answer(json!({
                        "status": "ok",
                        "error": Value::Null,
                    })));
                }
                Some(signin::RelayOutcome::Failed(error)) => {
                    return Ok(Handled::Answer(json!({
                        "status": "error",
                        "error": error,
                    })));
                }
                Some(signin::RelayOutcome::Waiting) | None => {}
            }
            let status = client::oauth_status(&hub.url, &key, state)
                .map_err(DomainStateError::bad_request)?;
            if status.status == "ok" {
                signin::cancel_relay(state);
                refresh_after_sign_in(paths, &hub);
            } else if status.status == "error" {
                signin::cancel_relay(state);
            }
            return Ok(Handled::Answer(json!({
                "status": status.status,
                "error": status.error,
            })));
        }
        "hubOAuthCallback" => {
            let hub_id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?;
            let provider = text_param(params, "provider")
                .ok_or_else(|| DomainStateError::bad_request("provider is required"))?
                .to_ascii_lowercase();
            let state = text_param(params, "state")
                .ok_or_else(|| DomainStateError::bad_request("state is required"))?;
            let input = text_param(params, "input")
                .ok_or_else(|| DomainStateError::bad_request("Paste the redirect URL or code."))?;
            let (hub, key) = managed_hub(paths, hub_id)?;
            signin::cancel_relay(state);
            client::oauth_callback(&hub.url, &key, &provider, state, input)
                .map_err(DomainStateError::bad_request)?;
            return Ok(Handled::Answer(json!({ "status": "ok" })));
        }
        "hubOAuthCancel" => {
            let hub_id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?;
            let state = text_param(params, "state")
                .ok_or_else(|| DomainStateError::bad_request("state is required"))?;
            let (hub, key) = managed_hub(paths, hub_id)?;
            signin::cancel_relay(state);
            client::oauth_cancel(&hub.url, &key, state).map_err(DomainStateError::bad_request)?;
            return Ok(Handled::Answer(json!({ "status": "ok" })));
        }
        "hubAccountRemove" => {
            let (hub_id, auth_index) = parse_hub_account_id(params)?;
            let (hub, key) = managed_hub(paths, &hub_id)?;
            let file_name = find_account_file_name(&hub, &key, &auth_index)?;
            client::delete_account(&hub.url, &key, &file_name)
                .map_err(DomainStateError::bad_request)?;
            refresh(paths, &hub);
            return Ok(Handled::Changed);
        }
        "hubAccountSetEnabled" => {
            let (hub_id, auth_index) = parse_hub_account_id(params)?;
            let enabled_val = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("enabled is required"))?;
            let (hub, key) = managed_hub(paths, &hub_id)?;
            let file_name = find_account_file_name(&hub, &key, &auth_index)?;
            client::set_account_disabled(&hub.url, &key, &file_name, &auth_index, !enabled_val)
                .map_err(DomainStateError::bad_request)?;
            refresh(paths, &hub);
            return Ok(Handled::Changed);
        }
        "hubAccountResetCooldown" => {
            let (hub_id, auth_index) = parse_hub_account_id(params)?;
            let (hub, key) = managed_hub(paths, &hub_id)?;
            ensure_supported_account(&hub, &key, &auth_index)?;
            client::reset_cooldown(&hub.url, &key, &auth_index)
                .map_err(DomainStateError::bad_request)?;
            refresh(paths, &hub);
            return Ok(Handled::Changed);
        }
        "hubSetSessionAffinity" => {
            let hub_id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?;
            let enabled_val = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("enabled is required"))?;
            let (hub, key) = managed_hub(paths, hub_id)?;
            client::set_session_affinity(&hub.url, &key, enabled_val)
                .map_err(DomainStateError::bad_request)?;
            refresh(paths, &hub);
            return Ok(Handled::Changed);
        }
        _ => {}
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
            let new_id = hub_id_for(&url);
            // Check for an existing duplicate before touching the hub's config.
            {
                let _lock = store::lock();
                let hubs = store::read(paths).map_err(io_error)?;
                if hubs
                    .iter()
                    .any(|saved| saved.id == new_id || saved.url == url)
                {
                    return Err(DomainStateError::bad_request(
                        "That hub is already added. Edit it to change its keys or name.",
                    ));
                }
            }
            let client_key = text_param(params, "clientKey").ok_or_else(|| {
                DomainStateError::bad_request(
                    "Enter a client key: one of the keys under api-keys in the hub's config.",
                )
            })?;
            client::check_client_key(&url, client_key).map_err(DomainStateError::bad_request)?;
            let management_key = text_param(params, "managementKey");
            if let Some(key) = management_key {
                check_management_key(&url, key)?;
                // Turn on session affinity by default when connecting a hub so prompt caching stays warm.
                let _ = client::set_session_affinity(&url, key, true);
            }
            let hub = store::Hub {
                id: new_id,
                label: label.unwrap_or_default(),
                url,
                client_key: seal(paths, client_key)?,
                management_key: management_key
                    .map(|key| seal(paths, key))
                    .transpose()?
                    .unwrap_or_default(),
                hidden_in_sidebar: Default::default(),
            };
            let saved = hub.clone();
            let _lock = store::lock();
            let mut hubs = store::read(paths).map_err(io_error)?;
            if hubs
                .iter()
                .any(|saved| saved.id == hub.id || saved.url == hub.url)
            {
                return Err(DomainStateError::bad_request(
                    "That hub is already added. Edit it to change its keys or name.",
                ));
            }
            hubs.push(hub);
            store::write(paths, &hubs).map_err(io_error)?;
            refresh_soon(paths, &saved);
        }
        "hubRemove" => {
            let id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?
                .to_string();
            let _lock = store::lock();
            let mut hubs = store::read(paths).map_err(io_error)?;
            let index = hubs
                .iter()
                .position(|hub| hub.id == id)
                .ok_or_else(|| DomainStateError::not_found("That hub was already removed."))?;
            hubs.remove(index);
            store::write(paths, &hubs).map_err(io_error)?;
            forget_reading(&id);
            usage_reads::forget(&id);
            launch::forget_key(paths.agent_config_home_dir(), &id);
        }
        "hubUpdate" => {
            let id = text_param(params, "hubId")
                .ok_or_else(|| DomainStateError::bad_request("hubId is required"))?
                .to_string();
            let removed = || DomainStateError::not_found("That hub was already removed.");
            // The keys are checked against the hub without holding the hubs lock, which every
            // read of the accounts list waits on; the change is applied to the file as it is then.
            let current = {
                let _lock = store::lock();
                store::read(paths)
                    .map_err(io_error)?
                    .into_iter()
                    .find(|hub| hub.id == id)
                    .ok_or_else(removed)?
            };
            let url = match text_param(params, "url") {
                Some(url) => normalize_url(url)?,
                None => current.url.clone(),
            };
            let client_key = text_param(params, "clientKey").map(str::to_string);
            let management_key = text_param(params, "managementKey").map(str::to_string);
            let clear_management_key =
                params.get("clearManagementKey").and_then(Value::as_bool) == Some(true);
            // CDXC:AgentProviders 2026-11-19 WHY: saved keys are never sent to a new address (security review of the hub work): a hub that moves is given its keys again, so a typo or a look-alike host can't collect them.
            if url != current.url
                && (client_key.is_none()
                    || (!current.management_key.is_empty()
                        && management_key.is_none()
                        && !clear_management_key))
            {
                return Err(DomainStateError::bad_request(
                    if current.management_key.is_empty() {
                        "A new address needs the hub's client key again. Enter it with the address."
                    } else {
                        "A new address needs the hub's keys again. Enter its client key and management key with the address, or remove the management key."
                    },
                ));
            }
            if let Some(key) = &client_key {
                client::check_client_key(&url, key).map_err(DomainStateError::bad_request)?;
            }
            if let Some(key) = &management_key {
                check_management_key(&url, key)?;
                if current.management_key.is_empty() {
                    let _ = client::set_session_affinity(&url, key, true);
                }
            }
            let client_key = client_key.map(|key| seal(paths, &key)).transpose()?;
            let management_key = management_key.map(|key| seal(paths, &key)).transpose()?;
            let _lock = store::lock();
            let mut hubs = store::read(paths).map_err(io_error)?;
            if hubs
                .iter()
                .find(|hub| hub.id == id)
                .ok_or_else(removed)?
                .url
                != current.url
            {
                return Err(DomainStateError::bad_request(
                    "The hub changed while this was being saved. Try again.",
                ));
            }
            // CDXC:AgentProviders 2026-11-19 WHY: a hub's id comes from its address (`hub_id_for`), so a hub that moves takes the id it would get if added at the new address (re-review of the hub work); the old id's readings and profile key go, and its New sessions start on pins follow (accounts/endpoint.rs).
            let moved = (current.url != url).then(|| hub_id_for(&url));
            if let Some(next) = &moved {
                if hubs
                    .iter()
                    .any(|other| other.id == *next || other.url == url)
                {
                    return Err(DomainStateError::bad_request(
                        "Another hub is already added at that address.",
                    ));
                }
            }
            let hub = hubs
                .iter_mut()
                .find(|hub| hub.id == id)
                .ok_or_else(removed)?;
            if let Some(label) = label {
                hub.label = label;
            }
            hub.url = url;
            if let Some(key) = client_key {
                hub.client_key = key;
            }
            if let Some(key) = management_key {
                hub.management_key = key;
            } else if clear_management_key {
                hub.management_key.clear();
            }
            if let Some(next) = &moved {
                hub.id = next.clone();
            }
            let saved = hub.clone();
            store::write(paths, &hubs).map_err(io_error)?;
            if let Some(next) = moved {
                forget_reading(&id);
                usage_reads::forget(&id);
                launch::forget_key(paths.agent_config_home_dir(), &id);
                refresh_soon(paths, &saved);
                return Ok(Handled::Moved { from: id, to: next });
            }
            // The last reading stays on screen until the new one lands, so no meter blinks.
            refresh_soon(paths, &saved);
        }
        _ => unreachable!(),
    }
    Ok(Handled::Changed)
}

/// What [`handle`] did with an accounts operation.
pub(crate) enum Handled {
    /// Not a hub operation.
    NotMine,
    /// A hub operation returned its own answer directly (OAuth start, status, callback, cancel).
    Answer(Value),
    /// A hub was added, changed or removed; the caller answers with the accounts list.
    Changed,
    /// A hub moved to another address and so to another id.
    Moved { from: String, to: String },
}

/// Immediately reads `hub`'s updated account list after a sign-in completes (keeping existing
/// accounts' cached usage so meters don't blink), then refreshes usage in the background.
pub(super) fn refresh_after_sign_in(paths: &GxserverPaths, hub: &store::Hub) {
    if hub.management_key.is_empty() {
        return;
    }
    if let Ok(key) = store::open_key(paths, &hub.management_key) {
        if let Ok(files) = client::auth_files(&hub.url, &key) {
            let now = Utc::now();
            let previous = reading(&hub.id);
            let mut accounts: Vec<AccountReading> = files
                .iter()
                .filter_map(|file| account_from_file(file, now))
                .collect();
            if let Some(prev) = &previous {
                for account in &mut accounts {
                    if let Some(old) = prev
                        .accounts
                        .iter()
                        .find(|a| a.auth_index == account.auth_index)
                    {
                        account.usage = old.usage.clone();
                        account.usage_error = old.usage_error.clone();
                    }
                }
            }
            readings().lock().unwrap_or_else(|e| e.into_inner()).insert(
                hub.id.clone(),
                Reading {
                    error: None,
                    accounts,
                    updated_at: Some(Utc::now().to_rfc3339()),
                    fetched_at: Instant::now(),
                    session_affinity: previous.as_ref().and_then(|p| p.session_affinity),
                    routing_strategy: previous.and_then(|p| p.routing_strategy),
                },
            );
        }
    }
    refresh_soon(paths, hub);
}

/// Reads `hub` again in the background, keeping its last reading meanwhile.
fn refresh_soon(paths: &GxserverPaths, hub: &store::Hub) {
    if hub.management_key.is_empty() {
        return;
    }
    let (paths, hub) = (paths.clone(), hub.clone());
    std::thread::spawn(move || refresh(&paths, &hub));
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

    /// CDXC:AgentProviders 2026-11-27: Antigravity offers other apps only ACP, so no usage reader
    /// ever runs for an Antigravity account: a hub's Antigravity login is not listed and its usage
    /// is never asked for (a local listener standing in for the hub sees no connection), and the
    /// usage-only Antigravity entry has no reader at all.
    #[test]
    fn antigravity_accounts_never_reach_a_network_usage_reader() {
        let hub = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        hub.set_nonblocking(true).expect("nonblocking");
        let url = format!("http://{}", hub.local_addr().expect("address"));
        for provider in ["antigravity", "Antigravity", "gemini"] {
            let file = json!({
                "auth_index": "ag-1",
                "provider": provider,
                "email": "someone@example.com",
                "name": "antigravity-someone.json",
            });
            assert!(account_from_file(&file, Utc::now()).is_none());
            assert!(matches!(client::usage(&url, "key", &file), Ok(None)));
        }
        assert!(
            matches!(hub.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "an Antigravity usage read reached the hub"
        );
        use crate::accounts::usage_only::{reader, UsageProvider};
        assert!(reader(UsageProvider::Antigravity).is_none());
        assert!(reader(UsageProvider::Gemini).is_none());
    }
}
