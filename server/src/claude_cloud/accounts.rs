//! What a cloud session needs to know about its Claude account: the name and email its claude.ai
//! page must be signed in as, and the usage-limit notice.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::accounts::model::Provider;

/// `accountName` and `accountEmail` of a saved Claude account (the email Settings > Accounts
/// shows, from the last usage reading). Empty when the account is not saved.
pub(crate) fn account_facts(db: &Connection, account_id: &str) -> Map<String, Value> {
    let mut facts = Map::new();
    let Ok(registry) = crate::accounts::store::read(db) else {
        return facts;
    };
    let Some(account) = registry
        .accounts
        .iter()
        .find(|account| account.id == account_id && account.provider == Provider::Claude)
    else {
        return facts;
    };
    facts.insert("accountName".to_string(), json!(account.name));
    let email = crate::accounts::runtime::current_snapshot()
        .accounts
        .iter()
        .find(|found| {
            found.provider == Provider::Claude
                && (found.identity == account.identity || found.selector == account.selector)
        })
        .map(|found| found.email.trim().to_lowercase())
        .filter(|email| email.contains('@'))
        .or_else(|| {
            let identity = account.identity.trim().to_lowercase();
            identity.contains('@').then_some(identity)
        });
    if let Some(email) = email {
        facts.insert("accountEmail".to_string(), json!(email));
    }
    facts
}

/// "<account> hit its 5h limit · resumes 4:10 pm": the account's usage window that is full, with
/// its reset time, or a plain sentence when Ghostex has no reading of it.
///
/// CDXC:ClaudeCloud 2026-10-23 DECISION:
/// User picked "notice-and-copy" for a cloud session whose account is out of usage: a notice with the reset time ("<account> hit its 5h limit · resumes 4:10 pm") plus "Start a copy on another account", a new cloud session on the same branch with a summary, on the account the user chooses; the original stays where it is. Usage windows are labelled 5h and 7d.
pub(crate) fn limit_notice(db: &Connection, account_id: Option<&str>) -> String {
    let saved = account_id.and_then(|account_id| {
        crate::accounts::store::read(db)
            .ok()?
            .accounts
            .into_iter()
            .find(|account| account.id == account_id && account.provider == Provider::Claude)
    });
    let name = saved
        .as_ref()
        .map(|account| account.name.clone())
        .unwrap_or_else(|| "This account".to_string());
    let window = saved.as_ref().and_then(|account| {
        crate::accounts::runtime::current_snapshot()
            .accounts
            .into_iter()
            .find(|found| {
                found.provider == Provider::Claude
                    && (found.identity == account.identity || found.selector == account.selector)
            })?
            .usage
            .into_iter()
            .filter(|window| window.used_percent >= 100.0 && window.model.is_none())
            // A full 7d window outlasts a full 5h one, so it is the one to name.
            .max_by_key(|window| window.label == "7d")
    });
    match window {
        Some(window) => match window
            .resets_at
            .as_deref()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        {
            Some(at) => format!(
                "{name} hit its {} limit · resumes {}",
                window.label,
                at.with_timezone(&chrono::Local).format("%-I:%M %P")
            ),
            None => format!("{name} hit its {} limit", window.label),
        },
        None => format!("{name} hit its usage limit"),
    }
}
