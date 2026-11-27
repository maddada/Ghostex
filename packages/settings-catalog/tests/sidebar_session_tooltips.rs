use ghostex_settings_catalog::{
    default_value, has_search_row, is_power_user_flag, json::J, Platform,
};

/// CDXC:Tooltips 2026-10-29 DECISION: User: "please let me disable the tooltip for sessions in the
/// sidebar with a toggle (advanced setting)". The switch is on by default (today's behaviour), shows
/// only with Show Advanced, and has a search row on every platform.
#[test]
fn session_tooltips_switch_is_an_advanced_row_on_by_default() {
    assert!(matches!(
        default_value("showSidebarSessionTooltips"),
        Some(J::Bool(true))
    ));
    assert!(is_power_user_flag("showSidebarSessionTooltips"));
    for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        assert!(has_search_row(platform, "showSidebarSessionTooltips"));
    }
}
