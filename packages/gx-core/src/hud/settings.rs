//! The HUD's `settings` block: the saved settings object with every key a Rust reader takes from
//! `hud.settings` normalized the way `normalizeghostexSettings` normalizes it
//! (packages/shared/ghostex-settings/normalize.ts), and the two runtime flags pinned.
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! The runtime's HUD carried the whole normalized settings object. Porting the full 1.4k-line
//! normalizer would be the wrong trade while the Settings page that writes the file stays
//! TypeScript, so the keys Rust READS are normalized here, with the TypeScript default and clamp
//! for each, and every other key rides along as it is saved. A reader that starts taking another
//! key from `hud.settings` adds it to [`NORMALIZED_KEYS`] with the default and clamp `normalize.ts`
//! gives it.

use serde_json::{Map, Value};

/// The keys normalized here.
pub const NORMALIZED_KEYS: [&str; 13] = [
    "sidebarTheme",
    "sidebarTooltipDelayMs",
    "sidebarCollapseAnimationDurationMs",
    "showProjectIcons",
    "hideProjectHeaderDiffStats",
    "showProjectEditorDiffFileCount",
    "renameSessionOnDoubleClick",
    "createSessionOnSidebarDoubleClick",
    "hideBrowserFaviconUntilHover",
    "hideSessionAgentIconUntilHover",
    "sidebarSessionCycleSkipsSleeping",
    "agentManagerZoomPercent",
    "defaultPromptAgentId",
];

fn boolean(source: &Map<String, Value>, key: &str, fallback: bool) -> Value {
    Value::Bool(source.get(key).and_then(Value::as_bool).unwrap_or(fallback))
}

fn number(source: &Map<String, Value>, key: &str, fallback: f64) -> f64 {
    source.get(key).and_then(Value::as_f64).unwrap_or(fallback)
}

/// `Math.round`: halves go up, towards positive infinity.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// A whole number stays an integer on the wire, as `JSON.stringify` writes it.
fn whole(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

/// `clampSidebarThemeSetting(readString(source, 'sidebarTheme', 'system'))`.
pub(crate) fn sidebar_theme_setting(source: &Map<String, Value>) -> &'static str {
    match source.get("sidebarTheme").and_then(Value::as_str) {
        None | Some("system") => "system",
        Some("plain-light") => "plain-light",
        Some(_) => "dark-2",
    }
}

/// `resolveSidebarTheme(setting, 'dark')`.
pub(crate) fn resolved_dark_theme(setting: &str) -> &str {
    match setting {
        "system" | "auto" | "plain" => "dark-2",
        other => other,
    }
}

/// `clampAgentManagerZoomPercent(readNumber(...))`.
pub(crate) fn agent_manager_zoom_percent(source: &Map<String, Value>) -> Value {
    let value = number(source, "agentManagerZoomPercent", 100.0);
    whole(js_round(value).clamp(50.0, 200.0))
}

/// The HUD's `settings` value.
pub(crate) fn hud_settings(raw: &Value, debugging_mode: bool, show_beta_features: bool) -> Value {
    let empty = Map::new();
    let source = raw.as_object().unwrap_or(&empty);
    let mut settings = source.clone();
    settings.insert(
        "sidebarTheme".into(),
        Value::from(sidebar_theme_setting(source)),
    );
    // `clampSidebarTooltipDelayMs`: 0..2000 in steps of 100, 600 when not finite.
    let tooltip = number(source, "sidebarTooltipDelayMs", 600.0);
    settings.insert(
        "sidebarTooltipDelayMs".into(),
        whole(js_round(tooltip.clamp(0.0, 2000.0) / 100.0) * 100.0),
    );
    // `clampSidebarCollapseAnimationDurationMs`: 0..1000 in steps of 100.
    let collapse = number(source, "sidebarCollapseAnimationDurationMs", 400.0);
    settings.insert(
        "sidebarCollapseAnimationDurationMs".into(),
        whole(js_round(collapse.clamp(0.0, 1000.0) / 100.0) * 100.0),
    );
    for (key, fallback) in [
        ("showProjectIcons", true),
        ("hideProjectHeaderDiffStats", false),
        ("showProjectEditorDiffFileCount", false),
        ("renameSessionOnDoubleClick", false),
        ("createSessionOnSidebarDoubleClick", false),
        ("hideBrowserFaviconUntilHover", false),
        ("hideSessionAgentIconUntilHover", false),
        ("sidebarSessionCycleSkipsSleeping", false),
    ] {
        settings.insert(key.into(), boolean(source, key, fallback));
    }
    settings.insert(
        "agentManagerZoomPercent".into(),
        agent_manager_zoom_percent(source),
    );
    // `normalizeDefaultPromptAgentId`: trimmed, `codex` when empty, at most 120 characters.
    let prompt_agent = source
        .get("defaultPromptAgentId")
        .and_then(Value::as_str)
        .unwrap_or("codex")
        .trim();
    let prompt_agent = if prompt_agent.is_empty() {
        "codex"
    } else {
        prompt_agent
    };
    settings.insert(
        "defaultPromptAgentId".into(),
        Value::from(prompt_agent.chars().take(120).collect::<String>()),
    );
    settings.insert("debuggingMode".into(), Value::Bool(debugging_mode));
    settings.insert("showBetaFeatures".into(), Value::Bool(show_beta_features));
    Value::Object(settings)
}
