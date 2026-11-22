//! Logins and ticked secrets on a box (plan §2.5): home (or the phone) writes `secrets.json`
//! into the private folder; box mode makes one account per agent the active one and hands the
//! sessions what they need.
//!
//! - Claude: the account's one-year box token, as `CLAUDE_CODE_OAUTH_TOKEN` in every session's
//!   environment (D5).
//! - Codex: a copy of the account's `auth.json` in the private folder, with `~/.codex/auth.json`
//!   a link to it; a refresh Codex writes there goes home through `write-back` (D6).
//! - Ticked variables: set in every session's environment.

use std::path::{Path, PathBuf};

use ghostex_gx_protocol::cloud_box::{
    box_files, BoxInjectedAccount, BoxSecretsFile, BoxWriteBackItem, BoxWriteBackResult,
    CloudProviderId,
};
use serde::{Deserialize, Serialize};

use super::state;

/// [`BoxInjectedAccount::kind`] values.
pub(crate) const CLAUDE_BOX_TOKEN: &str = "claudeBoxToken";
pub(crate) const CODEX_AUTH_JSON: &str = "codexAuthJson";

const ACTIVE_FILE: &str = "active.json";
const ACCOUNTS_DIR: &str = "accounts";
/// The SHA-256 of the `auth.json` home last sent or saved, beside the account's copy.
const INJECTED_SHA_FILE: &str = "injected.sha256";

/// The account each agent runs on, by [`BoxInjectedAccount::account_id`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveAccounts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claude: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex: Option<String>,
}

pub(crate) fn read_secrets(provider: &CloudProviderId) -> BoxSecretsFile {
    state::read_json(&state::private_dir(provider).join(box_files::SECRETS)).unwrap_or_default()
}

/// On disk, not in the private folder: it holds only account ids, and it must outlive a restart
/// (which empties the private folder) so the box knows which login to put back when home sends
/// `secrets.json` again.
fn active_file(_provider: &CloudProviderId) -> PathBuf {
    state::state_dir().join(ACTIVE_FILE)
}

fn account_dir(provider: &CloudProviderId, account_id: &str) -> PathBuf {
    state::private_dir(provider)
        .join(ACCOUNTS_DIR)
        .join(safe_name(account_id))
}

fn safe_name(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// `$CODEX_HOME/auth.json` (default `~/.codex/auth.json`), where Codex reads its login.
pub(crate) fn codex_auth_path() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/root"))
                .join(".codex")
        })
        .join("auth.json")
}

/// Makes `account_id` (or, without one, the first injected account for `agent`) the account
/// the box's `agent` sessions run on. Returns the account used, `None` when home sent none for
/// that agent (the agent then uses whatever login the snapshot has).
pub(crate) fn activate(
    provider: &CloudProviderId,
    agent: &str,
    account_id: Option<&str>,
) -> Result<Option<BoxInjectedAccount>, String> {
    let secrets = read_secrets(provider);
    let account = match account_id {
        Some(id) => Some(
            secrets
                .accounts
                .iter()
                .find(|account| account.account_id == id)
                .cloned()
                .ok_or_else(|| {
                    format!("Home sent no login for account {id}; allow it on cloud boxes first.")
                })?,
        ),
        None => secrets
            .accounts
            .iter()
            .find(|account| account.agent == agent)
            .cloned(),
    };
    let Some(account) = account else {
        return Ok(None);
    };
    let mut active: ActiveAccounts = state::read_json(&active_file(provider)).unwrap_or_default();
    match account.kind.as_str() {
        CLAUDE_BOX_TOKEN => active.claude = Some(account.account_id.clone()),
        CODEX_AUTH_JSON => {
            place_codex_login(provider, &account)?;
            active.codex = Some(account.account_id.clone());
        }
        other => return Err(format!("Unknown login kind \"{other}\".")),
    }
    state::write_json(&active_file(provider), &active)?;
    Ok(Some(account))
}

/// Puts the account's `auth.json` in the private folder and links Codex's login file to it. A
/// copy Codex refreshed on the box is kept unless home has sent a different file since (it saved
/// the refresh, or refreshed on the computer).
fn place_codex_login(
    provider: &CloudProviderId,
    account: &BoxInjectedAccount,
) -> Result<(), String> {
    let dir = account_dir(provider, &account.account_id);
    let copy = dir.join("auth.json");
    let sent_sha = sha256_hex(account.value.as_bytes());
    let recorded_sha = std::fs::read_to_string(dir.join(INJECTED_SHA_FILE)).ok();
    if !copy.is_file() || recorded_sha.as_deref().map(str::trim) != Some(sent_sha.as_str()) {
        state::write_private(&copy, account.value.as_bytes())?;
        state::write_private(&dir.join(INJECTED_SHA_FILE), sent_sha.as_bytes())?;
    }
    link_codex_login(&copy)
}

fn link_codex_login(target: &Path) -> Result<(), String> {
    let link = codex_auth_path();
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Can't make {}: {error}", parent.display()))?;
    }
    if std::fs::read_link(&link).ok().as_deref() == Some(target) {
        return Ok(());
    }
    // A real file here is a login the snapshot carried or Codex rewrote in place of the link.
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, &link)
            .map_err(|error| format!("Can't link {}: {error}", link.display()))
    }
    #[cfg(not(unix))]
    {
        let _ = target;
        Err("Cloud Boxes run Linux.".to_string())
    }
}

/// Removes Codex's login file (the link, or a file Codex wrote in its place after copying that
/// one into the private folder) before a snapshot. Returns the path removed.
pub(crate) fn remove_codex_login(provider: &CloudProviderId) -> Option<String> {
    let link = codex_auth_path();
    let metadata = std::fs::symlink_metadata(&link).ok()?;
    let active: ActiveAccounts = state::read_json(&active_file(provider)).unwrap_or_default();
    let account_id = active.codex?;
    if metadata.file_type().is_file() {
        // Codex replaced the link with a file: that file is the newest login.
        let copy = account_dir(provider, &account_id).join("auth.json");
        if let Ok(bytes) = std::fs::read(&link) {
            let _ = state::write_private(&copy, &bytes);
        }
    }
    std::fs::remove_file(&link).ok()?;
    Some(link.to_string_lossy().into_owned())
}

/// After a restart emptied the private folder (a `local` or Boat resume), puts the active Codex
/// account's login back once home has written `secrets.json` again. Cheap when nothing is
/// missing; run by the idle watcher and before each session starts.
pub(crate) fn restore_after_restart() {
    let Some(ticket) = state::read_ticket() else {
        return;
    };
    let provider = &ticket.provider;
    let active: ActiveAccounts = state::read_json(&active_file(provider)).unwrap_or_default();
    let Some(account_id) = active.codex else {
        return;
    };
    if account_dir(provider, &account_id)
        .join("auth.json")
        .is_file()
    {
        return;
    }
    if let Some(account) = read_secrets(provider)
        .accounts
        .into_iter()
        .find(|account| account.account_id == account_id && account.kind == CODEX_AUTH_JSON)
    {
        let _ = place_codex_login(provider, &account);
    }
}

/// Links Codex's login again after a snapshot.
pub(crate) fn restore_codex_login(provider: &CloudProviderId) -> Option<String> {
    let active: ActiveAccounts = state::read_json(&active_file(provider)).unwrap_or_default();
    let copy = account_dir(provider, &active.codex?).join("auth.json");
    if !copy.is_file() {
        return None;
    }
    link_codex_login(&copy).ok()?;
    Some(codex_auth_path().to_string_lossy().into_owned())
}

/// Claude Code asks for a theme and whether to trust a folder the first time it runs; a box
/// session has nobody at its terminal for that, so both are answered in `~/.claude.json` (the
/// user picked this project for the box). Other keys in the file are kept.
pub(crate) fn prepare_claude_folder(cwd: &Path) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"));
    let path = home.join(".claude.json");
    let mut config: serde_json::Value =
        state::read_json(&path).unwrap_or_else(|| serde_json::json!({}));
    let Some(object) = config.as_object_mut() else {
        return Ok(());
    };
    object.insert("hasCompletedOnboarding".into(), serde_json::json!(true));
    let projects = object
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(projects) = projects.as_object_mut() {
        let entry = projects
            .entry(cwd.to_string_lossy().into_owned())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(entry) = entry.as_object_mut() {
            entry.insert("hasTrustDialogAccepted".into(), serde_json::json!(true));
        }
    }
    let text = serde_json::to_vec_pretty(&config).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| format!("Can't write {}: {error}", path.display()))
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// What every session started on the box gets in its environment: the active Claude account's
/// box token and the ticked variables. Empty on a machine that is not a box. Read on each
/// session start, so a later `secrets.json` (another session's prepare) applies without a
/// gxserver restart.
pub(crate) fn session_environment() -> Vec<(String, String)> {
    let Some(ticket) = state::read_ticket() else {
        return Vec::new();
    };
    restore_after_restart();
    let provider = &ticket.provider;
    let secrets = read_secrets(provider);
    let mut environment: Vec<(String, String)> = secrets
        .vars
        .iter()
        .filter(|var| valid_env_name(&var.name))
        .map(|var| (var.name.clone(), var.value.clone()))
        .collect();
    let active: ActiveAccounts = state::read_json(&active_file(provider)).unwrap_or_default();
    if let Some(account) = active.claude.and_then(|id| {
        secrets
            .accounts
            .iter()
            .find(|account| account.account_id == id && account.kind == CLAUDE_BOX_TOKEN)
            .cloned()
    }) {
        environment.push(("CLAUDE_CODE_OAUTH_TOKEN".to_string(), account.value));
    }
    environment
}

/// `write-back`: Codex logins refreshed on the box that home has not saved yet. `ack` lists the
/// ids home saved; those become the new baseline and are not offered again.
pub(crate) fn write_back(
    provider: &CloudProviderId,
    ack: &[String],
) -> Result<BoxWriteBackResult, String> {
    let secrets = read_secrets(provider);
    let mut items = Vec::new();
    for account in secrets
        .accounts
        .iter()
        .filter(|account| account.kind == CODEX_AUTH_JSON)
    {
        let dir = account_dir(provider, &account.account_id);
        let copy = dir.join("auth.json");
        // Codex may have replaced the link with a file of its own; that one is newer.
        let link = codex_auth_path();
        let active: ActiveAccounts = state::read_json(&active_file(provider)).unwrap_or_default();
        let current_path = if active.codex.as_deref() == Some(account.account_id.as_str())
            && std::fs::symlink_metadata(&link).is_ok_and(|meta| meta.file_type().is_file())
        {
            link
        } else {
            copy.clone()
        };
        let Ok(bytes) = std::fs::read(&current_path) else {
            continue;
        };
        let current_sha = sha256_hex(&bytes);
        let baseline = std::fs::read_to_string(dir.join(INJECTED_SHA_FILE)).unwrap_or_default();
        if baseline.trim() == current_sha {
            continue;
        }
        let id = current_sha[..16].to_string();
        if ack.contains(&id) {
            state::write_private(&dir.join(INJECTED_SHA_FILE), current_sha.as_bytes())?;
            continue;
        }
        let refreshed_at = std::fs::metadata(&current_path)
            .and_then(|meta| meta.modified())
            .map(|time| {
                chrono::DateTime::<chrono::Utc>::from(time)
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            })
            .unwrap_or_else(|_| state::now_iso());
        items.push(BoxWriteBackItem {
            id,
            account_id: account.account_id.clone(),
            kind: CODEX_AUTH_JSON.to_string(),
            value: String::from_utf8_lossy(&bytes).into_owned(),
            refreshed_at,
        });
    }
    Ok(BoxWriteBackResult { items })
}

/// `ghostex box write-back [--ack <ids>]`.
pub(crate) fn run_write_back(ack: &[String]) -> Result<BoxWriteBackResult, String> {
    let ticket = state::require_ticket()?;
    write_back(&ticket.provider, ack)
}
