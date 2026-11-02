//! The voice agent's settings (provider and voices, from the shared settings file) and its API keys
//! (sealed in gxserver's vault, never in the settings file).
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: the API keys live on the computer, entered in desktop Settings and kept by gxserver; the phone never stores a key. Each provider has its own sealed key and the phone only asks gxserver whether one is set.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::provider::Provider;
use crate::paths::GxserverPaths;
use crate::vault::{self, UnsealError};

pub(super) const PROVIDER_SETTING: &str = "voiceAgentProvider";
const XAI_VOICE_SETTING: &str = "voiceAgentXaiVoice";
const OPENAI_VOICE_SETTING: &str = "voiceAgentOpenaiVoice";
pub(super) const SPEED_SETTING: &str = "voiceAgentSpeed";
/// The speaking speeds clients offer (`voiceAgentSpeed`); clients play the audio this much faster.
pub(super) const SPEEDS: [f64; 6] = [1.0, 1.2, 1.4, 1.6, 1.8, 2.0];

/// The shared settings file, read fresh (Settings writes it while gxserver runs).
pub(super) fn read_settings(paths: &GxserverPaths) -> Value {
    crate::shared_settings_file::read_shared_settings(paths).unwrap_or(Value::Null)
}

pub(super) fn selected_provider(paths: &GxserverPaths) -> Provider {
    read_settings(paths)
        .get(PROVIDER_SETTING)
        .and_then(Value::as_str)
        .and_then(Provider::from_id)
        .unwrap_or(Provider::Xai)
}

pub(super) fn selected_voice(paths: &GxserverPaths, provider: Provider) -> String {
    let key = match provider {
        Provider::Xai => XAI_VOICE_SETTING,
        Provider::OpenAi => OPENAI_VOICE_SETTING,
    };
    read_settings(paths)
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|voice| {
            !voice.is_empty()
                && voice.len() <= 64
                && voice
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| provider.default_voice().to_string())
}

pub(super) const CALL_MODE_SETTING: &str = "voiceAgentCallMode";

/// Whether new calls are push-to-talk (`voiceAgentCallMode` is `pushToTalk`); the default `regular`
/// call listens hands-free. Read when a call starts; a change applies to the next call.
pub(super) fn push_to_talk_calls(paths: &GxserverPaths) -> bool {
    read_settings(paths)
        .get(CALL_MODE_SETTING)
        .and_then(Value::as_str)
        == Some("pushToTalk")
}

/// `voiceAgentSpeed` as one of [`SPEEDS`] (1 when unset or unknown).
pub(super) fn selected_speed(paths: &GxserverPaths) -> f64 {
    let settings = read_settings(paths);
    let saved = match settings.get(SPEED_SETTING) {
        Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
        Some(Value::Number(number)) => number.as_f64(),
        _ => None,
    };
    saved
        .and_then(|saved| {
            SPEEDS
                .into_iter()
                .find(|speed| (speed - saved).abs() < 0.01)
        })
        .unwrap_or(1.0)
}

/// The setting key of `provider`'s voice.
pub(super) fn voice_setting(provider: Provider) -> &'static str {
    match provider {
        Provider::Xai => XAI_VOICE_SETTING,
        Provider::OpenAi => OPENAI_VOICE_SETTING,
    }
}

fn sealed_path(paths: &GxserverPaths, provider: Provider) -> PathBuf {
    paths
        .auth_dir
        .join("voice-agent")
        .join(format!("{}.sealed", provider.id()))
}

/// Where a key that lives in a secret provider is pointed at (`<provider>.ref.json`): the
/// reference only, never the key.
///
/// CDXC:Secrets 2026-11-02 WHY:
/// A key field that is given an `op://vault/item/field` reference keeps the reference instead of a sealed copy, and the key is read through 1Password each time it is used, so a key rotated in 1Password needs no paste here. The sealed key and the reference replace each other.
fn reference_path(paths: &GxserverPaths, provider: Provider) -> PathBuf {
    paths
        .auth_dir
        .join("voice-agent")
        .join(format!("{}.ref.json", provider.id()))
}

/// The secret provider reference the provider's key is read from, if it has one.
pub(super) fn key_reference(
    paths: &GxserverPaths,
    provider: Provider,
) -> Option<crate::secret_providers::SecretReference> {
    let text = fs::read_to_string(reference_path(paths, provider)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Points the provider's key at a secret provider reference, dropping any sealed copy.
pub(super) fn write_key_reference(
    paths: &GxserverPaths,
    provider: Provider,
    reference: &crate::secret_providers::SecretReference,
) -> Result<(), String> {
    let path = reference_path(paths, provider);
    let body = serde_json::to_string(reference)
        .map_err(|_| "Couldn't save the 1Password reference.".to_string())?;
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
    write().map_err(|error| format!("Couldn't save the 1Password reference: {error}"))?;
    match fs::remove_file(sealed_path(paths, provider)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("Couldn't remove the old API key: {error}"))
        }
        _ => Ok(()),
    }
}

/// [`read_key`] off the async runtime: a key kept in 1Password may wait for an unlock.
pub(super) async fn read_key_async(
    paths: &GxserverPaths,
    provider: Provider,
) -> Result<Option<String>, String> {
    let paths = paths.clone();
    tokio::task::spawn_blocking(move || read_key(&paths, provider))
        .await
        .map_err(|_| "Couldn't read the API key.".to_string())?
}

/// The provider's saved key, unsealed or read through its secret provider; `None` when none is
/// saved. Blocking.
pub(super) fn read_key(
    paths: &GxserverPaths,
    provider: Provider,
) -> Result<Option<String>, String> {
    if let Some(reference) = key_reference(paths, provider) {
        return crate::secret_providers::resolve(paths, &reference)
            .map(|key| Some(key.trim().to_string()))
            .map_err(|message| {
                format!(
                    "Couldn't read the {} API key from 1Password: {message}",
                    provider.label()
                )
            });
    }
    match fs::read_to_string(sealed_path(paths, provider)) {
        Ok(sealed) => match vault::unseal(paths, sealed.trim()) {
            Ok(key) => Ok(Some(key)),
            Err(UnsealError::Unreadable) => Err(format!(
                "The saved {} API key can't be read anymore. Paste it again in Settings > Extensions > Voice agent.",
                provider.label()
            )),
            Err(UnsealError::Vault(message)) => Err(message),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Couldn't read the saved API key: {error}")),
    }
}

/// What `/api/readVoiceAgent` says about a provider's key.
///
/// CDXC:VoiceMode 2026-10-22 DECISION:
/// User, asked whether Settings should say when a saved key can't be unlocked: "sure just make the error more general please". A saved key that no longer opens shows in Settings as needing to be pasted again instead of as saved, and every message about it stays general ("can't be read anymore"), never guessing why (no other computer, restore or vault key).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyState {
    Missing,
    Saved,
    /// Saved, but it doesn't open any more; only pasting it again helps.
    Unreadable,
}

/// Opens the saved key to see that it still can: one small file and the vault key the vault
/// caches for the process, cheap enough for every read.
pub(super) fn key_state(paths: &GxserverPaths, provider: Provider) -> KeyState {
    // Read only when used: 1Password may ask to unlock, which a status read must never cause.
    if key_reference(paths, provider).is_some() {
        return KeyState::Saved;
    }
    match fs::read_to_string(sealed_path(paths, provider)) {
        Ok(sealed) => match vault::unseal(paths, sealed.trim()) {
            Ok(_) => KeyState::Saved,
            Err(UnsealError::Unreadable) => KeyState::Unreadable,
            // The vault key may load next time (an unlocked Keychain); using the key says why.
            Err(UnsealError::Vault(_)) => KeyState::Saved,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => KeyState::Missing,
        Err(_) => KeyState::Unreadable,
    }
}

/// Seals and saves the key (a 0600 temporary file renamed over the old one), or removes it when
/// `key` is `None`.
pub(super) fn write_key(
    paths: &GxserverPaths,
    provider: Provider,
    key: Option<&str>,
) -> Result<(), String> {
    let path = sealed_path(paths, provider);
    match fs::remove_file(reference_path(paths, provider)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Couldn't remove the 1Password reference: {error}"))
        }
        _ => {}
    }
    let Some(key) = key else {
        return match fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("Couldn't remove the API key: {error}"))
            }
            _ => Ok(()),
        };
    };
    let sealed = vault::seal(paths, key)?;
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("sealed.tmp");
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
            file.write_all(sealed.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&temp, &path)
    };
    write().map_err(|error| format!("Couldn't save the API key: {error}"))
}

/// What checking a key with its provider found.
pub(super) enum KeyCheck {
    Valid,
    Refused,
    Unreachable(String),
}

/// Asks the provider whether the key works (blocking; run it off the async runtime).
pub(super) fn check_key(provider: Provider, key: &str) -> KeyCheck {
    let response = crate::http_agent::agent()
        .get(provider.key_check_url())
        .header("authorization", format!("Bearer {key}"))
        .config()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .call();
    match response {
        Ok(_) => KeyCheck::Valid,
        Err(ureq::Error::StatusCode(400 | 401 | 403)) => KeyCheck::Refused,
        Err(ureq::Error::StatusCode(status)) => {
            KeyCheck::Unreachable(format!("{} answered HTTP {status}.", provider.label()))
        }
        Err(error) => {
            KeyCheck::Unreachable(format!("Couldn't reach {}: {error}", provider.label()))
        }
    }
}
