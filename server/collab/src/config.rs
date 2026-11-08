//! Ghostex Collab's saved choices: what a new share starts with. The component owns them in one
//! file in its data folder so Settings > Collab, the chat's Share popover and the phone read and
//! change the same thing. Collab is on while it is installed: there is no switch here any more
//! (installing it is the owner's signed approval; uninstalling deletes this folder).
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User, on the design page (Q1610 to Q1617): "we need a page in settings called "Collab" and we need to call this feature Ghostex Collab". The switch is off by default. A new share starts with "expiry after 24 hours and a short code the guest types" (no "ask before someone joins" by default), and in "approve each message"; the other modes (straight to the agent, view only) are picked per chat.

use std::sync::{Mutex, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// How long a new share's link works: an hour by default, 4 hours, a day, or until the owner stops
/// sharing.
///
/// CDXC:Collab 2026-11-08 DECISION:
/// User (Q1785): "add \"until i stop it\" as an option please here and in settings". A link that never runs out is created with the same Touch ID approval as any link (the approval card says it stays live until you stop it), and the Share popover's status band says so plainly. This supersedes #495's "24 h max, no never" for links the owner chooses to keep; the code is still always asked, and Stop sharing or deleting the chat ends it at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Expiry {
    #[default]
    Hour,
    FourHours,
    Day,
    UntilStopped,
}

/// The longest a link that runs out may live, whatever the files say.
pub const MAX_SHARE_MS: i64 = 24 * 3_600_000;

impl Expiry {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "hour" => Some(Self::Hour),
            "fourHours" => Some(Self::FourHours),
            "day" => Some(Self::Day),
            "untilStopped" => Some(Self::UntilStopped),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::FourHours => "fourHours",
            Self::Day => "day",
            Self::UntilStopped => "untilStopped",
        }
    }

    /// How long a link lives; `None` until the owner stops sharing.
    pub fn millis(self) -> Option<i64> {
        match self {
            Self::Hour => Some(3_600_000),
            Self::FourHours => Some(4 * 3_600_000),
            Self::Day => Some(MAX_SHARE_MS),
            Self::UntilStopped => None,
        }
    }
}

/// The version of `collab.json` this component writes.
pub const CONFIG_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CollabConfig {
    pub version: u32,
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

pub fn read_config() -> CollabConfig {
    let mut cache = cached().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(config) = cache.as_ref() {
        return config.clone();
    }
    let config = parse_config(
        &std::fs::read_to_string(crate::data_dir().join(CONFIG_FILE)).unwrap_or_default(),
    );
    *cache = Some(config.clone());
    config
}

/// The saved choices, with anything a file edit could slip in put back to a safe value: Send is
/// never a default, and a file from an older version keeps only Your name.
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

pub fn update_config(change: impl FnOnce(&mut CollabConfig)) -> std::io::Result<CollabConfig> {
    let current = read_config();
    let mut cache = cached().lock().unwrap_or_else(PoisonError::into_inner);
    let mut next = cache.clone().unwrap_or(current);
    change(&mut next);
    next.version = CONFIG_VERSION;
    next.owner_name = next.owner_name.trim().chars().take(40).collect();
    write_config_in(&crate::data_dir(), &next)?;
    *cache = Some(next.clone());
    Ok(next)
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

/// Applies a partial JSON object of choices (Settings > Collab's and the CLI's edits). `relayUrl`,
/// `requireCode` and a Send default are refused: the relay is the owner's (set where gxserver
/// starts, never through the API), a code is always asked, and Send is turned on per share.
pub fn apply_patch(config: &mut CollabConfig, patch: &Value) -> Result<(), String> {
    let Some(patch) = patch.as_object() else {
        return Err("patch must be an object.".to_string());
    };
    if patch.contains_key("enabled") {
        return Err(
            "Collab is on while it is installed. Uninstall it in Settings > Extensions to turn it off."
                .to_string(),
        );
    }
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
        Some(value) => Some(
            value
                .as_str()
                .and_then(Expiry::parse)
                .ok_or_else(|| "expiry is hour, fourHours, day or untilStopped.".to_string())?,
        ),
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
/// gxserver was started with (gxserver passes it on to the sidecar) (a local relay while developing, a self-hosted one). Never a file or
/// an API call: an agent that could point it elsewhere would route every link, key included,
/// through its own server.
pub fn relay_url() -> String {
    std::env::var("GHOSTEX_COLLAB_RELAY_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| crate::GHOSTEX_SHARE_RELAY_URL.to_string())
}
