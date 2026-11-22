//! The secret providers the user added in Settings > Secrets (`auth/secret-providers.json`, 0600),
//! and reading one secret through them. A provider entry names where secrets live (a 1Password
//! account), never a secret itself; the value is read at the moment it is needed and handed only
//! to the code that asked.

use std::{fs, io::Write as _, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::one_password;
use crate::paths::GxserverPaths;

const FILE_NAME: &str = "secret-providers.json";
const MAX_LABEL_CHARS: usize = 64;
const MAX_PROVIDERS: usize = 32;
pub(crate) const KIND_ONE_PASSWORD: &str = "onePassword";

/// One provider. `kind` decides what `account` means; kinds this build does not know are kept as
/// written so a newer build's entries survive an older build's save.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecretProvider {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) account: String,
}

/// Points at a secret a provider holds. It is not a secret itself and is stored as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecretReference {
    pub(crate) provider: String,
    pub(crate) reference: String,
}

#[derive(Default, Serialize, Deserialize)]
struct StoredProviders {
    #[serde(default)]
    providers: Vec<SecretProvider>,
}

fn file_path(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join(FILE_NAME)
}

pub(crate) fn list(paths: &GxserverPaths) -> Vec<SecretProvider> {
    fs::read_to_string(file_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<StoredProviders>(&text).ok())
        .map(|stored| stored.providers)
        .unwrap_or_default()
}

fn write(paths: &GxserverPaths, providers: &[SecretProvider]) -> Result<(), String> {
    let path = file_path(paths);
    let body = serde_json::to_string_pretty(&json!({ "providers": providers }))
        .map_err(|_| "Couldn't save the secret providers.".to_string())?;
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        let _ = fs::remove_file(&temp);
        {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(body.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&temp, &path)
    };
    write().map_err(|error| format!("Couldn't save the secret providers: {error}"))
}

/// A stable id from the name, so removing and adding a provider under the same name keeps every
/// setting that points at it working.
pub(crate) fn id_from_label(label: &str) -> Option<String> {
    let mut slug = String::new();
    for ch in label.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(64).collect();
    let slug = slug.trim_end_matches('-').to_string();
    (!slug.is_empty()).then_some(slug)
}

/// Adds a provider (no `id`) or changes the one with `id`. Every check runs here, so a client
/// cannot save what this build would refuse to read.
pub(crate) fn save(
    paths: &GxserverPaths,
    id: Option<&str>,
    kind: &str,
    label: &str,
    account: &str,
) -> Result<SecretProvider, String> {
    if kind != KIND_ONE_PASSWORD {
        return Err("Only 1Password providers can be added.".to_string());
    }
    let label = label.trim();
    if label.is_empty() {
        return Err("Enter a name.".to_string());
    }
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(format!("Keep the name under {MAX_LABEL_CHARS} characters."));
    }
    let account = account.trim();
    one_password::validate_account(account)?;
    let mut providers = list(paths);
    let provider = match id {
        Some(id) => {
            let existing = providers
                .iter_mut()
                .find(|provider| provider.id == id)
                .ok_or_else(|| "That secret provider no longer exists.".to_string())?;
            existing.label = label.to_string();
            existing.account = account.to_string();
            existing.clone()
        }
        None => {
            let id = id_from_label(label)
                .ok_or_else(|| "The name needs at least one letter or digit.".to_string())?;
            if providers.iter().any(|provider| provider.id == id) {
                return Err("A secret provider with this name already exists.".to_string());
            }
            if providers.len() >= MAX_PROVIDERS {
                return Err("That is as many secret providers as Ghostex keeps.".to_string());
            }
            let provider = SecretProvider {
                id,
                kind: kind.to_string(),
                label: label.to_string(),
                account: account.to_string(),
            };
            providers.push(provider.clone());
            provider
        }
    };
    write(paths, &providers)?;
    Ok(provider)
}

pub(crate) fn remove(paths: &GxserverPaths, id: &str) -> Result<bool, String> {
    let mut providers = list(paths);
    let before = providers.len();
    providers.retain(|provider| provider.id != id);
    if providers.len() == before {
        return Ok(false);
    }
    write(paths, &providers)?;
    Ok(true)
}

/// Checks a reference's shape against its provider without reading anything.
pub(crate) fn validate_reference(
    paths: &GxserverPaths,
    reference: &SecretReference,
) -> Result<SecretProvider, String> {
    let provider = list(paths)
        .into_iter()
        .find(|provider| provider.id == reference.provider)
        .ok_or_else(|| {
            "That secret provider is not set up on this computer. Add it in Settings > Secrets."
                .to_string()
        })?;
    if provider.kind != KIND_ONE_PASSWORD {
        return Err(
            "This version of Ghostex can't read from that kind of secret provider.".to_string(),
        );
    }
    if !one_password::is_reference(reference.reference.trim()) {
        return Err(one_password::INVALID_REFERENCE_MESSAGE.to_string());
    }
    Ok(provider)
}

/// Reads the secret a reference points at. Blocking (1Password may wait for an unlock), so call it
/// off the async runtime.
pub(crate) fn resolve(
    paths: &GxserverPaths,
    reference: &SecretReference,
) -> Result<String, String> {
    let provider = validate_reference(paths, reference)?;
    one_password::read(&provider.account, reference.reference.trim())
        .map_err(|error| error.message().to_string())
}

/// What Settings shows for each provider: the entry, plus whether `op` is installed and signed in
/// to its account on this computer (no unlock needed to tell).
pub(crate) fn read_for_settings(paths: &GxserverPaths) -> Value {
    let providers = list(paths);
    let installed = one_password::op_path().is_some();
    let known = if installed && !providers.is_empty() {
        one_password::known_accounts()
    } else {
        None
    };
    let rows: Vec<Value> = providers
        .iter()
        .map(|provider| {
            let status = if provider.kind != KIND_ONE_PASSWORD {
                "unsupported"
            } else if !installed {
                "cliMissing"
            } else {
                match &known {
                    Some(accounts)
                        if accounts
                            .iter()
                            .any(|known| known.matches(&provider.account)) =>
                    {
                        "ready"
                    }
                    Some(_) => "accountMissing",
                    None => "unknown",
                }
            };
            json!({
                "id": provider.id,
                "kind": provider.kind,
                "label": provider.label,
                "account": provider.account,
                "status": status,
            })
        })
        .collect();
    json!({
        "ok": true,
        "providers": rows,
        "onePasswordCliInstalled": installed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_from_the_name() {
        assert_eq!(
            id_from_label("1Password Personal").as_deref(),
            Some("1password-personal")
        );
        assert_eq!(
            id_from_label("  Work / Team ").as_deref(),
            Some("work-team")
        );
        assert_eq!(id_from_label("---"), None);
    }
}
