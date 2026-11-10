//! The Settings catalog: every Settings search row (titles and subtitles are customer copy), the
//! setting defaults, option tables and ranges, and the hotkey catalog.
//!
//! The native Settings window, Quick Access (through gx-core), `ghostex settings` (gxserver) and the
//! Ghostex Help files (`cargo xtask help-generate`) all read this crate, so a row, default or hotkey
//! changes in one place.
//!
//! CDXC:Settings 2026-10-01 DECISION:
//! User (answer 33A, docs/2026-10-01/zero-ts/NEXT-STEPS.md): move the Settings search rows, defaults and hotkeys from TypeScript to Rust, so the native app owns them and the Help files are generated from Rust. This crate is that one source; it supersedes `CDXC:Settings 2026-09-28`, which kept `packages/core-ui/settings-modal/search-catalog.ts` as the source and embedded a generated JSON copy in the desktop.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/catalog.rs, server/src/ghostex_cli/settings.rs, packages/gx-core/src/quick_access/hotkeys.rs, tooling/xtask/src/help.rs, skills/ghostex-help/references/.
//!
//! To add a setting: give it a default in `data/defaults.rs`, a row in the `general/` or `pages/`
//! file of the page that shows it, and run `cargo xtask help-generate`.

pub mod availability;
pub mod built_in_extensions;
pub mod dark_chrome;
mod data;
pub mod file_opening;
mod general;
pub mod help;
pub mod hotkey_chords;
mod hotkey_definitions;
pub mod hotkey_label;
pub mod hotkeys;
pub mod interface_scale;
pub mod intros;
pub mod json;
pub mod layout;
pub mod modules;
mod pages;
pub mod platform_text;
pub mod rows;
mod search_keywords;
pub mod spaces;
pub use search_keywords::search_keywords;

pub use hotkey_chords::{
    customized_normal_hotkey, hotkey_keystroke, platform_hotkey, terminal_hotkey,
};
pub use hotkey_label::hotkey_label;
pub use hotkeys::{
    default_hotkeys, hotkey_definition, hotkey_definitions, hotkey_has_normal_binding,
    hotkey_has_terminal_binding, terminal_default_key, windows_linux_terminal_default_key,
    HotkeyDefinition, TERMINAL_HOTKEYS_SETTING_KEY, TERMINAL_ONLY_HOTKEY_IDS,
    VIEW_SCOPED_HOTKEY_IDS,
};
pub use intros::page_intro;
pub use json::{Json, ToJson, J};
pub use layout::{general_group, general_navigation, GeneralGroup, NavItem, GENERAL_GROUPS};
pub use rows::{Page, RuntimeOptions, Section, SettingOption, SettingRow};

/// The desktop platform a catalog is built for: shortcut labels and a few rows differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    /// The platform this binary was built for.
    pub const fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// The General page's search sections, in definition order.
pub fn general_sections(platform: Platform) -> Vec<Section> {
    general::sections(platform)
}

/// The searchable pages other than General, Theme and Hotkeys.
pub fn extra_pages(platform: Platform) -> Vec<Page> {
    pages::pages(platform)
}

/// Whether Settings' own search lists a row for `key`: a row of the General (and Theme) sections
/// or of another searchable page. Keys the Help catalog adds without one (its supplemental rows and
/// app state) have none, so searching Settings for their title finds nothing.
pub fn has_search_row(platform: Platform, key: &str) -> bool {
    general_sections(platform)
        .iter()
        .any(|section| section.settings.iter().any(|row| row.key == key))
        || extra_pages(platform).iter().any(|page| {
            page.sections
                .iter()
                .any(|section| section.settings.iter().any(|row| row.key == key))
        })
}

/// Every setting key with its default value, in the order the settings file lists them.
pub fn defaults() -> &'static [(&'static str, J)] {
    data::DEFAULT_GHOSTEX_SETTINGS
}

pub fn default_value(key: &str) -> Option<&'static J> {
    data::DEFAULT_GHOSTEX_SETTINGS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

/// Fills in Default to Detailed view (`sessionChatDetailedView`) from the default keys it replaced,
/// for a settings file that never saved it: a saved `sessionChatVerboseMode` true or
/// `sessionChatSimpleMode` false opened new chats in more detail than Simple, which is Detailed now.
/// The old keys are left alone; nothing reads them any more.
///
/// CDXC:SessionChat 2026-11-02 WHY: the user retired Verbose and renamed Simple to Normal (general/chat.rs has their words), and a preference saved under the old keys must not reset.
pub fn migrate_chat_view_default(settings: &mut serde_json::Map<String, serde_json::Value>) {
    if settings.contains_key("sessionChatDetailedView") {
        return;
    }
    let verbose = settings.get("sessionChatVerboseMode") == Some(&serde_json::Value::Bool(true));
    let not_simple = settings.get("sessionChatSimpleMode") == Some(&serde_json::Value::Bool(false));
    if verbose || not_simple {
        settings.insert(
            "sessionChatDetailedView".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}

/// The switches Enable Experimental Features (`showBetaFeatures`) used to open, with the value of
/// each that means on, and whether it was on by default under the old switch.
const EXPERIMENTAL_FEATURE_SWITCHES: &[(&str, bool, bool)] = &[
    ("readAloudHidden", false, false),
    ("connectorsHidden", false, true),
    ("osIntegrationHidden", false, true),
    ("keepAwakeEnabled", true, true),
    ("allAutomationsEnabled", true, true),
];

/// Replaces Enable Experimental Features (`showBetaFeatures`) with the switches it opened: each
/// gets the state it had, on only when the old switch was on and the switch itself was on (or
/// unset and on by default), and the old key is removed. Returns whether it changed anything; a
/// settings object without the old key is left alone, so it runs once per settings file.
///
/// CDXC:Extensions 2026-11-09 DECISION: User (Q2043 on the Extensions tabs page): "remove the
/// 'Experimental features' toggle itself and move toggling each of those individually here
/// instead", with each new switch off by default and someone who had the old switch on keeping
/// everything it showed. The settings file stores every default, so most files carry
/// `showBetaFeatures: false` and `connectorsHidden: false`: the old effective state is computed
/// from both rather than left to the new defaults. Terminal shaders, GBots and Main Hub kept their
/// own switch under the old one and are not touched. The desktop saves this once at start
/// (shared_settings/experimental_switches.rs); gxserver applies it to every settings read and
/// before saving the file itself, so a gxserver that starts before the app, or runs without one,
/// sees the same switches.
pub fn migrate_experimental_features_switch(
    settings: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let Some(legacy) = settings.remove("showBetaFeatures") else {
        return false;
    };
    let was_on = legacy.as_bool() == Some(true);
    for (key, on_value, on_by_default) in EXPERIMENTAL_FEATURE_SWITCHES {
        let switch_on = settings
            .get(*key)
            .and_then(serde_json::Value::as_bool)
            .map_or(*on_by_default, |value| value == *on_value);
        let now_on = was_on && switch_on;
        settings.insert(
            (*key).to_string(),
            serde_json::Value::Bool(if now_on { *on_value } else { !*on_value }),
        );
    }
    true
}

/// Writes `workAreaFrame` false into the settings of someone who already had a settings file when the
/// frame became on by default, so their work area keeps the edge-to-edge look it had. Returns whether it
/// changed anything; a saved choice (either way) is left alone.
///
/// CDXC:Workarea 2026-11-08 WHY: stored settings leave a key out until it is saved, so an existing user who never touched Frame the work area has no `workAreaFrame` in the file, and the new default (on) would silently frame their window. The desktop runs this once, behind a marker file, at start-up when the settings file already exists (`migrate_work_area_frame_default_once`); a new install has no file then, so it gets the default.
pub fn keep_work_area_frame_off_for_existing_settings(
    settings: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    if settings.contains_key("workAreaFrame") {
        return false;
    }
    settings.insert("workAreaFrame".to_string(), serde_json::Value::Bool(false));
    true
}

/// Rows a page folds into its collapsed Advanced section (`isAdvancedMainSetting`).
pub fn is_advanced_setting(key: &str) -> bool {
    data::ADVANCED_MAIN_SETTING_KEYS.contains(&key)
}

/// Rows that carry a "Not recommended" label (`NOT_RECOMMENDED_SETTING_KEYS`).
pub fn is_not_recommended_setting(key: &str) -> bool {
    data::NOT_RECOMMENDED_SETTING_KEYS.contains(&key)
}

/// The setting keys each General group renders, for groups whose sections are not listed in `GENERAL_GROUPS`.
pub fn general_group_setting_keys(group: &str) -> &'static [&'static str] {
    data::MAIN_SETTINGS_SECTION_SETTING_KEYS
        .iter()
        .find(|(id, _)| *id == group)
        .map(|(_, keys)| *keys)
        .unwrap_or(&[])
}

#[cfg(test)]
mod chat_view_default_tests {
    use super::migrate_chat_view_default;
    use serde_json::{json, Map, Value};

    fn migrated(value: Value) -> Option<Value> {
        let mut settings: Map<String, Value> = serde_json::from_value(value).unwrap();
        migrate_chat_view_default(&mut settings);
        settings.get("sessionChatDetailedView").cloned()
    }

    #[test]
    fn old_verbose_or_not_simple_defaults_become_detailed() {
        assert_eq!(
            migrated(json!({"sessionChatVerboseMode": true})),
            Some(json!(true))
        );
        assert_eq!(
            migrated(json!({"sessionChatSimpleMode": false})),
            Some(json!(true))
        );
        assert_eq!(migrated(json!({"sessionChatSimpleMode": true})), None);
        assert_eq!(migrated(json!({})), None);
        // A saved choice of the new switch wins over the old keys.
        assert_eq!(
            migrated(json!({"sessionChatDetailedView": false, "sessionChatVerboseMode": true})),
            Some(json!(false))
        );
    }
}
