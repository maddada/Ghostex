use super::{helpers, model::*, store};
use crate::{
    agents::quote_shell_arg,
    domain::{DomainRepository, DomainStateError},
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
pub(crate) fn provider(project: &Value, session: &Value) -> Option<Provider> {
    match crate::agents::session_agent_family_id(project, session).as_deref() {
        Some("claude") => Some(Provider::Claude),
        Some("codex") => Some(Provider::Codex),
        _ => None,
    }
}
pub(crate) fn command(home: &Path, account: &SavedAccount) -> Result<String, DomainStateError> {
    validate_identity(home, account)?;
    let executable = helpers::executable(home, account.provider.helper()).ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Install {} on this computer first.",
            account.provider.helper()
        ))
    })?;
    Ok(format!(
        "{} run {} --share-history --",
        quote_shell_arg(&executable.to_string_lossy()),
        quote_shell_arg(&account.selector)
    ))
}
pub(crate) fn assign(runtime: &mut Map<String, Value>, account: &SavedAccount, command: String) {
    for (k, v) in [
        ("accountId", json!(account.id)),
        ("accountProvider", json!(account.provider)),
        ("accountName", json!(account.name)),
        ("accountColor", json!(account.color)),
        ("accountCommand", json!(command)),
        ("agentCommand", json!(command)),
    ] {
        runtime.insert(k.into(), v);
    }
}
pub(crate) fn apply_new_session(
    db: &Connection,
    agent_id: &str,
    icon: Option<&str>,
    runtime: &mut Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    let provider = match icon.unwrap_or(agent_id) {
        "claude" => Provider::Claude,
        "codex" => Provider::Codex,
        _ => return Ok(None),
    };
    let registry = store::read(db)?;
    runtime
        .entry("accountPolicyDefault")
        .or_insert(json!(registry
            .defaults
            .get(&provider)
            .cloned()
            .unwrap_or_default()));
    if runtime.get("accountId").is_some_and(Value::is_null) {
        return Ok(Some(provider.id().into()));
    }
    let id = runtime
        .get("accountId")
        .and_then(Value::as_str)
        .or_else(|| registry.default_accounts.get(&provider).map(String::as_str));
    let Some(id) = id else { return Ok(None) };
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id && a.provider == provider)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The selected account is no longer registered. Choose another account.",
            )
        })?;
    let home = home()?;
    let cmd = command(&home, account)?;
    assign(runtime, account, cmd.clone());
    Ok(Some(cmd))
}
pub(crate) fn home() -> Result<PathBuf, DomainStateError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| DomainStateError::bad_request("The server's home directory is unavailable."))
}
pub(crate) fn validate_session(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    let Some(id) = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    let registry = store::read(repository.db)?;
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "This account was removed from Ghostex. Select an account before resuming.",
            )
        })?;
    validate_identity(&home()?, account)
}
pub(crate) fn validate_identity(
    home: &Path,
    account: &SavedAccount,
) -> Result<(), DomainStateError> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let path = match account.provider {
        Provider::Codex => std::env::var_os("XSWAP_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_home.join("codex-swap"))
            .join("accounts.json"),
        Provider::Claude => {
            if cfg!(target_os = "linux") {
                data_home.join("claude-swap/sequence.json")
            } else {
                home.join(".claude-swap-backup/sequence.json")
            }
        }
    };
    let fail = || {
        DomainStateError::bad_request("The saved account changed or is unavailable. Refresh Accounts and reconnect it before resuming.")
    };
    let raw = std::fs::read(path).map_err(|_| fail())?;
    let data: Value = serde_json::from_slice(&raw).map_err(|_| fail())?;
    let identity = if account.provider == Provider::Claude {
        let row = &data["accounts"][&account.selector];
        format!(
            "{}:{}",
            row["email"].as_str().unwrap_or("").to_lowercase(),
            row["organizationUuid"].as_str().unwrap_or("")
        )
    } else {
        let row = data["accounts"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|r| {
                    r["number"].as_u64().map(|n| n.to_string()).as_deref()
                        == Some(&account.selector)
                })
            })
            .ok_or_else(fail)?;
        row.pointer("/identity/accountId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    if identity != account.identity || identity.is_empty() {
        return Err(fail());
    }
    Ok(())
}
/// CDXC:AgentProviders 2026-09-05 DECISION:
/// Continuation defaults apply to new sessions. Existing sessions retain their saved policy; sessions created before account management stay off until explicitly configured.
pub(crate) fn effective_policy(
    _registry: &Registry,
    _provider: Provider,
    session: &Value,
) -> Policy {
    session
        .pointer("/runtimeSettings/accountPolicyOverride")
        .filter(|v| !v.is_null())
        .or_else(|| session.pointer("/runtimeSettings/accountPolicyDefault"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
