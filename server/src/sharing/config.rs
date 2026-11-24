//! Persistent Sharing settings (`<gxserver_state_dir>/sharing/config.json`, mode `0600` on Unix)
//! and expiry helpers. Never stores passcodes or active share secrets.
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! User chose: OpenTunnel default behind a pluggable provider trait (with custom OPENTUNNEL_API
//! relay URL setting), 24h default expiry (1h / 24h / 7d / untilStopped), passcode off by default
//! for both Live pages and localhost ports, and an on/off switch in Settings > Sharing.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub(crate) const DEFAULT_PROVIDER: &str = "opentunnel";
pub(crate) const DEFAULT_RELAY_URL: &str = "https://opentunnel.xyz";

/// How long a newly created share link remains active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ShareExpiry {
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "24h")]
    TwentyFourHours,
    #[serde(rename = "7d")]
    SevenDays,
    #[serde(rename = "untilStopped")]
    UntilStopped,
}

impl Default for ShareExpiry {
    fn default() -> Self {
        Self::TwentyFourHours
    }
}

impl ShareExpiry {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OneHour => "1h",
            Self::TwentyFourHours => "24h",
            Self::SevenDays => "7d",
            Self::UntilStopped => "untilStopped",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "1h" | "hour" => Some(Self::OneHour),
            "24h" | "day" => Some(Self::TwentyFourHours),
            "7d" | "week" => Some(Self::SevenDays),
            "untilStopped" | "until_stopped" | "forever" => Some(Self::UntilStopped),
            _ => None,
        }
    }

    /// Lifetime in milliseconds, or `None` for `UntilStopped`.
    pub(crate) fn duration_ms(self) -> Option<i64> {
        match self {
            Self::OneHour => Some(60 * 60 * 1000),
            Self::TwentyFourHours => Some(24 * 60 * 60 * 1000),
            Self::SevenDays => Some(7 * 24 * 60 * 60 * 1000),
            Self::UntilStopped => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SharingConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "default_provider")]
    pub(crate) provider: String,
    /// Optional self-hosted OpenTunnel API URL (`OPENTUNNEL_API`); empty uses `https://opentunnel.xyz`.
    #[serde(default)]
    pub(crate) relay_url: String,
    #[serde(default)]
    pub(crate) default_expiry: ShareExpiry,
    /// Passcode off by default for both pages and ports.
    #[serde(default)]
    pub(crate) default_passcode: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_provider() -> String {
    DEFAULT_PROVIDER.to_string()
}

impl Default for SharingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: DEFAULT_PROVIDER.to_string(),
            relay_url: String::new(),
            default_expiry: ShareExpiry::TwentyFourHours,
            default_passcode: false,
        }
    }
}

impl SharingConfig {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "provider": self.provider,
            "relayUrl": self.relay_url,
            "effectiveRelayUrl": self.effective_relay_url(),
            "defaultExpiry": self.default_expiry.as_str(),
            "defaultPasscode": self.default_passcode,
        })
    }

    pub(crate) fn effective_relay_url(&self) -> String {
        let trimmed = self.relay_url.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
        if let Ok(env_url) = std::env::var("OPENTUNNEL_API") {
            let env_trimmed = env_url.trim();
            if !env_trimmed.is_empty() {
                return env_trimmed.trim_end_matches('/').to_string();
            }
        }
        DEFAULT_RELAY_URL.to_string()
    }
}

/// Validates a custom OpenTunnel relay URL: empty (default) is allowed, `https://…` is allowed,
/// and loopback `http://127.0.0.1` / `http://localhost` is allowed for local self-hosted testing.
pub(crate) fn validate_relay_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|_| "Relay URL must be a valid https:// address.".to_string())?;
    let scheme = parsed.scheme();
    let host = parsed.host_str().unwrap_or_default();
    let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]");
    if scheme != "https" && !(scheme == "http" && is_loopback) {
        return Err(
            "Relay URL must use https:// (or http://localhost for local testing).".to_string(),
        );
    }
    if host.is_empty() {
        return Err("Relay URL must include a hostname.".to_string());
    }
    Ok(trimmed.to_string())
}

pub(crate) fn sharing_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("sharing")
}

fn config_path(state_dir: &Path) -> PathBuf {
    sharing_dir(state_dir).join("config.json")
}

pub(crate) fn load_config(state_dir: &Path) -> SharingConfig {
    let path = config_path(state_dir);
    let Ok(bytes) = fs::read(&path) else {
        return SharingConfig::default();
    };
    serde_json::from_slice::<SharingConfig>(&bytes).unwrap_or_default()
}

pub(crate) fn save_config(state_dir: &Path, config: &SharingConfig) -> Result<(), String> {
    let dir = sharing_dir(state_dir);
    let mut dir_builder = fs::DirBuilder::new();
    dir_builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut dir_builder, 0o700);
    dir_builder
        .create(&dir)
        .map_err(|error| format!("Could not create sharing directory: {error}"))?;

    let path = config_path(state_dir);
    let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("Could not encode sharing settings: {error}"))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("Could not write sharing settings: {error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Could not flush sharing settings: {error}"))?;
    fs::rename(&temp, &path)
        .map_err(|error| format!("Could not save sharing settings: {error}"))?;
    Ok(())
}
