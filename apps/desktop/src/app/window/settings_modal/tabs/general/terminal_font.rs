//! `normalizeGhosttyFontFamily` (packages/shared/ghostex-settings/normalize-fields.ts (deleted 2026-10-01)) over the generated
//! `TERMINAL_FONT_PRESETS` table (packages/shared/terminal-font-preset.ts (deleted 2026-10-01)): a legacy preset name
//! saves as the Ghostty family it stood for, anything else is saved trimmed.
use super::super::super::catalog::{module, settings_catalog};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// `normalizeComparableValue`: trimmed, runs of whitespace as one space, one leading and one
/// trailing quote dropped, lower case.
fn comparable(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let without_leading = collapsed.strip_prefix(['\'', '"']).unwrap_or(&collapsed);
    let without_trailing = without_leading
        .strip_suffix(['\'', '"'])
        .unwrap_or(without_leading);
    without_trailing.to_lowercase()
}

/// `TERMINAL_FONT_PRESETS` as `(preset, fontFamily)`.
fn presets() -> &'static [(String, String)] {
    static PRESETS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        settings_catalog()
            .module_value(module::TERMINAL_FONT_PRESET, "TERMINAL_FONT_PRESETS")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        Some((
                            row.get("preset")?.as_str()?.to_string(),
                            row.get("fontFamily")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// `TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE`: each preset by its name and by its family stack,
/// then the four short aliases the module adds by hand.
fn preset_by_comparable() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for (preset, font_family) in presets() {
            map.insert(comparable(preset), preset.clone());
            map.insert(comparable(font_family), preset.clone());
        }
        for (alias, preset) in [
            ("ui-monospace", "UI Monospace"),
            ("consolas", "Consolas (Windows Default)"),
            ("monaco", "Monaco"),
            ("droid sans mono", "Droid Sans Mono (Linux Default)"),
        ] {
            map.insert(alias.to_string(), preset.to_string());
        }
        map
    })
}

/// `normalizeTerminalFontPreset`: the preset a value names, else `DEFAULT_TERMINAL_FONT_PRESET`.
fn normalize_preset(value: &str) -> String {
    preset_by_comparable()
        .get(&comparable(value))
        .cloned()
        .unwrap_or_else(|| {
            settings_catalog().text(module::TERMINAL_FONT_PRESET, "DEFAULT_TERMINAL_FONT_PRESET")
        })
}

/// `getGhosttyFontFamilyForPreset`: the first concrete family of the preset's stack.
fn ghostty_family_for_preset(preset: &str) -> String {
    let stack = presets()
        .iter()
        .find(|(candidate, _)| candidate == preset)
        .map(|(_, font_family)| font_family.clone())
        .unwrap_or_else(|| {
            settings_catalog().text(
                module::TERMINAL_FONT_PRESET,
                "MONOSPACE_TERMINAL_FONT_FAMILY",
            )
        });
    stack
        .split(',')
        .map(|part| {
            let part = part.trim();
            let part = part.strip_prefix(['\'', '"']).unwrap_or(part);
            part.strip_suffix(['\'', '"']).unwrap_or(part).to_string()
        })
        .find(|part| !part.is_empty() && part != "monospace" && part != "ui-monospace")
        .unwrap_or_else(|| "monospace".to_string())
}

/// `normalizeGhosttyFontFamily`: blank stays blank; a value that is exactly a preset name (such
/// as `Cross Platform Mono`) becomes that preset's Ghostty family (`Consolas`); anything else is
/// the trimmed text.
pub(in super::super) fn normalize_ghostty_font_family(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let preset = normalize_preset(trimmed);
    if preset == trimmed {
        return ghostty_family_for_preset(&preset);
    }
    trimmed.to_string()
}
