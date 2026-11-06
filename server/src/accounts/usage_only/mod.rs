//! Usage-only accounts: Cursor, Gemini and Copilot logins Ghostex signs in to itself, only to show
//! their usage beside the Claude and Codex accounts (Settings > Accounts, the sidebar meters, the
//! phone).
//!
//! CDXC:AgentProviders 2026-11-05 DECISION:
//! User picked "Usage only for now" for Cursor (Q1336) and asked: "lets do gemini and copilot usage also ... just to show usage". These accounts never run, switch or pick a session, so they are kept apart from the Claude and Codex registry (`model::Provider`, which every launch and switch rule reads) and answered under their own `usageAccounts` key, which the launcher, the switch menus and the new-thread picker never read.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/accounts/ (Settings), apps/desktop/src/app/titlebar/account_usage.rs (the sidebar meters), apps/mobile/app/src/chat/native/composer/AccountsPanel.tsx (the phone).

mod copilot;
mod cursor;
mod gemini;
mod http;
mod signin;
mod store;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::model::UsageWindow;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub(crate) enum UsageProvider {
    Cursor,
    Gemini,
    Copilot,
}

impl UsageProvider {
    fn id(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Gemini => "gemini",
            Self::Copilot => "copilot",
        }
    }
    fn parse(text: &str) -> Option<Self> {
        match text {
            "cursor" => Some(Self::Cursor),
            "gemini" => Some(Self::Gemini),
            "copilot" => Some(Self::Copilot),
            _ => None,
        }
    }
}

fn account_id(provider: UsageProvider, identity: &str) -> String {
    format!(
        "{}-{}",
        provider.id(),
        hex::encode(Sha256::digest(identity.as_bytes()))
    )
}

/// Whether an account id is one of these accounts' (they are named after their provider).
pub(crate) fn owns_id(id: &str) -> bool {
    ["cursor-", "gemini-", "copilot-"]
        .iter()
        .any(|prefix| id.starts_with(prefix))
}

/// One labelled value under an account's usage bars ("Extra usage · $4.20 of $20.00").
#[derive(Clone, Serialize)]
pub(crate) struct Detail {
    label: String,
    value: String,
}

impl Detail {
    fn new(label: &str, value: String) -> Self {
        Self {
            label: label.to_string(),
            value,
        }
    }
}

/// A usage read's result.
pub(crate) struct Reading {
    usage: Vec<UsageWindow>,
    plan: Option<String>,
    details: Vec<Detail>,
}

/// The last reading of an account, or why there is none.
#[derive(Clone)]
struct Cached {
    status: &'static str,
    usage: Vec<UsageWindow>,
    plan: Option<String>,
    details: Vec<Detail>,
    error: Option<String>,
    /// When `usage` was read (it stays shown, marked stale, after a failed read).
    updated_at: Option<String>,
    fetched_at: Instant,
}

fn readings() -> &'static Mutex<HashMap<String, Cached>> {
    static READINGS: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
    READINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn reading(id: &str) -> Option<Cached> {
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

/// The accounts being read right now, so a slow provider is never read twice at once.
fn reading_now() -> &'static Mutex<std::collections::HashSet<String>> {
    static NOW: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    NOW.get_or_init(Default::default)
}

/// A reading is refreshed after two minutes, like the Claude and Codex usage.
const FRESH_FOR: Duration = Duration::from_secs(120);

/// Reads one account's usage, saves renewed tokens and keeps the result.
fn refresh(paths: &GxserverPaths, account: &store::UsageAccount) {
    if !reading_now()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(account.id.clone())
    {
        return;
    }
    let result = match account.provider {
        UsageProvider::Cursor => cursor::read(account),
        UsageProvider::Gemini => gemini::read(account),
        UsageProvider::Copilot => copilot::read(account),
    };
    let previous = reading(&account.id);
    let cached = match result {
        Ok((reading, renewed)) => {
            if let Some(credential) = renewed {
                let _ = store::update(paths, &account.id, |saved| saved.credential = credential);
            }
            Cached {
                status: "ready",
                usage: reading.usage,
                plan: reading.plan,
                details: reading.details,
                error: None,
                updated_at: Some(chrono::Utc::now().to_rfc3339()),
                fetched_at: Instant::now(),
            }
        }
        Err(failure) => {
            let expired = matches!(failure, http::Failure::SignInExpired);
            let previous = previous.filter(|_| !expired);
            Cached {
                status: if expired {
                    "loginRequired"
                } else {
                    "unavailable"
                },
                usage: previous
                    .as_ref()
                    .map(|p| p.usage.clone())
                    .unwrap_or_default(),
                plan: previous.as_ref().and_then(|p| p.plan.clone()),
                details: previous
                    .as_ref()
                    .map(|p| p.details.clone())
                    .unwrap_or_default(),
                error: Some(failure.message()),
                updated_at: previous.and_then(|p| p.updated_at),
                fetched_at: Instant::now(),
            }
        }
    };
    readings()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(account.id.clone(), cached);
    reading_now()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&account.id);
}

/// How a read waits for usage.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Answer from what is kept, reading stale accounts in the background.
    Never,
    /// Read accounts never read yet before answering (Settings opening).
    FirstRead,
    /// Read every account again before answering (Settings' Refresh).
    Always,
}

/// The rows for `usageAccounts`. `sidebar_only` keeps the accounts shown in the sidebar.
pub(crate) fn rows(paths: &GxserverPaths, sidebar_only: bool, wait: Wait) -> Vec<Value> {
    let accounts = {
        let _lock = store::lock();
        store::read(paths).unwrap_or_default()
    };
    let active: Vec<_> = accounts
        .iter()
        .filter(|account| !account.user_disabled && (!sidebar_only || account.show_in_titlebar))
        .cloned()
        .collect();
    let mut now = Vec::new();
    let mut later = Vec::new();
    for account in active {
        match reading(&account.id) {
            None if wait != Wait::Never => now.push(account),
            Some(_) if wait == Wait::Always => now.push(account),
            Some(cached) if cached.fetched_at.elapsed() < FRESH_FOR => {}
            _ => later.push(account),
        }
    }
    if !now.is_empty() {
        let threads: Vec<_> = now
            .into_iter()
            .map(|account| {
                let paths = paths.clone();
                std::thread::spawn(move || refresh(&paths, &account))
            })
            .collect();
        for thread in threads {
            let _ = thread.join();
        }
    }
    for account in later {
        let paths = paths.clone();
        std::thread::spawn(move || refresh(&paths, &account));
    }
    accounts
        .iter()
        .filter(|account| !sidebar_only || (account.show_in_titlebar && !account.user_disabled))
        .map(|account| {
            let cached = (!account.user_disabled)
                .then(|| reading(&account.id))
                .flatten();
            json!({
                "id": account.id,
                "provider": account.provider,
                "usageOnly": true,
                "registered": true,
                "name": account.name,
                "email": account.email,
                "showInTitlebar": account.show_in_titlebar && !account.user_disabled,
                "userDisabled": account.user_disabled,
                "status": if account.user_disabled { "disabled" } else { cached.as_ref().map_or("loading", |c| c.status) },
                "usage": cached.as_ref().map(|c| c.usage.clone()).unwrap_or_default(),
                "plan": cached.as_ref().and_then(|c| c.plan.clone()),
                "details": cached.as_ref().map(|c| c.details.clone()).unwrap_or_default(),
                "usageError": cached.as_ref().and_then(|c| c.error.clone()),
                "usageUpdatedAt": cached.as_ref().and_then(|c| c.updated_at.clone()),
                "usageStale": cached.as_ref().is_some_and(|c| c.status != "ready" && !c.usage.is_empty()),
            })
        })
        .collect()
}

/// What an accounts operation meant for these accounts.
pub(crate) enum Handled {
    /// Not about a usage-only account.
    NotMine,
    /// The answer to send as it is (a sign-in's job).
    Answer(Value),
    /// An account changed: answer with the accounts list.
    Changed,
}

fn field<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && text.len() < 4096)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required")))
}

fn io_error(error: std::io::Error) -> DomainStateError {
    super::store::error(error)
}

/// The sign-in operations, and the account edits Settings sends for a usage-only account's id.
pub(crate) fn handle(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Handled, DomainStateError> {
    let operation = field(params, "operation")?;
    if operation == "usageSignInStart" {
        let provider = UsageProvider::parse(field(params, "provider")?)
            .ok_or_else(|| DomainStateError::bad_request("Unknown provider."))?;
        let again = params
            .get("accountId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let job = signin::start(paths, provider, field(params, "owner")?, again)?;
        return Ok(Handled::Answer(json!({ "usageSignIn": job })));
    }
    if operation.starts_with("usageSignIn") {
        let job = signin::operate(
            operation,
            field(params, "jobId")?,
            field(params, "owner")?,
            params.get("input").and_then(Value::as_str),
        )?;
        return Ok(Handled::Answer(json!({ "usageSignIn": job })));
    }
    let Some(id) = params
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| owns_id(id))
    else {
        return Ok(Handled::NotMine);
    };
    let found = match operation {
        "setTitlebar" => {
            let shown = params.get("shown").and_then(Value::as_bool).unwrap_or(true);
            store::update(paths, id, |account| account.show_in_titlebar = shown)
        }
        "setEnabled" => {
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            forget_reading(id);
            store::update(paths, id, |account| account.user_disabled = !enabled)
        }
        "update" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty() && name.len() < 120)
                .map(str::to_string);
            store::update(paths, id, |account| {
                if let Some(name) = name {
                    account.name = name;
                }
            })
        }
        "remove" => {
            let _lock = store::lock();
            let mut accounts = store::read(paths).map_err(io_error)?;
            let before = accounts.len();
            accounts.retain(|account| account.id != id);
            store::write(paths, &accounts).map_err(io_error)?;
            forget_reading(id);
            Ok(accounts.len() != before)
        }
        _ => {
            return Err(DomainStateError::bad_request(
                "That can't be done to a usage-only account.",
            ))
        }
    }
    .map_err(io_error)?;
    if !found {
        return Err(DomainStateError::not_found("Account not found."));
    }
    Ok(Handled::Changed)
}
