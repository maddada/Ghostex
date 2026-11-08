//! Ghostex Collab's saved choices: the switch and what a new share starts with. gxserver owns
//! them in one file so Settings > Collab, the chat's Share popover, the CLI and the phone read and
//! change the same thing.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User, on the design page (Q1610 to Q1617): "we need a page in settings called "Collab" and we need to call this feature Ghostex Collab". The switch is off by default. A new share starts with "expiry after 24 hours and a short code the guest types" (no "ask before someone joins" by default), and in "approve each message"; the other modes (straight to the agent, view only) are picked per chat.

use std::sync::{Mutex, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::GxserverPaths;

const CONFIG_FILE: &str = "collab.json";

/// What a guest's message does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuestMode {
    /// Straight to the agent, labelled with the guest's name.
    Send,
    /// Held until the owner presses Send (or Edit, or Decline).
    #[default]
    Approve,
    /// Guests watch; their chat box is off.
    View,
}

impl GuestMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "send" => Some(Self::Send),
            "approve" => Some(Self::Approve),
            "view" => Some(Self::View),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Approve => "approve",
            Self::View => "view",
        }
    }
}

/// How long a new share's link works: an hour by default, a day at most, never "until stopped".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Expiry {
    #[default]
    Hour,
    FourHours,
    Day,
}

/// The longest a share may live, whatever the files say.
pub const MAX_SHARE_MS: i64 = 24 * 3_600_000;

impl Expiry {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "hour" => Some(Self::Hour),
            "fourHours" => Some(Self::FourHours),
            "day" => Some(Self::Day),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::FourHours => "fourHours",
            Self::Day => "day",
        }
    }

    pub fn millis(self) -> i64 {
        match self {
            Self::Hour => 3_600_000,
            Self::FourHours => 4 * 3_600_000,
            Self::Day => MAX_SHARE_MS,
        }
    }
}

/// The version of `collab.json` this gxserver writes. A file from before it (11.0.0's) is
/// migrated once by [`migrate`]: every share ends and Collab is off.
pub const CONFIG_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CollabConfig {
    pub version: u32,
    /// Ghostex Collab itself. Off: nothing can be shared, gxserver opens no relay connection and
    /// every link stops working. Turning it on needs the owner's signed approval.
    pub enabled: bool,
    /// How long a new share's link works.
    pub expiry: Expiry,
    /// What a new share's guest messages do: `approve` or `view`. Send mode is turned on per
    /// share, with the owner's signed approval, never as a default.
    pub mode: GuestMode,
    /// The name guests see for the owner; empty uses this computer's user name.
    pub owner_name: String,
}

impl Default for CollabConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            enabled: false,
            expiry: Expiry::Hour,
            mode: GuestMode::Approve,
            owner_name: String::new(),
        }
    }
}

fn cached() -> &'static Mutex<Option<CollabConfig>> {
    static CACHE: OnceLock<Mutex<Option<CollabConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn read_config(paths: &GxserverPaths) -> CollabConfig {
    let mut cache = cached().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(config) = cache.as_ref() {
        return config.clone();
    }
    let config = parse_config(
        &std::fs::read_to_string(paths.app_state_dir.join(CONFIG_FILE)).unwrap_or_default(),
    );
    *cache = Some(config.clone());
    config
}

/// The saved choices, with anything a file edit could slip in put back to a safe value: Send is
/// never a default, and a file from an older version reads as off.
fn parse_config(text: &str) -> CollabConfig {
    let raw: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let old = raw["version"].as_u64().unwrap_or(0) < u64::from(CONFIG_VERSION);
    let mut config = if old {
        CollabConfig {
            owner_name: raw["ownerName"].as_str().unwrap_or_default().to_string(),
            ..CollabConfig::default()
        }
    } else {
        serde_json::from_value::<CollabConfig>(raw).unwrap_or_default()
    };
    if config.mode == GuestMode::Send {
        config.mode = GuestMode::Approve;
    }
    config
}

pub fn update_config(
    paths: &GxserverPaths,
    change: impl FnOnce(&mut CollabConfig),
) -> std::io::Result<CollabConfig> {
    let current = read_config(paths);
    let mut cache = cached().lock().unwrap_or_else(PoisonError::into_inner);
    let mut next = cache.clone().unwrap_or(current);
    change(&mut next);
    next.version = CONFIG_VERSION;
    next.owner_name = next.owner_name.trim().chars().take(40).collect();
    write_config(paths, &next)?;
    *cache = Some(next.clone());
    Ok(next)
}

fn write_config(paths: &GxserverPaths, config: &CollabConfig) -> std::io::Result<()> {
    write_config_in(&paths.app_state_dir, config)
}

fn write_config_in(folder: &std::path::Path, config: &CollabConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let target = folder.join(CONFIG_FILE);
    let temporary = folder.join(format!("{CONFIG_FILE}.tmp"));
    std::fs::write(
        &temporary,
        serde_json::to_string_pretty(config).unwrap_or_default(),
    )?;
    std::fs::rename(&temporary, &target)
}

/// One-time move to the 11.0.1 rules (GitHub #495), run at gxserver start before anything reads a
/// share: a `collab.json` older than [`CONFIG_VERSION`] (or none, with share files beside it)
/// ends every share, forgets the held guest messages, turns Collab off and drops the old
/// `relayUrl`, `requireCode` and `never` choices. Only Your name is kept. `true` when it ran.
///
/// CDXC:Collab 2026-11-08 DECISION:
/// User (#495 decisions): "Free to break compatibility: assume nobody uses Collab sharing". So 11.0.0's shares, which could be no-code, never-expiring Send links made by any token holder, are ended rather than carried over.
pub fn migrate(paths: &GxserverPaths, share_files: &[&str]) -> bool {
    match migrate_folder(&paths.app_state_dir, share_files) {
        Some(config) => {
            *cached().lock().unwrap_or_else(PoisonError::into_inner) = Some(config);
            true
        }
        None => false,
    }
}

fn migrate_folder(folder: &std::path::Path, share_files: &[&str]) -> Option<CollabConfig> {
    let raw: Option<Value> = std::fs::read_to_string(folder.join(CONFIG_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let version = raw
        .as_ref()
        .and_then(|raw| raw.get("version"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let has_share_files = share_files.iter().any(|name| folder.join(name).exists());
    if version >= u64::from(CONFIG_VERSION) || (raw.is_none() && !has_share_files) {
        return None;
    }
    for name in share_files {
        let _ = std::fs::remove_file(folder.join(name));
    }
    let owner_name = raw
        .as_ref()
        .and_then(|raw| raw.get("ownerName"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .chars()
        .take(40)
        .collect();
    let config = CollabConfig {
        owner_name,
        ..CollabConfig::default()
    };
    let _ = write_config_in(folder, &config);
    Some(config)
}

/// Applies a partial JSON object of choices (Settings > Collab's and the CLI's edits). `relayUrl`,
/// `requireCode` and a Send default are refused: the relay is the owner's (set where gxserver
/// starts, never through the API), a code is always asked, and Send is turned on per share.
pub fn apply_patch(config: &mut CollabConfig, patch: &Value) -> Result<(), String> {
    let Some(patch) = patch.as_object() else {
        return Err("patch must be an object.".to_string());
    };
    if patch.contains_key("relayUrl") {
        return Err("The share relay can't be changed through Ghostex's API.".to_string());
    }
    if patch
        .get("requireCode")
        .is_some_and(|value| value != &Value::Bool(true))
    {
        return Err("Guests always need the share's code.".to_string());
    }
    let expiry = match patch.get("expiry") {
        None => None,
        Some(value) => Some(value.as_str().and_then(Expiry::parse).ok_or_else(|| {
            "expiry is hour, fourHours or day (a link lasts a day at most).".to_string()
        })?),
    };
    let mode = match patch.get("mode") {
        None => None,
        Some(value) => match value.as_str().and_then(GuestMode::parse) {
            Some(GuestMode::Send) => {
                return Err(
                    "Send is turned on per shared chat, with your approval, not as a default."
                        .to_string(),
                )
            }
            Some(mode) => Some(mode),
            None => return Err("mode is approve or view.".to_string()),
        },
    };
    if let Some(enabled) = patch.get("enabled").and_then(Value::as_bool) {
        config.enabled = enabled;
    }
    if let Some(expiry) = expiry {
        config.expiry = expiry;
    }
    if let Some(mode) = mode {
        config.mode = mode;
    }
    if let Some(name) = patch.get("ownerName").and_then(Value::as_str) {
        config.owner_name = name.to_string();
    }
    Ok(())
}

/// The owner's name as guests see it.
pub fn owner_display_name(config: &CollabConfig) -> String {
    if !config.owner_name.trim().is_empty() {
        return config.owner_name.trim().to_string();
    }
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(target_os = "macos")]
        if let Some(full) = std::process::Command::new("id")
            .arg("-F")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|name| !name.is_empty())
        {
            return full.split_whitespace().next().unwrap_or(&full).to_string();
        }
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .ok()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "The owner".to_string())
    })
    .clone()
}

/// The relay a share connects to: Ghostex's own, or `GHOSTEX_COLLAB_RELAY_URL` from the environment
/// gxserver was started with (a local relay while developing, a self-hosted one). Never a file or
/// an API call: an agent that could point it elsewhere would route every link, key included,
/// through its own server.
pub fn relay_url() -> String {
    std::env::var("GHOSTEX_COLLAB_RELAY_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| super::GHOSTEX_SHARE_RELAY_URL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patches_cannot_weaken_a_share() {
        let mut config = CollabConfig::default();
        assert!(apply_patch(&mut config, &json!({"relayUrl": "https://evil.example"})).is_err());
        assert!(apply_patch(&mut config, &json!({"requireCode": false})).is_err());
        assert!(apply_patch(&mut config, &json!({"expiry": "never"})).is_err());
        assert!(apply_patch(&mut config, &json!({"expiry": "week"})).is_err());
        assert!(apply_patch(&mut config, &json!({"mode": "send"})).is_err());
        // A refused patch changes nothing, even its harmless keys.
        assert!(apply_patch(&mut config, &json!({"enabled": true, "mode": "send"})).is_err());
        assert!(!config.enabled);
        apply_patch(
            &mut config,
            &json!({"expiry": "day", "mode": "view", "ownerName": "Sam"}),
        )
        .unwrap();
        assert_eq!(config.expiry.millis(), MAX_SHARE_MS);
        assert_eq!(config.mode, GuestMode::View);
    }

    #[test]
    fn defaults_are_off_one_hour_and_approve() {
        let config = CollabConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.expiry, Expiry::Hour);
        assert_eq!(config.mode, GuestMode::Approve);
    }

    #[test]
    fn an_old_or_edited_file_reads_safe() {
        // 11.0.0's file: on, Send, never, no code, another relay.
        let old = parse_config(
            r#"{"enabled":true,"expiry":"never","requireCode":false,"mode":"send","ownerName":"Sam","relayUrl":"https://evil.example"}"#,
        );
        assert!(!old.enabled);
        assert_eq!(old.owner_name, "Sam");
        assert_eq!(old.mode, GuestMode::Approve);
        let edited = parse_config(r#"{"version":2,"enabled":true,"mode":"send"}"#);
        assert_eq!(edited.mode, GuestMode::Approve);
    }

    #[test]
    fn migration_ends_old_shares_once() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path();
        std::fs::write(
            folder.join(CONFIG_FILE),
            r#"{"enabled":true,"expiry":"never","ownerName":"Sam","relayUrl":"x"}"#,
        )
        .unwrap();
        std::fs::write(folder.join("collab-shares.json"), "[]").unwrap();
        std::fs::write(folder.join("collab-held.json"), "{}").unwrap();
        let files = ["collab-shares.json", "collab-held.json"];
        assert!(migrate_folder(folder, &files).is_some());
        assert!(!folder.join("collab-shares.json").exists());
        assert!(!folder.join("collab-held.json").exists());
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(folder.join(CONFIG_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved["enabled"], false);
        assert_eq!(saved["ownerName"], "Sam");
        assert_eq!(saved["version"], CONFIG_VERSION);
        assert!(saved.get("relayUrl").is_none());
        std::fs::write(folder.join("collab-shares.json"), "[]").unwrap();
        assert!(migrate_folder(folder, &files).is_none(), "runs once");
        assert!(folder.join("collab-shares.json").exists());
    }
}
