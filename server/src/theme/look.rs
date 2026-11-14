//! The look settings `ghostex theme` reads and changes: every setting on Settings > Interface an
//! agent may write, in the groups `ghostex theme read` shows them in, with the words each value is
//! shown in.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::settings::{self as catalog, CatalogEntry};

/// The Settings page the look settings are on (Settings > Interface).
const LOOK_TAB: &str = "theme";

/// The groups `ghostex theme read` lists the look settings in. A key in none of them is a
/// Transparency setting when it starts with `windowGlass`, else a Window one.
const GROUPS: &[(&str, &str, &[&str])] = &[
    (
        "colours",
        "Appearance and colours",
        &[
            "sidebarTheme",
            "darkThemePreset",
            "customSidebarTitlebarBackgroundTintColor",
            "customSidebarTitlebarBackgroundDarknessPercent",
            "lightThemePreset",
            "customSidebarTitlebarLightBackgroundTintColor",
            "customSidebarTitlebarLightBackgroundLightnessPercent",
            "themeSidebarContrast",
            "themeWorkAreaContrast",
        ],
    ),
    (
        "chatAndTerminal",
        "Chat and terminal colours",
        &[
            "sessionChatTheme",
            "sessionChatTextStrengthPercent",
            "sessionChatMutedThinking",
            "terminalColorScheme",
            "terminalGhosttyTheme",
            "terminalGhosttyLightTheme",
        ],
    ),
    (
        "fonts",
        "Fonts",
        &["sessionChatFontFamily", "terminalFontFamily"],
    ),
    (
        "size",
        "Size",
        &[
            "interfaceScalePercent",
            "interfaceScaleAreasSeparately",
            "agentManagerZoomPercent",
            "sessionChatZoomPercent",
            "filesZoomPercent",
            "terminalFontSize",
            "linuxDisplayScale",
        ],
    ),
    ("glass", "Transparency", &[]),
    ("window", "Window", &[]),
];

fn group_of(key: &str) -> &'static str {
    GROUPS
        .iter()
        .find(|(_, _, keys)| keys.contains(&key))
        .map(|(id, ..)| *id)
        .unwrap_or(if key.starts_with("windowGlass") {
            "glass"
        } else {
            "window"
        })
}

/// The look settings this computer shows, in catalog order.
pub(crate) fn look_entries(settings: &Map<String, Value>) -> Vec<&'static CatalogEntry> {
    catalog::catalog()
        .map(|catalog| {
            catalog
                .settings
                .iter()
                .filter(|entry| {
                    entry.tab == LOOK_TAB
                        && entry.agent_writable
                        && !catalog::hidden_here(entry, settings)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The look setting `key` names, or why it can't be changed with `ghostex theme`.
pub(crate) fn look_entry(
    key: &str,
    settings: &Map<String, Value>,
) -> Result<&'static CatalogEntry, String> {
    let entry = catalog::find_entry(key).map_err(|error| error.to_string())?;
    if entry.tab != LOOK_TAB {
        return Err(format!(
            "\"{}\" ({}) is not a look setting. Change it with `ghostex settings set {} <value>`.",
            entry.title, entry.key, entry.key
        ));
    }
    if !entry.agent_writable {
        return Err(catalog::not_writable_error(entry).to_string());
    }
    if catalog::hidden_here(entry, settings) {
        return Err(format!(
            "\"{}\" ({}) is not on this computer's Settings.",
            entry.title, entry.key
        ));
    }
    Ok(entry)
}

/// The value `key` has: the saved one, else its default.
pub(crate) fn current(settings: &Map<String, Value>, key: &str) -> Value {
    match settings.get(key) {
        Some(value) if !value.is_null() => value.clone(),
        _ => catalog::find_entry(key)
            .ok()
            .and_then(catalog::effective_default)
            .unwrap_or(Value::Null),
    }
}

/// `value` in the words Settings shows it in: an option's label, On or Off, a number as written.
pub(crate) fn shown(entry: &CatalogEntry, value: &Value) -> String {
    if let Some(label) = entry.options.as_ref().and_then(|options| {
        let raw = match value {
            Value::String(text) => text.clone(),
            other => catalog::format_value(other),
        };
        options
            .iter()
            .find(|option| option.value == raw)
            .map(|option| option.label.clone())
    }) {
        return label;
    }
    match value {
        Value::Bool(true) => "On".to_string(),
        Value::Bool(false) => "Off".to_string(),
        Value::String(text) if text.trim().is_empty() => "none".to_string(),
        other => catalog::format_value(other),
    }
}

/// One look setting as `ghostex theme read` lists it.
fn entry_json(entry: &CatalogEntry, settings: &Map<String, Value>) -> Value {
    let value = current(settings, &entry.key);
    let mut object = json!({
        "key": entry.key,
        "title": entry.title,
        "about": entry.subtitle,
        "type": catalog::type_summary(entry),
        "value": value,
        "shown": shown(entry, &value),
    });
    if let Some(default) = catalog::effective_default(entry) {
        object["default"] = default;
    }
    if let Some(options) = &entry.options {
        // A long option list (the terminal palettes) is named, not listed: `ghostex settings get`
        // prints every value.
        if options.len() <= 24 {
            object["options"] = Value::Array(
                options
                    .iter()
                    .map(|option| json!({ "label": option.label, "value": option.value }))
                    .collect(),
            );
        } else {
            object["optionCount"] = json!(options.len());
        }
    }
    object
}

/// Every look setting, grouped: `[{id, title, settings: [...]}]`.
pub(crate) fn grouped(settings: &Map<String, Value>) -> Value {
    let entries = look_entries(settings);
    Value::Array(
        GROUPS
            .iter()
            .filter_map(|(id, title, _)| {
                let rows: Vec<Value> = entries
                    .iter()
                    .filter(|entry| group_of(&entry.key) == *id)
                    .map(|entry| entry_json(entry, settings))
                    .collect();
                (!rows.is_empty()).then(|| json!({ "id": id, "title": title, "settings": rows }))
            })
            .collect(),
    )
}
