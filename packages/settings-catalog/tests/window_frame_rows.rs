use ghostex_settings_catalog::{
    availability::row_hidden, default_value, has_search_row, is_advanced_setting, json::J, Platform,
};

const FRAME_ROWS: [&str; 2] = ["workAreaFrame", "showWindowButtonsWhenSidebarCollapsed"];

/// CDXC:Settings 2026-11-08 DECISION: User: "please add setting under this [Frame the work area] to
/// show traffic lights on macos when sidebar is collapsed (default to yes). only show this sub
/// setting on macos. also please move this whole setting under a new advanced area in the interface
/// page in settings". Both rows are on by default, advanced (the Interface page's Advanced fold) and
/// on macOS only.
#[test]
fn the_frame_rows_are_advanced_on_by_default_and_macos_only() {
    for key in FRAME_ROWS {
        assert!(matches!(default_value(key), Some(J::Bool(true))), "{key}");
        assert!(is_advanced_setting(key), "{key}");
        assert!(has_search_row(Platform::MacOs, key), "{key}");
        assert!(!row_hidden(Platform::MacOs, key, |_| None), "{key}");
        for platform in [Platform::Windows, Platform::Linux] {
            assert!(row_hidden(platform, key, |_| None), "{key}");
        }
    }
}
