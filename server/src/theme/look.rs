//! The look settings `ghostex theme` reads and changes: every setting on Settings > Interface an
//! agent may write, in the groups `ghostex theme read` shows them in, with the words each value is
//! shown in.

use ghostex_settings_catalog::theme_roles::{
    appearance_key, format_role_value, parse_role_value, theme_role, ThemeRole, THEME_ROLES,
    THEME_ROLES_SETTING,
};
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

/// How a theme role is named in a change and the history: `themeRoles.<dark|light>.<role>`.
const ROLE_PATH_PREFIX: &str = "themeRoles.";

/// The appearance and role a `themeRoles.<dark|light>.<role>` path names.
fn role_path(key: &str) -> Option<(bool, &'static ThemeRole)> {
    let (appearance, role) = key.strip_prefix(ROLE_PATH_PREFIX)?.split_once('.')?;
    let light = match appearance {
        "dark" => false,
        "light" => true,
        _ => return None,
    };
    Some((light, theme_role(role)?))
}

/// The role paths a `set` key names, or `None` when it names no role: `dark.border` or
/// `light.border` one appearance, a bare `border` both.
pub(crate) fn role_targets(key: &str) -> Option<Result<Vec<String>, String>> {
    let key = key.trim().trim_start_matches(ROLE_PATH_PREFIX);
    let (appearances, role): (&[&str], &str) = match key.split_once('.') {
        Some(("dark", role)) => (&["dark"], role),
        Some(("light", role)) => (&["light"], role),
        Some((other, _)) => {
            return Some(Err(format!(
                "\"{other}\" is not an appearance: write dark.<role> or light.<role>."
            )))
        }
        None => (&["dark", "light"], key),
    };
    if theme_role(role).is_none() {
        if key.contains('.') {
            return Some(Err(format!(
                "\"{role}\" is not a theme colour. `ghostex theme read` lists them: {}.",
                THEME_ROLES
                    .iter()
                    .map(|role| role.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        return None;
    }
    Some(Ok(appearances
        .iter()
        .map(|appearance| format!("{ROLE_PATH_PREFIX}{appearance}.{role}"))
        .collect()))
}

/// "Borders in dark mode".
pub(crate) fn role_title(key: &str) -> String {
    role_path(key)
        .map(|(light, role)| {
            format!(
                "{} in {} mode",
                role.title,
                if light { "light" } else { "dark" }
            )
        })
        .unwrap_or_else(|| key.to_string())
}

/// A role's new value from what `set` was given: `default` (or `reset`) takes the role off.
pub(crate) fn role_value(raw: &str) -> Result<Value, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "default" | "reset" | "none" => Ok(Value::Null),
        _ => parse_role_value(raw).map(|value| Value::String(format_role_value(value))),
    }
}

/// A role's value as the history and the card show it.
pub(crate) fn role_shown(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| "default".to_string())
}

/// The settings patch that saves `values` (keys as changes name them): a role path goes into the
/// one `themeRoles` setting, taken off when its value is `null`.
pub(crate) fn patch_for<'a>(
    settings: &Map<String, Value>,
    values: impl IntoIterator<Item = (&'a str, &'a Value)>,
) -> Map<String, Value> {
    let mut patch = Map::new();
    let mut roles: Option<Map<String, Value>> = None;
    for (key, value) in values {
        let Some((light, role)) = role_path(key) else {
            patch.insert(key.to_string(), value.clone());
            continue;
        };
        let roles = roles.get_or_insert_with(|| {
            settings
                .get(THEME_ROLES_SETTING)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        });
        let appearance = roles
            .entry(appearance_key(light))
            .or_insert_with(|| Value::Object(Map::new()));
        if !appearance.is_object() {
            *appearance = Value::Object(Map::new());
        }
        if let Some(appearance) = appearance.as_object_mut() {
            if value.is_null() {
                appearance.remove(role.id);
            } else {
                appearance.insert(role.id.to_string(), value.clone());
            }
        }
    }
    if let Some(mut roles) = roles {
        roles.retain(|_, appearance| appearance.as_object().is_none_or(|map| !map.is_empty()));
        patch.insert(THEME_ROLES_SETTING.to_string(), Value::Object(roles));
    }
    patch
}

/// The value `key` has: the saved one, else its default. A role path reads that role, `null`
/// while it is not set.
pub(crate) fn current(settings: &Map<String, Value>, key: &str) -> Value {
    if let Some((light, role)) = role_path(key) {
        return ghostex_settings_catalog::theme_roles::saved_role(settings, light, role.id)
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null);
    }
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

/// The theme colours as `ghostex theme read` lists them: each role with its dark and light value
/// (`null` while it keeps the theme's own).
fn roles_group(settings: &Map<String, Value>) -> Value {
    let rows: Vec<Value> = THEME_ROLES
        .iter()
        .map(|role| {
            json!({
                "key": role.id,
                "title": role.title,
                "about": role.about,
                "dark": current(settings, &format!("{ROLE_PATH_PREFIX}dark.{}", role.id)),
                "light": current(settings, &format!("{ROLE_PATH_PREFIX}light.{}", role.id)),
            })
        })
        .collect();
    json!({
        "id": "roles",
        "title": "Theme colours",
        "about": "Each one an opacity of the text colour over what is under it (4%), which stays see-through on glass, or a colour (#7dd3fc, or #7dd3fc80 with its opacity); default keeps the theme's own.",
        "roles": rows,
    })
}

/// Every look setting, grouped: `[{id, title, settings: [...]}]`, then the theme colours
/// (`{id: "roles", roles: [...]}`).
pub(crate) fn grouped(settings: &Map<String, Value>) -> Value {
    let entries = look_entries(settings);
    let mut groups: Vec<Value> = GROUPS
        .iter()
        .filter_map(|(id, title, _)| {
            let rows: Vec<Value> = entries
                .iter()
                .filter(|entry| group_of(&entry.key) == *id)
                .map(|entry| entry_json(entry, settings))
                .collect();
            (!rows.is_empty()).then(|| json!({ "id": id, "title": title, "settings": rows }))
        })
        .collect();
    groups.push(roles_group(settings));
    Value::Array(groups)
}
