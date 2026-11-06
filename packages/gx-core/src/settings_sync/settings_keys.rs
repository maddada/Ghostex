//! Which settings each category syncs, and how their keys read.
//!
//! Keys are `<category>/setting/<settings key>` for a value of the settings file (Keybindings:
//! `keybindings/setting/<os>/hotkeys`), `agents/modelPin/<agent>` for an agent's remembered
//! default model and effort, and `coordinator/playbook/all` or `coordinator/playbook/project/<repo>`
//! for an edited coordinator playbook.
//!
//! CDXC:SettingsSync 2026-11-06 WHY:
//! Appearance leaves out what belongs to the computer rather than the person: the glass picture and video (file paths there) and the interface scale and zoom (they follow the screen). Keyboard shortcuts sync only between computers of the same system, since a Mac's and a Windows computer's keys differ. A playbook edited for one project follows the repository, as Spaces do.

use std::collections::BTreeMap;

use ghostex_settings_catalog::{general_sections, Platform, ToJson};
use serde_json::Value;

use super::entries::{SyncConflict, SyncEntry};

/// Appearance rows that stay on each computer.
const APPEARANCE_LEFT_OUT: &[&str] = &[
    "windowGlassImageDark",
    "windowGlassImageLight",
    "windowGlassVideoDark",
    "windowGlassVideoLight",
    "interfaceScalePercent",
    "interfaceScaleAreasSeparately",
    "agentManagerZoomPercent",
    "sessionChatZoomPercent",
    "filesZoomPercent",
];

/// Agent defaults that live in the settings file (the remembered models are gxserver's own).
const AGENT_DEFAULT_KEYS: &[&str] = &[
    "defaultPromptAgentId",
    "preferredAgentInterface",
    "preferredAgentInterfaceOverrides",
    "newSessionAgentScope",
    "sessionTitleGenerationAgent",
    "agentAcceptAllEnabled",
    "showQuickModelPickerInTerminal",
];

const COORDINATOR_KEYS: &[&str] = &[
    "orchestratorPlainReplies",
    "orchestratorAllEfforts",
    "coordinatorBoardView",
];

/// This computer's system, as the Keybindings keys name it.
fn os_name() -> &'static str {
    match Platform::current() {
        Platform::MacOs => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

/// The settings-file keys category `prefix` syncs.
pub fn category_setting_keys(prefix: &str) -> Vec<&'static str> {
    let section = |id: &str| -> Vec<&'static str> {
        general_sections(Platform::current())
            .into_iter()
            .filter(|section| section.id == id)
            .flat_map(|section| section.settings.into_iter().map(|row| row.key))
            .collect()
    };
    match prefix {
        "appearance" => section("theming")
            .into_iter()
            .filter(|key| !APPEARANCE_LEFT_OUT.contains(key))
            .collect(),
        "sounds" => section("sounds"),
        "agents" => AGENT_DEFAULT_KEYS.to_vec(),
        "keybindings" => vec!["hotkeys"],
        "coordinator" => COORDINATOR_KEYS.to_vec(),
        _ => Vec::new(),
    }
}

/// The synced key of a settings-file value.
pub fn setting_sync_key(prefix: &str, setting: &str) -> String {
    if prefix == "keybindings" {
        format!("keybindings/setting/{}/{setting}", os_name())
    } else {
        format!("{prefix}/setting/{setting}")
    }
}

/// What a computer whose settings file holds `settings` has for each key of category `prefix`:
/// the saved value, else the shipped default. `defaults` lists the keys still at their default.
pub fn category_setting_values(
    prefix: &str,
    settings: Option<&Value>,
) -> (BTreeMap<String, Value>, Vec<String>) {
    let mut values = BTreeMap::new();
    let mut defaults = Vec::new();
    for setting in category_setting_keys(prefix) {
        let key = setting_sync_key(prefix, setting);
        let default =
            ghostex_settings_catalog::default_value(setting).map(|j| j.to_json().to_value());
        let saved = settings.and_then(|settings| settings.get(setting)).cloned();
        // Only what the app's settings patch can carry (a list or an empty value can't).
        let value = match (saved, default) {
            (Some(saved), default) => {
                if Some(&saved) == default.as_ref() {
                    defaults.push(key.clone());
                }
                saved
            }
            (None, Some(default)) => {
                defaults.push(key.clone());
                default
            }
            (None, None) => continue,
        };
        if !crate::renderer_commands::settings_patch_value_allowed(setting, &value) {
            defaults.retain(|default| *default != key);
            continue;
        }
        values.insert(key, value);
    }
    (values, defaults)
}

/// The settings-file key a synced key writes (`None` for a key that isn't a setting of this
/// computer's system).
pub fn setting_of_sync_key(key: &str) -> Option<&str> {
    let (prefix, rest) = key.split_once("/setting/")?;
    if prefix == "keybindings" {
        let (os, setting) = rest.split_once('/')?;
        return (os == os_name()).then_some(setting);
    }
    Some(rest)
}

/// One plain sentence about a conflict on a setting, a remembered model or a playbook.
pub fn describe_setting_key(conflict: &SyncConflict) -> Option<String> {
    let kept = &conflict.kept;
    let lost = &conflict.lost;
    let key = kept.key.as_str();
    if let Some(setting) = setting_of_sync_key(key) {
        return Some(format!(
            "{} is {} from {}, replacing {} from {}.",
            setting_title(setting),
            show(&kept.value),
            kept.origin_name,
            show(&lost.value),
            lost.origin_name,
        ));
    }
    if let Some(agent) = key.strip_prefix("agents/modelPin/") {
        let model = |entry: &SyncEntry| match &entry.value {
            Some(pin) => {
                let model = pin
                    .get("modelLabel")
                    .or_else(|| pin.get("model"))
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                match pin
                    .get("effortLabel")
                    .or_else(|| pin.get("effort"))
                    .and_then(Value::as_str)
                {
                    Some(effort) => format!("“{model} {effort}”"),
                    None => format!("“{model}”"),
                }
            }
            None => "none".to_string(),
        };
        return Some(format!(
            "The default model for {agent} is {} from {}, replacing {} from {}.",
            model(kept),
            kept.origin_name,
            model(lost),
            lost.origin_name,
        ));
    }
    if let Some(which) = key.strip_prefix("coordinator/playbook/") {
        let scope = match which.strip_prefix("project/") {
            Some(repo) => format!("for {}", repo.rsplit('/').next().unwrap_or(repo)),
            None => "for all projects".to_string(),
        };
        return Some(format!(
            "The coordinator playbook {scope} is the edit from {}, replacing the edit from {}.",
            kept.origin_name, lost.origin_name,
        ));
    }
    None
}

/// The setting's title in Settings, else its key.
fn setting_title(setting: &str) -> String {
    general_sections(Platform::current())
        .into_iter()
        .flat_map(|section| section.settings)
        .find(|row| row.key == setting)
        .map(|row| row.title)
        .unwrap_or_else(|| setting.to_string())
}

fn show(value: &Option<Value>) -> String {
    match value {
        None => "the default".to_string(),
        Some(Value::String(text)) => format!("“{text}”"),
        Some(Value::Bool(true)) => "on".to_string(),
        Some(Value::Bool(false)) => "off".to_string(),
        Some(Value::Object(_)) | Some(Value::Array(_)) => "a different set".to_string(),
        Some(other) => other.to_string(),
    }
}
