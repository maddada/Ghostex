use serde_json::{Map, Value};

/// CDXC:Theming 2026-09-13 SEE-ALSO:
/// packages/shared/appearance.ts owns the Follow app default and explicit Light, Dark, and System overrides shared with React.
pub fn effective_content_color_scheme(
    settings: &Map<String, Value>,
    setting_key: &str,
) -> &'static str {
    match settings.get(setting_key).and_then(Value::as_str) {
        Some("light") => "light",
        Some("dark") => "dark",
        Some("system") => "system",
        _ => match settings
            .get("sidebarTheme")
            .and_then(Value::as_str)
            .or_else(|| {
                ghostex_settings_catalog::default_value("sidebarTheme")
                    .and_then(ghostex_settings_catalog::J::as_str)
            }) {
            Some("plain-light") => "light",
            Some("system") | None => "system",
            _ => "dark",
        },
    }
}
