use super::{helpers, model::*, store};
use crate::{
    agents::{command_word, quote_shell_arg, reusable_account_command},
    domain::{DomainRepository, DomainStateError},
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// CDXC:AgentProviders 2026-09-07 DECISION:
/// Picking an account for a custom agent keeps its model, instructions and other command arguments. Replace only the provider invocation, retaining the original command for later account switches and resumes.
pub(crate) fn with_account_command(
    base: &str,
    provider: Provider,
    wrapper: &str,
) -> Result<String, DomainStateError> {
    let invalid = || {
        DomainStateError::bad_request(format!(
        "Account selection needs a {} command (or {} run). Update this custom agent's command while keeping its arguments.",
        provider.id(), provider.helper()
    ))
    };
    let mut offset = 0;
    while let Some((start, end, word)) = command_word(base, offset) {
        offset = end;
        // Environment assignments and the standard invocation prefixes keep their original shell spelling.
        if word.contains('=') || matches!(word.as_str(), "env" | "exec" | "command") {
            continue;
        }
        let executable = Path::new(&word)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if executable == provider.id() {
            return Ok(format!("{}{wrapper}{}", &base[..start], &base[end..]));
        }
        if executable == provider.helper() {
            let (_, run_end, run) = command_word(base, offset).ok_or_else(invalid)?;
            if run != "run" {
                return Err(invalid());
            }
            let (_, slot_end, _) = command_word(base, run_end).ok_or_else(invalid)?;
            offset = slot_end;
            while let Some((_, end, word)) = command_word(base, offset) {
                if word == "--" {
                    return Ok(format!("{}{wrapper}{}", &base[..start], &base[end..]));
                }
                if !matches!(
                    word.as_str(),
                    "--share-history" | "--no-share" | "--require-session"
                ) {
                    return Ok(format!("{}{wrapper}{}", &base[..start], &base[offset..]));
                }
                offset = end;
            }
            return Ok(format!("{}{wrapper}", &base[..start]));
        }
        return Err(invalid());
    }
    Err(invalid())
}

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
pub(crate) fn assign(
    runtime: &mut Map<String, Value>,
    account: &SavedAccount,
    command: String,
) -> Result<String, DomainStateError> {
    let base = runtime
        .get("accountBaseCommand")
        .or_else(|| runtime.get("agentCommand"))
        .and_then(Value::as_str)
        .unwrap_or(account.provider.id());
    let base = reusable_account_command(base, account.provider.id())?;
    let command = with_account_command(&base, account.provider, &command)?;
    runtime.insert("accountBaseCommand".into(), json!(base));
    for (k, v) in [
        ("accountId", json!(account.id)),
        ("accountProvider", json!(account.provider)),
        ("accountName", json!(account.name)),
        ("accountSlot", json!(account.selector)),
        ("accountCommand", json!(command)),
        ("agentCommand", json!(command)),
    ] {
        runtime.insert(k.into(), v);
    }
    Ok(command)
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
    // CDXC:AgentProviders 2026-09-11 DECISION: User: use the current CLI login until an account is added to Ghostex for that provider (2026-09-09); once accounts exist, a launch without an explicit account uses the provider's Account for new sessions rule from Settings (Most limit remaining by default, see default_account.rs), which supersedes the lowest-slot choice. When that rule yields no account the launch keeps the current CLI login, so a normal CLI launch needs no account switcher and starts with automatic continuation off.
    let snapshot = super::runtime::current_snapshot();
    let id = runtime
        .get("accountId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            super::default_account::quick_launch_account(&registry, &snapshot, provider)
                .map(|a| a.id.clone())
        });
    let Some(id) = id else {
        runtime.insert("accountPolicyDefault".into(), json!(Policy::default()));
        return Ok(None);
    };
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id && a.provider == provider)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The selected account is no longer registered. Choose another account.",
            )
        })?;
    runtime
        .entry("accountPolicyDefault")
        .or_insert(json!(registry
            .defaults
            .get(&provider)
            .cloned()
            .unwrap_or_default()));
    let home = home()?;
    let cmd = command(&home, account)?;
    let assigned = assign(runtime, account, cmd)?;
    super::default_account::record_last_used(db, &registry, provider, &account.id)?;
    Ok(Some(assigned))
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
