//! When each hub account's usage is read through the hub, and the last reading of each.
//!
//! CDXC:AgentProviders 2026-11-19 WHY:
//! A hub's usage reads go to Anthropic's and ChatGPT's usage endpoints with the account's own token, the same endpoints Claude Code, Codex and cswap poll and that answer 429 when read too often. Like Claude reset status (claude_resets.rs), each account is read at most every 15 minutes (30 after a 429), one account after another rather than all at once, and not at all while the user hid it from the sidebar or while it is a saved Claude Swap or Codex Swap login here, whose own reading it shows instead. The hub's account list itself (`auth-files`, the hub's own state) is still read every two minutes.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{AccountReading, client, store};
use crate::accounts::model::UsageWindow;
use crate::paths::GxserverPaths;

const EVERY: Duration = Duration::from_secs(15 * 60);
const AFTER_RATE_LIMIT: Duration = Duration::from_secs(30 * 60);
/// The pause between two accounts' reads.
const GAP: Duration = Duration::from_secs(2);
const HIDDEN: &str =
    "Ghostex doesn't read this account's usage while it's hidden from the sidebar.";

#[derive(Clone)]
struct Read {
    usage: Vec<UsageWindow>,
    error: Option<String>,
    at: Instant,
    rate_limited: bool,
}

fn reads() -> &'static Mutex<HashMap<String, Read>> {
    static READS: OnceLock<Mutex<HashMap<String, Read>>> = OnceLock::new();
    READS.get_or_init(Default::default)
}

fn key(hub: &store::Hub, account: &AccountReading) -> String {
    format!("{}:{}", hub.id, account.auth_index)
}

/// Forgets a removed hub's readings.
pub(super) fn forget(hub_id: &str) {
    let prefix = format!("{hub_id}:");
    reads()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|key, _| !key.starts_with(&prefix));
}

/// The saved Swap login of this computer `account` is, by provider, email and Codex workspace:
/// its usage windows. `None` when it is none of them, or the user turned it off.
fn swap_twin(paths: &GxserverPaths, account: &AccountReading) -> Option<Vec<UsageWindow>> {
    if !account.name.contains('@') {
        return None;
    }
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let registry = crate::accounts::store::read(&db).ok()?;
    let snapshot = crate::accounts::runtime::current_snapshot();
    snapshot
        .accounts
        .iter()
        .filter(|found| found.provider.id() == account.provider)
        // Only a signed-in Swap login's reading stands in for the hub's.
        .filter(|found| found.status == "ready" && found.usage_error.is_none())
        .filter(|found| found.email.eq_ignore_ascii_case(&account.name))
        .filter(|found| {
            account.workspace.as_deref().is_none_or(|workspace| {
                crate::accounts::codex_identity::codex_account_part(&found.identity) == workspace
                    || found.provider.id() != "codex"
            })
        })
        .find(|found| {
            let id = crate::accounts::endpoint::account_id(found);
            registry
                .accounts
                .iter()
                .any(|saved| saved.id == id && !saved.user_disabled)
        })
        .map(|found| found.usage.clone())
}

/// Fills each account's usage from its last reading, a Swap twin's reading or a new read when one
/// is due, reading the due ones one after another. `publish` is called after each new read, so the
/// readings already taken show while later ones wait.
pub(super) fn fill(
    paths: &GxserverPaths,
    hub: &store::Hub,
    key_text: &str,
    accounts: &mut [AccountReading],
    files: &[Value],
    mut publish: impl FnMut(&[AccountReading]),
) {
    let mut due = Vec::new();
    for (index, account) in accounts.iter_mut().enumerate() {
        if account.status == "disabled" {
            continue;
        }
        if hub.hidden_in_sidebar.contains(&account.auth_index) {
            account.usage_error = Some(HIDDEN.to_string());
            continue;
        }
        if let Some(usage) = swap_twin(paths, account) {
            account.usage = usage;
            continue;
        }
        let last = reads()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key(hub, account))
            .cloned();
        if let Some(last) = &last {
            account.usage = last.usage.clone();
            account.usage_error = last.error.clone();
        }
        let wait = if last.as_ref().is_some_and(|l| l.rate_limited) {
            AFTER_RATE_LIMIT
        } else {
            EVERY
        };
        if last.is_none_or(|l| l.at.elapsed() >= wait) {
            due.push(index);
        }
    }
    for (turn, index) in due.into_iter().enumerate() {
        if turn > 0 {
            std::thread::sleep(GAP);
        }
        let account = &mut accounts[index];
        let Some(file) = files.iter().find(|file| {
            file.get("auth_index").and_then(Value::as_str) == Some(&account.auth_index)
        }) else {
            continue;
        };
        let read = match client::usage(&hub.url, key_text, file) {
            Ok(usage) => Read {
                usage: usage.unwrap_or_default(),
                error: None,
                at: Instant::now(),
                rate_limited: false,
            },
            // A failed read keeps the last meters, marked by its error.
            Err(error) => Read {
                usage: account.usage.clone(),
                error: Some(error.message),
                at: Instant::now(),
                rate_limited: error.rate_limited,
            },
        };
        account.usage = read.usage.clone();
        account.usage_error = read.error.clone();
        reads()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key(hub, account), read);
        publish(accounts);
    }
}
