//! Settings export and import: a JSON file that carries the user's Ghostex settings from one
//! computer to another (Settings > Sync, `ghostex settings export` / `import`).
//!
//! The file carries the settings gxserver's API may write (the settings catalog's `agentWritable`
//! rule, which already leaves out tokens, keys, passwords, the Accounts and Remote pages, structured
//! values and app state), plus the keyboard shortcut map, minus what only makes sense on the
//! computer that wrote it. An import runs the same rule again, so a hand-edited file can carry
//! nothing more than an export would.
//!
//! CDXC:SettingsSync 2026-11-14 DECISION:
//! User: "can we please also in syncing page add export and import json for ghostex? this is to transfer settings between machines if someone needs". Export saves every transferable setting's current value; import shows how many change before it saves them, skips keys this Ghostex doesn't know (counted, for a file from a newer Ghostex) and values of the wrong type, and refuses a file that isn't a Ghostex settings export.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/sync_transfer.rs (the Sync page rows and the confirm), server/src/ghostex_cli/settings_transfer.rs (the CLI verbs), server/src/settings_agent_writable.rs (the API rule this reuses).

use std::collections::BTreeSet;
use std::sync::OnceLock;

use ghostex_settings_catalog::availability::{default_value_on, platform_defaults};
use ghostex_settings_catalog::help::{api_may_write, catalog, ValueType};
use ghostex_settings_catalog::{Platform, ToJson};
use serde_json::{json, Map, Value};

use crate::renderer_commands::settings_patch_value_allowed;
use crate::settings_sync::{APPEARANCE_LEFT_OUT, SYNC_CATEGORIES};

/// The file's `format`: what tells a settings export from any other JSON file.
pub const SETTINGS_EXPORT_FORMAT: &str = "ghostex-settings";
/// The file's `formatVersion`. A newer file still imports: its unknown keys are skipped.
pub const SETTINGS_EXPORT_FORMAT_VERSION: u64 = 1;

/// The structured value the file carries besides plain values: the keyboard shortcut map, between
/// computers of the same system (as settings sync does).
const HOTKEYS_KEY: &str = "hotkeys";

/// CDXC:SettingsSync 2026-11-14 WHY:
/// Settings that only make sense on the computer that wrote them stay out of the file, besides the ones settings sync already keeps on each computer (`APPEARANCE_LEFT_OUT`: the glass picture and video, the interface scale and zoom): folders and files on that computer, the microphone and WSL distribution it has, each computer's own sync switches (Settings > Sync says each has its own), automatic folder trust (a security choice made on each computer; the API refuses turning it on anyway) and Start boxes from your phone (it copies keys to that computer's paired phones).
const LEFT_ON_EACH_COMPUTER: &[&str] = &[
    "terminalBackgroundImage",
    "defaultProjectsFolder",
    "manageAdditionalDocsFolders",
    "globalBeadsDirectory",
    "globalDocsDirectory",
    "windowsWslDistribution",
    "dictateMicrophone",
    "trustProjectFoldersAutomatically",
    "cloudBoxesPhoneStartEnabled",
];

/// One setting the file may carry.
#[derive(Clone, Debug)]
pub struct TransferSetting {
    pub key: String,
    pub title: String,
    value_type: ValueType,
    /// Its choices' values with their labels in Settings.
    options: Vec<(String, String)>,
    range: Option<(f64, f64)>,
}

/// Whether the settings catalog lists `key` but the file leaves it out on purpose (a secret,
/// account or pairing state, app state, or something of one computer).
fn left_out_on_purpose(key: &str) -> bool {
    LEFT_ON_EACH_COMPUTER.contains(&key)
        || APPEARANCE_LEFT_OUT.contains(&key)
        || SYNC_CATEGORIES
            .iter()
            .any(|category| category.setting_key == key)
}

/// The settings the file carries, in Settings order.
pub fn transfer_settings() -> &'static [TransferSetting] {
    static SETTINGS: OnceLock<Vec<TransferSetting>> = OnceLock::new();
    SETTINGS.get_or_init(|| {
        catalog()
            .settings
            .into_iter()
            .filter(|entry| {
                let plain = !matches!(entry.value_type, ValueType::Json | ValueType::Ui)
                    && api_may_write(&entry.key, entry.agent_writable);
                (plain || entry.key == HOTKEYS_KEY) && !left_out_on_purpose(&entry.key)
            })
            .map(|entry| TransferSetting {
                options: entry
                    .options
                    .iter()
                    .map(|option| (option.value.clone(), option.label.clone()))
                    .collect(),
                range: entry.range.map(|range| (range.min, range.max)),
                key: entry.key,
                title: entry.title,
                value_type: entry.value_type,
            })
            .collect()
    })
}

fn transfer_setting(key: &str) -> Option<&'static TransferSetting> {
    transfer_settings()
        .iter()
        .find(|setting| setting.key == key)
}

/// Every key the settings catalog knows, for telling a key left out on purpose from one this
/// Ghostex has never heard of.
fn catalog_keys() -> &'static BTreeSet<String> {
    static KEYS: OnceLock<BTreeSet<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        catalog()
            .settings
            .into_iter()
            .map(|entry| entry.key)
            .collect()
    })
}

/// The value of `key` on this computer: the saved one, else this system's default.
fn current_value(settings: &Map<String, Value>, key: &str) -> Option<Value> {
    settings.get(key).cloned().or_else(|| {
        default_value_on(Platform::current(), key).map(|default| default.to_json().to_value())
    })
}

/// The system as the file names it.
pub fn platform_id(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

/// Who wrote the file: the Ghostex version, the computer's name and the time (the host's clock;
/// this crate reads none).
pub struct ExportSource<'a> {
    pub ghostex_version: &'a str,
    pub computer_name: &'a str,
    /// RFC 3339.
    pub exported_at: &'a str,
}

/// The export file for a computer whose settings file holds `settings`: every transferable
/// setting's current value (the saved one, else the default), and the shortcut map when one is
/// saved.
pub fn export_settings_document(settings: &Map<String, Value>, source: &ExportSource) -> Value {
    let mut values = Map::new();
    for setting in transfer_settings() {
        let Some(value) = current_value(settings, &setting.key) else {
            continue;
        };
        // Only what an import can save (an empty shortcut map is the same as none).
        if !settings_patch_value_allowed(&setting.key, &value)
            || value.as_object().is_some_and(Map::is_empty)
        {
            continue;
        }
        values.insert(setting.key.clone(), value);
    }
    json!({
        "format": SETTINGS_EXPORT_FORMAT,
        "formatVersion": SETTINGS_EXPORT_FORMAT_VERSION,
        "ghostexVersion": source.ghostex_version,
        "exportedAt": source.exported_at,
        "computer": source.computer_name,
        "platform": platform_id(Platform::current()),
        "settings": values,
    })
}

/// `ghostex-settings-<computer>-<date>.json`, with the computer's name reduced to letters, digits
/// and dashes.
pub fn export_file_name(computer_name: &str, date: &str) -> String {
    let mut slug = String::new();
    for character in computer_name.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        format!("ghostex-settings-{date}.json")
    } else {
        format!("ghostex-settings-{slug}-{date}.json")
    }
}

/// One setting an import changes.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportChange {
    pub key: String,
    pub title: String,
    pub from: Option<Value>,
    pub to: Value,
}

/// What importing a file would do on this computer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportPlan {
    pub changes: Vec<ImportChange>,
    /// Settings the file has at the value this computer already has.
    pub unchanged: usize,
    /// Keys this Ghostex doesn't know (a file from a newer Ghostex).
    pub unknown: Vec<String>,
    /// Values of the wrong type or outside what the setting takes.
    pub wrong_type: Vec<String>,
    /// Keys an export leaves out (secrets, account state, things of one computer).
    pub left_out: Vec<String>,
    /// Settings that differ between systems (the keyboard shortcuts), from a file written on
    /// another system.
    pub other_system: Vec<String>,
    pub from_computer: Option<String>,
    pub from_version: Option<String>,
    pub from_platform: Option<String>,
}

/// Why a file can't be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    NotJson,
    NotAnExport,
}

impl ImportError {
    pub fn message(&self) -> &'static str {
        match self {
            ImportError::NotJson | ImportError::NotAnExport => {
                "This file isn't a Ghostex settings export. Pick a file saved with Export settings."
            }
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

/// Whether `value` is something setting `setting` takes.
fn value_fits(setting: &TransferSetting, value: &Value) -> bool {
    if !settings_patch_value_allowed(&setting.key, value) {
        return false;
    }
    match setting.value_type {
        ValueType::Boolean => value.is_boolean(),
        ValueType::String => value.is_string(),
        ValueType::Enum => value.as_str().is_some_and(|text| {
            setting.options.is_empty() || setting.options.iter().any(|(option, _)| option == text)
        }),
        ValueType::Number => value.as_f64().is_some_and(|number| {
            number.is_finite()
                && setting
                    .range
                    .is_none_or(|(min, max)| number >= min && number <= max)
        }),
        ValueType::Json => value.is_object(),
        ValueType::Ui => false,
    }
}

fn same_value(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left == right,
        _ => left == right,
    }
}

/// Reads an export file's text and works out what importing it changes on a computer whose
/// settings file holds `settings`.
pub fn plan_settings_import(
    text: &str,
    settings: &Map<String, Value>,
) -> Result<ImportPlan, ImportError> {
    let document: Value = serde_json::from_str(text).map_err(|_| ImportError::NotJson)?;
    let document = document.as_object().ok_or(ImportError::NotAnExport)?;
    if document.get("format").and_then(Value::as_str) != Some(SETTINGS_EXPORT_FORMAT) {
        return Err(ImportError::NotAnExport);
    }
    let values = document
        .get("settings")
        .and_then(Value::as_object)
        .ok_or(ImportError::NotAnExport)?;
    let text_field = |key: &str| {
        document
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let from_platform = text_field("platform");
    let same_system = from_platform
        .as_deref()
        .is_none_or(|platform| platform == platform_id(Platform::current()));
    let mut plan = ImportPlan {
        from_computer: text_field("computer"),
        from_version: text_field("ghostexVersion"),
        from_platform,
        ..ImportPlan::default()
    };
    for (key, value) in values {
        let Some(setting) = transfer_setting(key) else {
            if catalog_keys().contains(key) {
                plan.left_out.push(key.clone());
            } else {
                plan.unknown.push(key.clone());
            }
            continue;
        };
        // Shortcuts and the settings whose default differs by system stay with their system.
        if !same_system && (key == HOTKEYS_KEY || !platform_defaults(key).is_empty()) {
            plan.other_system.push(key.clone());
            continue;
        }
        if !value_fits(setting, value) {
            plan.wrong_type.push(key.clone());
            continue;
        }
        let from = current_value(settings, key);
        if from.as_ref().is_some_and(|from| same_value(from, value)) {
            plan.unchanged += 1;
            continue;
        }
        plan.changes.push(ImportChange {
            key: key.clone(),
            title: setting.title.clone(),
            from,
            to: value.clone(),
        });
    }
    // Settings order, so the list reads like the Settings window.
    let order = |key: &str| {
        transfer_settings()
            .iter()
            .position(|setting| setting.key == key)
            .unwrap_or(usize::MAX)
    };
    plan.changes.sort_by_key(|change| order(&change.key));
    Ok(plan)
}

impl ImportPlan {
    /// The settings patch that applies the plan.
    pub fn patch(&self) -> Map<String, Value> {
        self.changes
            .iter()
            .map(|change| (change.key.clone(), change.to.clone()))
            .collect()
    }

    /// How many settings the import skips.
    pub fn skipped(&self) -> usize {
        self.unknown.len() + self.wrong_type.len() + self.left_out.len() + self.other_system.len()
    }

    /// One plain sentence per kind of skipped setting, for the confirm and the CLI.
    pub fn skipped_notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        let count = |count: usize| {
            if count == 1 {
                "1 setting".to_string()
            } else {
                format!("{count} settings")
            }
        };
        // "that stays" for one setting, "that stay" for several.
        let verb = |count: usize, verb: &str| {
            if count == 1 {
                format!("{verb}s")
            } else {
                verb.to_string()
            }
        };
        if !self.unknown.is_empty() {
            notes.push(format!(
                "Skipped {}: not in this version of Ghostex.",
                count(self.unknown.len())
            ));
        }
        if !self.wrong_type.is_empty() {
            notes.push(format!(
                "Skipped {} with a value this version of Ghostex can't use.",
                count(self.wrong_type.len())
            ));
        }
        if !self.left_out.is_empty() {
            notes.push(format!(
                "Skipped {} that {} on each computer (sign-ins, keys, folders).",
                count(self.left_out.len()),
                verb(self.left_out.len(), "stay")
            ));
        }
        if !self.other_system.is_empty() {
            notes.push(format!(
                "Skipped {} that {} between systems (like keyboard shortcuts): the file comes from another system.",
                count(self.other_system.len()),
                verb(self.other_system.len(), "differ")
            ));
        }
        notes
    }
}

/// Setting `key`'s value as the confirm lists it: a choice by its label in Settings.
pub fn describe_transfer_value(key: &str, value: Option<&Value>) -> String {
    let label = value.and_then(Value::as_str).and_then(|text| {
        transfer_setting(key)?
            .options
            .iter()
            .find(|(option, _)| option == text)
            .map(|(_, label)| label.clone())
    });
    if let Some(label) = label {
        return label;
    }
    match value {
        None | Some(Value::Null) => "default".to_string(),
        Some(Value::Bool(true)) => "on".to_string(),
        Some(Value::Bool(false)) => "off".to_string(),
        Some(Value::String(text)) if text.is_empty() => "empty".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Object(_)) | Some(Value::Array(_)) => "a different set".to_string(),
    }
}
