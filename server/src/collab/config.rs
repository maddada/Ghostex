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

/// How long a new share's link works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Expiry {
    Hour,
    #[default]
    Day,
    Week,
    Never,
}

impl Expiry {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "hour" => Some(Self::Hour),
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Never => "never",
        }
    }

    pub fn millis(self) -> Option<i64> {
        match self {
            Self::Hour => Some(3_600_000),
            Self::Day => Some(24 * 3_600_000),
            Self::Week => Some(7 * 24 * 3_600_000),
            Self::Never => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CollabConfig {
    /// Ghostex Collab itself. Off: nothing can be shared, gxserver opens no relay connection and
    /// every link stops working.
    pub enabled: bool,
    /// How long a new share's link works.
    pub expiry: Expiry,
    /// A new share asks guests for its six-digit code.
    pub require_code: bool,
    /// What a new share's guest messages do.
    pub mode: GuestMode,
    /// The name guests see for the owner; empty uses this computer's user name.
    pub owner_name: String,
    /// A relay other than Ghostex's own, for a local relay while developing (not in Settings).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub relay_url: String,
}

impl Default for CollabConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            expiry: Expiry::Day,
            require_code: true,
            mode: GuestMode::Approve,
            owner_name: String::new(),
            relay_url: String::new(),
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
    let config = std::fs::read_to_string(paths.app_state_dir.join(CONFIG_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<CollabConfig>(&text).ok())
        .unwrap_or_default();
    *cache = Some(config.clone());
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
    next.owner_name = next.owner_name.trim().chars().take(40).collect();
    std::fs::create_dir_all(&paths.app_state_dir)?;
    let target = paths.app_state_dir.join(CONFIG_FILE);
    let temporary = paths.app_state_dir.join(format!("{CONFIG_FILE}.tmp"));
    std::fs::write(
        &temporary,
        serde_json::to_string_pretty(&next).unwrap_or_default(),
    )?;
    std::fs::rename(&temporary, &target)?;
    *cache = Some(next.clone());
    Ok(next)
}

/// Applies a partial JSON object of choices (Settings > Collab's and the CLI's edits).
pub fn apply_patch(config: &mut CollabConfig, patch: &Value) {
    let Some(patch) = patch.as_object() else {
        return;
    };
    if let Some(enabled) = patch.get("enabled").and_then(Value::as_bool) {
        config.enabled = enabled;
    }
    if let Some(expiry) = patch
        .get("expiry")
        .and_then(Value::as_str)
        .and_then(Expiry::parse)
    {
        config.expiry = expiry;
    }
    if let Some(require) = patch.get("requireCode").and_then(Value::as_bool) {
        config.require_code = require;
    }
    if let Some(mode) = patch
        .get("mode")
        .and_then(Value::as_str)
        .and_then(GuestMode::parse)
    {
        config.mode = mode;
    }
    if let Some(name) = patch.get("ownerName").and_then(Value::as_str) {
        config.owner_name = name.to_string();
    }
    if let Some(url) = patch.get("relayUrl").and_then(Value::as_str) {
        config.relay_url = url.trim().to_string();
    }
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

/// The relay a share connects to: `GHOSTEX_COLLAB_RELAY_URL` or the file's `relayUrl` for a local
/// relay (`server/share-relay`), Ghostex's own otherwise.
pub fn relay_url(config: &CollabConfig) -> String {
    std::env::var("GHOSTEX_COLLAB_RELAY_URL")
        .ok()
        .or_else(|| Some(config.relay_url.clone()))
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| super::GHOSTEX_SHARE_RELAY_URL.to_string())
}
