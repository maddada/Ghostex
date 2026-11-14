//! Which settings each category syncs, and how their keys read.
//!
//! Keys are `<category>/setting/<settings key>` for a value of the settings file (Sidebar
//! organization's settings: `spaces/setting/<settings key>`; Keybindings:
//! `keybindings/setting/<os>/hotkeys`), `agents/modelPin/<agent>` for an agent's remembered
//! default model and effort, `coordinator/playbook/overrides` for the user's playbook overrides,
//! and `coordinator/playbook/all` or `coordinator/playbook/project/<repo>` for a full copy of the
//! coordinator playbook saved before overrides.
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
    "linuxDisplayScale",
    "agentManagerZoomPercent",
    "sessionChatZoomPercent",
    "filesZoomPercent",
];

/// Appearance values without a row of their own: the custom colours' depths, which Colourfulness
/// and the theme presets save together with the rows' values.
const APPEARANCE_WITHOUT_ROWS: &[&str] = &[
    "customSidebarTitlebarBackgroundDarknessPercent",
    "customSidebarTitlebarLightBackgroundLightnessPercent",
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

/// Sidebar organization: the settings that change how the sidebar is organized, synced with the
/// Settings that sync whatever Settings > Sync's switches say (`ALWAYS_CATEGORY`).
///
/// CDXC:MemoryGuard 2026-11-14 DECISION:
/// User (Q3122, "always sync"): the memory guard's two switches sync to all the user's computers even while Settings > Sync > Agent defaults is off, and each computer still guards itself (server/src/memory_guard/mod.rs). Supersedes the same day's choice to sync them with Agent defaults.
pub const ALWAYS_SYNCED_KEYS: &[&str] = &["memoryGuardEnabled", "memoryGuardNotify"];

/// Spaces themselves (the `spaces` category, its switch `syncSpaces`).
///
/// CDXC:SettingsSync 2026-11-12 DECISION:
/// User: "the spaces order and the projects inside them and whether we separate by machines or not need to be synced please when i select sync spaces. (call it sync sidebar organization instead)", then "also sync whether it's unified or not unified and the stuff related to sidebar in general that change how it is organized". So the Spaces switch, renamed Sidebar organization, also carries these: the Project or Unified sidebar, Separate by machine, how copies and worktrees are listed, the Spaces switch, the Unified sidebar's sections, grouping and project order, Hide projects without sessions, Pinned order, Group working sessions, the Parked section and the Session Tags list (the custom tags themselves sync per tag, server/src/settings_sync/sidebar_tags.rs). Left on each computer: sizes, animations, tooltips, card details and hover buttons (how it looks, not how it is organized), the click and parking behaviours, and what each window collapsed, hid, filtered or selected (kept in that window's own storage by this computer's project ids).
pub const SIDEBAR_ORGANIZATION_KEYS: &[&str] = &[
    "sidebarView",
    "sidebarSeparateByMachine",
    "sidebarMergeRepositoryCopies",
    "worktreeSessionsInProject",
    "sidebarSpacesEnabled",
    "sidebarActiveProjects",
    "chiefShowOnAllSpaces",
    "sidebarHideProjectsWithoutSessions",
    "groupWorkingSessions",
    "enableSessionParking",
    "pinnedSessionOrder",
    "unifiedWorkingSection",
    "unifiedGroupByProject",
    "unifiedProjectOrder",
    "sidebarSessionTagListItems",
];

const COORDINATOR_KEYS: &[&str] = &["orchestratorAllEfforts", "coordinatorBoardView"];

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
            .chain(APPEARANCE_WITHOUT_ROWS.iter().copied())
            .collect(),
        "sounds" => section("sounds"),
        "agents" => AGENT_DEFAULT_KEYS.to_vec(),
        "keybindings" => vec!["hotkeys"],
        "coordinator" => COORDINATOR_KEYS.to_vec(),
        "spaces" => SIDEBAR_ORGANIZATION_KEYS.to_vec(),
        _ => Vec::new(),
        super::ALWAYS_CATEGORY => ALWAYS_SYNCED_KEYS.to_vec(),
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
    if key == "coordinator/playbook/overrides" {
        return Some(format!(
            "The coordinator playbook overrides are the edit from {}, replacing the edit from {}.",
            kept.origin_name, lost.origin_name,
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

/// The setting's title in Settings, else the Help catalog's title for a value without a row of its
/// own (the custom colours' depths and some agent defaults), else its key.
fn setting_title(setting: &str) -> String {
    general_sections(Platform::current())
        .into_iter()
        .flat_map(|section| section.settings)
        .find(|row| row.key == setting)
        .map(|row| row.title)
        .or_else(|| {
            ghostex_settings_catalog::help::catalog()
                .settings
                .into_iter()
                .find(|entry| entry.key == setting)
                .map(|entry| entry.title)
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// What the Catppuccin Mocha preset saves (`theme_preset_patch` in the desktop's theme presets).
    fn mocha_preset_settings() -> Value {
        json!({
            "syncAppearance": true,
            "sidebarTheme": "dark-2",
            "darkThemePreset": "custom",
            "customSidebarTitlebarBackgroundTintColor": "#1c1c2c",
            "terminalGhosttyTheme": "Ghostex Catppuccin Mocha",
            "lightThemePreset": "custom",
            "customSidebarTitlebarLightBackgroundTintColor": "#eaecf2",
            "terminalGhosttyLightTheme": "Ghostex Catppuccin Latte",
            "themeSidebarContrast": -24,
            "themeWorkAreaContrast": -24,
            "customSidebarTitlebarBackgroundDarknessPercent": 85,
            "customSidebarTitlebarLightBackgroundLightnessPercent": 84,
            "terminalBackgroundMode": "theme",
        })
    }

    #[test]
    fn appearance_carries_the_custom_colour_depths_a_preset_saves() {
        let (values, defaults) =
            category_setting_values("appearance", Some(&mocha_preset_settings()));
        for (setting, depth) in [
            ("customSidebarTitlebarBackgroundDarknessPercent", 85),
            ("customSidebarTitlebarLightBackgroundLightnessPercent", 84),
        ] {
            let key = format!("appearance/setting/{setting}");
            assert_eq!(values.get(&key), Some(&json!(depth)), "{key}");
            assert!(!defaults.contains(&key), "{key} is not at its default");
            assert_eq!(setting_of_sync_key(&key), Some(setting));
        }
        // The values a computer keeps for itself still stay out.
        assert!(!values.contains_key("appearance/setting/interfaceScalePercent"));
    }

    #[test]
    fn unsaved_custom_colour_depths_read_as_their_defaults() {
        let (values, defaults) = category_setting_values("appearance", Some(&json!({})));
        for setting in APPEARANCE_WITHOUT_ROWS {
            let key = format!("appearance/setting/{setting}");
            let default = ghostex_settings_catalog::default_value(setting)
                .map(|default| default.to_json().to_value());
            assert_eq!(values.get(&key), default.as_ref(), "{key}");
            assert!(defaults.contains(&key), "{key}");
        }
    }

    #[test]
    fn a_conflict_on_a_custom_colour_depth_names_it_as_help_does() {
        let key = "appearance/setting/customSidebarTitlebarBackgroundDarknessPercent";
        let entry = |value: i64, origin: &str, name: &str| SyncEntry {
            key: key.to_string(),
            value: Some(json!(value)),
            hlc: format!("0000000000002-000000-{origin}"),
            parent: None,
            origin: origin.to_string(),
            origin_name: name.to_string(),
        };
        let note = describe_setting_key(&SyncConflict {
            id: key.to_string(),
            kept: entry(85, "S2b", "Studio"),
            lost: entry(96, "S1a", "Laptop"),
        });
        assert_eq!(
            note.as_deref(),
            Some(
                "Custom dark mode depth (set by Colourfulness) is 85 from Studio, replacing 96 \
                 from Laptop."
            )
        );
    }
}
