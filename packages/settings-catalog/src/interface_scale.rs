//! Settings > Appearance > Interface scale: one Scale for the sidebar, the chat, Files and the
//! terminal font, unless Scale areas separately gives each its own value.
//!
//! CDXC:Settings 2026-11-05 DECISION:
//! User (issue #425, option B): Settings > Appearance gets an Interface scale section with one Scale slider and a Scale areas separately switch, off by default, that reveals Sidebar, Chat, Files and Terminal (font size) sliders. Only those four areas scale; the header, tabs, menus and dialogs keep their size (no window-wide zoom). Off, Scale sets all four together; on, each area uses only its own slider. Off, Cmd/Ctrl + = / - / 0 step Scale everywhere; on, they keep each area's own zoom.
//! The per-area values stay in their old keys (`agentManagerZoomPercent`, `sessionChatZoomPercent`, `filesZoomPercent`, `terminalFontSize`), so a settings file that never saved the switch reads it as on when any of them differs from its default: nobody's sizes change on update.
//! SEE-ALSO: packages/gx-core/src/hud/settings.rs (sidebar), apps/desktop/src/app/native_chat/zoom.rs (chat), apps/desktop/src/ui_fonts.rs (Files), apps/desktop/src/shared_settings/ghostty_config_values.rs and snapshot.rs (terminal), apps/desktop/src/app/interface_scale_zoom.rs (the zoom keys), apps/desktop/src/app/window/settings_modal/tabs/theme/interface_scale.rs (the section).

use serde_json::{Map, Value};

/// Settings > Appearance > Interface scale > Scale (%).
pub const INTERFACE_SCALE_SETTING: &str = "interfaceScalePercent";
/// Settings > Appearance > Interface scale > Scale areas separately.
pub const SCALE_AREAS_SEPARATELY_SETTING: &str = "interfaceScaleAreasSeparately";
/// The terminal's area value: a font size in points, not a percentage.
pub const TERMINAL_FONT_SIZE_SETTING: &str = "terminalFontSize";
/// The areas Scale drives, each by its own key: three percentages and the terminal font size.
pub const SCALED_AREA_SETTINGS: [&str; 4] = [
    "agentManagerZoomPercent",
    "sessionChatZoomPercent",
    "filesZoomPercent",
    TERMINAL_FONT_SIZE_SETTING,
];

/// Scale's range, the chat zoom's.
pub const MIN_INTERFACE_SCALE_PERCENT: f64 = 70.0;
pub const MAX_INTERFACE_SCALE_PERCENT: f64 = 200.0;
/// The terminal font size range, in points; the desktop clamps the same way
/// (`MIN_TERMINAL_FONT_SIZE`/`MAX_TERMINAL_FONT_SIZE` in apps/desktop/src/shared_settings/defaults.rs).
pub const TERMINAL_FONT_SIZE_RANGE: (f64, f64) = (8.0, 32.0);
/// The sidebar's own range, wider than Scale's.
pub const SIDEBAR_ZOOM_RANGE: (f64, f64) = (
    crate::data::MIN_AGENT_MANAGER_ZOOM_PERCENT,
    crate::data::MAX_AGENT_MANAGER_ZOOM_PERCENT,
);

fn default_number(key: &str) -> f64 {
    match crate::default_value(key) {
        Some(crate::json::J::Num(value)) => *value,
        _ => 100.0,
    }
}

/// Whether each area uses its own value. A saved switch wins; otherwise it reads as on exactly
/// when an area was already set away from its default.
pub fn scale_areas_separately(settings: &Map<String, Value>) -> bool {
    if let Some(saved) = settings
        .get(SCALE_AREAS_SEPARATELY_SETTING)
        .and_then(Value::as_bool)
    {
        return saved;
    }
    SCALED_AREA_SETTINGS.iter().any(|key| {
        settings
            .get(*key)
            .and_then(Value::as_f64)
            .is_some_and(|value| (value - default_number(key)).abs() > 0.001)
    })
}

/// Scale as a percentage, clamped to its range.
pub fn interface_scale_percent(settings: &Map<String, Value>) -> f64 {
    settings
        .get(INTERFACE_SCALE_SETTING)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(100.0)
        .clamp(MIN_INTERFACE_SCALE_PERCENT, MAX_INTERFACE_SCALE_PERCENT)
}

/// The value an area at `percent` Scale has: the percentage itself, or for the terminal the
/// default font size scaled by it.
pub fn area_value_at_scale(key: &str, percent: f64) -> f64 {
    if key == TERMINAL_FONT_SIZE_SETTING {
        // Tenths of a point, so the Settings slider and the terminal config show a clean size.
        (default_number(key) * percent / 100.0 * 10.0).round() / 10.0
    } else {
        percent
    }
}

/// The value an area draws with: its own saved value when areas scale separately (`None` when it
/// is not saved, so the reader keeps its own default), else the one taken from Scale. Each reader
/// still applies its own range.
pub fn scaled_area_value(settings: &Map<String, Value>, key: &str) -> Option<f64> {
    if scale_areas_separately(settings) {
        settings.get(key).and_then(Value::as_f64)
    } else {
        Some(area_value_at_scale(key, interface_scale_percent(settings)))
    }
}
