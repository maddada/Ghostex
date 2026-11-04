use std::{path::PathBuf, sync::OnceLock};

use super::*;
use serde_json::{Map, Value};

static GHOSTEX_STORAGE_PATHS: OnceLock<ghostex_paths::GhostexPaths> = OnceLock::new();

pub fn ghostex_storage_paths() -> &'static ghostex_paths::GhostexPaths {
    GHOSTEX_STORAGE_PATHS.get_or_init(|| {
        let paths = ghostex_paths::GhostexPaths::resolve();
        if let Some(warning) = paths.migrate_legacy_layout().warning() {
            eprintln!("{warning}");
        }
        paths
    })
}

pub fn shared_sidebar_settings_path() -> PathBuf {
    ghostex_storage_paths().sidebar_settings_file()
}

pub fn normalize_project_editor_auto_sleep_idle_minutes(value: Option<f32>) -> f64 {
    match value.map(f64::from).filter(|minutes| minutes.is_finite()) {
        Some(0.0) => 0.0,
        Some(minutes) if minutes > 0.0 => minutes.min(PROJECT_EDITOR_AUTO_SLEEP_MAX_IDLE_MINUTES),
        _ => PROJECT_EDITOR_AUTO_SLEEP_DEFAULT_IDLE_MINUTES,
    }
}

pub fn normalize_terminal_font_size(value: Option<f32>) -> f32 {
    value
        .filter(|font_size| font_size.is_finite())
        .map(|font_size| font_size.clamp(MIN_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE))
        .unwrap_or(DEFAULT_TERMINAL_FONT_SIZE)
}

pub fn normalize_default_prompt_agent_id(value: Option<&str>) -> String {
    let normalized = value
        .unwrap_or("")
        .trim()
        .chars()
        .take(MAX_DEFAULT_PROMPT_AGENT_ID_LEN)
        .collect::<String>();
    if normalized.is_empty() {
        DEFAULT_PROMPT_AGENT_ID.to_string()
    } else {
        normalized
    }
}

pub fn normalize_default_editor_command(value: Option<&str>) -> SharedDefaultEditorCommand {
    match value.unwrap_or(DEFAULT_DEFAULT_EDITOR_COMMAND) {
        "code-insiders" => SharedDefaultEditorCommand::CodeInsiders,
        "zed" => SharedDefaultEditorCommand::Zed,
        "zeditor" => SharedDefaultEditorCommand::Zeditor,
        "cursor" => SharedDefaultEditorCommand::Cursor,
        "windsurf" => SharedDefaultEditorCommand::Windsurf,
        "codium" => SharedDefaultEditorCommand::Codium,
        "subl" => SharedDefaultEditorCommand::Subl,
        "other" => SharedDefaultEditorCommand::Other,
        _ => SharedDefaultEditorCommand::Code,
    }
}

pub fn normalize_custom_default_editor_command(value: Option<&str>) -> String {
    value
        .unwrap_or("")
        .trim()
        .chars()
        .take(MAX_CUSTOM_DEFAULT_EDITOR_COMMAND_CHARS)
        .collect()
}

pub(crate) fn read_bool_field(object: &Map<String, Value>, key: &str, fallback: bool) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

pub(crate) fn read_finite_number_field(
    object: &Map<String, Value>,
    key: &str,
    fallback: f64,
) -> f64 {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

pub(crate) fn read_string_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

pub(crate) fn normalize_terminal_cursor_style(value: &str) -> String {
    match value {
        "block" | "underline" => value.to_string(),
        _ => DEFAULT_TERMINAL_CURSOR_STYLE.to_string(),
    }
}

pub(crate) fn normalize_ghostty_font_family(value: &str) -> String {
    value.trim().to_string()
}

pub(crate) fn normalize_ghostty_theme(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == GHOSTTY_THEME_UNMANAGED_SENTINEL {
        String::new()
    } else {
        trimmed.to_string()
    }
}

/// SEE-ALSO: `normalizeTerminalBackgroundMode` in packages/shared/ghostex-settings/normalize-fields.ts (deleted 2026-10-01), whose
/// migration this mirrors: a file without a mode keeps its custom colour, otherwise it gets the
/// Black / white default.
pub(crate) fn terminal_background(
    object: &serde_json::Map<String, serde_json::Value>,
) -> SharedTerminalBackground {
    let custom = normalize_terminal_background_rgb(read_string_field(
        object,
        "workspaceBackgroundColor",
        DEFAULT_TERMINAL_BACKGROUND_COLOR,
    ));
    match read_string_field(object, "terminalBackgroundMode", "") {
        "theme" => SharedTerminalBackground::Theme,
        "custom" => custom.map_or(
            SharedTerminalBackground::Theme,
            SharedTerminalBackground::Custom,
        ),
        "pure" => SharedTerminalBackground::Pure,
        _ => custom.map_or(
            SharedTerminalBackground::Pure,
            SharedTerminalBackground::Custom,
        ),
    }
}

/// `None` is unset: an empty value, or the retired default #010101 that every settings
/// file carried (the same migration as `normalizeTerminalBackgroundSetting` in TS).
fn normalize_terminal_background_rgb(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("#010101") {
        return None;
    }
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

pub(crate) fn normalize_terminal_background_image_fit(value: &str) -> String {
    match value.trim() {
        "contain" | "stretch" | "natural" => value.trim().to_string(),
        _ => DEFAULT_TERMINAL_BACKGROUND_IMAGE_FIT.to_string(),
    }
}

pub(crate) fn normalize_ghostty_copy_on_select(value: &str) -> String {
    match value {
        "true" | "clipboard" => value.to_string(),
        _ => DEFAULT_TERMINAL_COPY_ON_SELECT.to_string(),
    }
}

pub(crate) fn normalize_ghostty_confirm_close_surface(value: &str) -> String {
    match value {
        "false" | "true" | "always" => value.to_string(),
        _ => DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE.to_string(),
    }
}

pub(crate) fn normalize_ghostty_scrollbar(value: &str) -> String {
    if value == "never" {
        "never".to_string()
    } else {
        DEFAULT_TERMINAL_SCROLLBAR.to_string()
    }
}

pub(crate) fn format_ghostty_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

pub(crate) fn format_ghostty_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(crate) fn format_ghostty_number(value: f64) -> String {
    if value.round() == value {
        return (value as i64).to_string();
    }
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

pub(crate) fn format_ghostty_percent(value: f64) -> String {
    format!("{}%", format_ghostty_number(value * 100.0))
}

pub(crate) fn insert_string(object: &mut Map<String, Value>, key: &str, value: &str) {
    object.insert(key.to_string(), Value::String(value.to_string()));
}

pub(crate) fn insert_bool(object: &mut Map<String, Value>, key: &str, value: bool) {
    object.insert(key.to_string(), Value::Bool(value));
}

pub(crate) fn insert_number(object: &mut Map<String, Value>, key: &str, value: f64) {
    let number = serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0));
    object.insert(key.to_string(), Value::Number(number));
}

pub(crate) fn strict_bool_field(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key)?.as_bool()
}

/// Where `kind` opened under the settings that came before `documentFileOpenTarget`.
pub fn legacy_document_file_open_target(
    object: &Map<String, Value>,
    kind: SharedDocumentFileKind,
) -> SharedFileOpenTarget {
    match ghostex_settings_catalog::file_opening::legacy_document_file_open_target(object, kind) {
        "code" => SharedFileOpenTarget::Code,
        "system-app" => SharedFileOpenTarget::SystemApp,
        _ => SharedFileOpenTarget::Files,
    }
}

pub fn web_links_open_in_app_from_object(object: &Map<String, Value>) -> bool {
    if let Some(target) = object.get("webLinkOpenTarget").and_then(Value::as_str) {
        match target {
            "internal-browser" => return true,
            "system-default-browser" => return false,
            _ => {}
        }
    }
    if let Some(open_in_app) = strict_bool_field(object, "openTerminalLinksInApp") {
        return open_in_app;
    }
    match object
        .get("terminalDevServerOpenTarget")
        .and_then(Value::as_str)
    {
        Some("internal-browser") => true,
        Some("system-default-browser") => false,
        _ => DEFAULT_WEB_LINKS_OPEN_IN_APP,
    }
}

pub(crate) fn normalize_keep_awake_duration_minutes(
    value: Option<&Value>,
) -> SharedKeepAwakeDurationMinutes {
    let Some(minutes) = value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
    else {
        return DEFAULT_KEEP_AWAKE_DURATION_MINUTES;
    };
    if (minutes - 0.0).abs() < f64::EPSILON {
        SharedKeepAwakeDurationMinutes::UntilTurnedOff
    } else if (minutes - 120.0).abs() < f64::EPSILON {
        SharedKeepAwakeDurationMinutes::TwoHours
    } else if (minutes - 300.0).abs() < f64::EPSILON {
        SharedKeepAwakeDurationMinutes::FiveHours
    } else {
        DEFAULT_KEEP_AWAKE_DURATION_MINUTES
    }
}

pub(crate) fn normalize_keep_awake_battery_threshold_percent(value: Option<&Value>) -> f64 {
    let Some(percent) = value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
    else {
        return DEFAULT_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT;
    };
    if percent == 0.0 {
        return 0.0;
    }
    percent.clamp(
        MIN_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT,
        MAX_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT,
    )
}

pub(crate) fn json_value_to_f32(value: &Value) -> Option<f32> {
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number as f32)
}

pub(crate) fn json_number_value_to_f32(value: &Value) -> Option<f32> {
    let number = value.as_f64()?;
    number.is_finite().then_some(number as f32)
}

/// Normalizes an `agentboxDefaultLocation` (or `/api/createAgentSession` `runLocation`) value:
/// `"local"`, `"agentbox:<provider>"` for a known provider, or `"agentbox:docker:<alias>"` for a
/// registered remote Docker server. Anything else is `"local"`, the same rule as
/// `normalizeAgentboxDefaultLocation` in `packages/shared/ghostex-settings/normalize-fields.ts` (deleted 2026-10-01).
pub fn normalize_agentbox_location(value: Option<&str>) -> String {
    let value = value.unwrap_or("").trim();
    let Some(provider) = value.strip_prefix("agentbox:") else {
        return DEFAULT_AGENTBOX_LOCATION.to_string();
    };
    if AGENTBOX_PROVIDER_IDS.contains(&provider) {
        return value.to_string();
    }
    match provider.strip_prefix("docker:") {
        Some(alias) if is_valid_agentbox_remote_docker_alias(alias) => value.to_string(),
        _ => DEFAULT_AGENTBOX_LOCATION.to_string(),
    }
}

/// A remote Docker alias agentbox accepts (`agentbox remote-docker add <alias> <ssh>`): letters,
/// digits, `.`, `_` and `-`.
pub fn is_valid_agentbox_remote_docker_alias(alias: &str) -> bool {
    !alias.is_empty()
        && alias.len() <= 64
        && alias
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

/// The dark-mode glass keys and their light-mode twins that Same for light and dark folds into one
/// (CDXC:Settings 2026-10-29 DECISION, cleanup Q19, in packages/settings-catalog/src/general/theme.rs).
pub const WINDOW_GLASS_LIGHT_DARK_PAIRS: [(&str, &str); 5] = [
    ("windowGlassImageDark", "windowGlassImageLight"),
    ("windowGlassVideoDark", "windowGlassVideoLight"),
    ("windowGlassLiveStyleDark", "windowGlassLiveStyleLight"),
    (
        "windowGlassSidebarOpacityDark",
        "windowGlassSidebarOpacityLight",
    ),
    (
        "windowGlassWorkAreaTintDark",
        "windowGlassWorkAreaTintLight",
    ),
];

/// Whether light mode uses the dark-mode picture, video, Live background and tints
/// (`windowGlassSameForLightAndDark`). A settings file that never saw the switch counts as on
/// unless its two sides were already set apart: a pair is "the same" when both values are equal or
/// neither was ever saved, so a fresh install starts with one set of rows and a file whose light
/// mode was tuned on purpose keeps both sets.
pub fn window_glass_same_for_light_and_dark(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    if let Some(same) = object
        .get("windowGlassSameForLightAndDark")
        .and_then(serde_json::Value::as_bool)
    {
        return same;
    }
    WINDOW_GLASS_LIGHT_DARK_PAIRS.iter().all(|(dark, light)| {
        match (object.get(*dark), object.get(*light)) {
            (None, None) => true,
            (dark, light) => dark == light,
        }
    })
}

/// The key light mode reads for `light`: its dark twin while Same for light and dark is on.
pub fn window_glass_light_key<'a>(
    object: &serde_json::Map<String, serde_json::Value>,
    light: &'a str,
) -> &'a str {
    if !window_glass_same_for_light_and_dark(object) {
        return light;
    }
    WINDOW_GLASS_LIGHT_DARK_PAIRS
        .iter()
        .find(|(_, twin)| *twin == light)
        .map_or(light, |(dark, _)| *dark)
}
