//! The Settings modal's search rows, option tables, defaults and ranges, read from the Settings
//! catalog crate (`packages/settings-catalog`).
//!
//! CDXC:Settings 2026-10-01 WHY:
//! The rows, defaults, option tables and hotkeys are Rust data in `ghostex-settings-catalog` (`CDXC:Settings 2026-10-01 DECISION` in its `lib.rs`), so the modal reads the crate for the platform this binary was built for. This supersedes `CDXC:Settings 2026-09-28`, which embedded `catalog/settings-catalog.generated.json` exported from the React Settings sources. The named tables keep their export names (`module_value(module::SETTINGS, "SESSION_CHAT_THEME_OPTIONS")`); the crate's `modules::exports` lists them.
//! SEE-ALSO: packages/settings-catalog/src/lib.rs, packages/settings-catalog/src/modules.rs, docs/2026-09-28/gpui-modals-migration/SETTINGS-ARCH.md.
use ghostex_settings_catalog::{self as catalog, Platform, json::ToJson};
use serde_json::{Map, Value};
use std::sync::OnceLock;

/// The areas whose named tables `module_value` looks up.
pub(crate) use ghostex_settings_catalog::modules::module;

/// One `{ label, value }` option. Numeric option values are kept as the string the React
/// select used (`String(option.value)`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingOption {
    pub(crate) label: String,
    pub(crate) value: String,
}

/// One searchable row (`SettingSearchDefinition`).
#[derive(Clone, Debug)]
pub(crate) struct SettingRowDef {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) options: Vec<SettingOption>,
    /// Hidden words search matches like the title (`ghostex_settings_catalog::search_keywords`).
    pub(crate) keywords: Vec<String>,
    pub(crate) advanced: bool,
}

/// A titled list of searchable rows: a General section, or one section of another page.
#[derive(Clone, Debug)]
pub(crate) struct SearchSectionDef {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) settings: Vec<SettingRowDef>,
}

/// A General rail group (`GENERAL_GROUPS`).
#[derive(Clone, Debug)]
pub(crate) struct GeneralGroupDef {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) sections: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct NavItemDef {
    pub(crate) id: String,
    pub(crate) title: String,
}

/// A searchable page other than General, Theme and Hotkeys.
#[derive(Clone, Debug)]
pub(crate) struct ExtraTabDef {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) sections: Vec<SearchSectionDef>,
}

/// One hotkey action, with its default formatted for this platform.
#[derive(Clone, Debug)]
pub(crate) struct HotkeyDef {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) default_key: String,
    pub(crate) default_key_label: String,
    pub(crate) windows_linux_default_key: Option<String>,
}

pub(crate) struct SettingsCatalog {
    pub(crate) general_sections: Vec<SearchSectionDef>,
    pub(crate) general_groups: Vec<GeneralGroupDef>,
    /// The General rail groups in rendered order.
    pub(crate) general_navigation: Vec<NavItemDef>,
    pub(crate) extra_tabs: Vec<ExtraTabDef>,
    pub(crate) hotkeys: Vec<HotkeyDef>,
    modules: Map<String, Value>,
    defaults: Map<String, Value>,
}

/// The catalog for this build's platform, built once.
pub(crate) fn settings_catalog() -> &'static SettingsCatalog {
    static CATALOG: OnceLock<SettingsCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| SettingsCatalog::build(Platform::current()))
}

/// `String(value)` for a JSON option value.
pub(crate) fn js_value_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => js_number_string(number.as_f64().unwrap_or_default()),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// `String(number)` for the finite numbers the settings use.
pub(crate) fn js_number_string(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        let text = format!("{number}");
        if text.contains('e') {
            text
        } else {
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    }
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn parse_options(value: Option<&Value>) -> Vec<SettingOption> {
    value
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|option| {
                    Some(SettingOption {
                        label: text(option.get("label")),
                        value: js_value_string(option.get("value")?),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn section_def(section: &catalog::Section) -> SearchSectionDef {
    SearchSectionDef {
        id: section.id.to_string(),
        title: section.title.to_string(),
        settings: section
            .settings
            .iter()
            .map(|row| SettingRowDef {
                key: row.key.to_string(),
                title: row.title.clone(),
                subtitle: row.subtitle.clone(),
                options: row
                    .options
                    .iter()
                    .map(|option| SettingOption {
                        label: option.label.clone(),
                        value: option.value.clone(),
                    })
                    .collect(),
                // A renamed group's old name finds each of its rows (`section_search_keywords`).
                keywords: row
                    .keywords
                    .iter()
                    .chain(ghostex_settings_catalog::section_search_keywords(
                        section.id,
                    ))
                    .map(|word| word.to_string())
                    .collect(),
                advanced: row.advanced,
            })
            .collect(),
    }
}

impl SettingsCatalog {
    fn build(platform: Platform) -> Self {
        let mut modules = Map::new();
        for module in catalog::modules::MODULES {
            let mut exports = Map::new();
            for (name, value) in catalog::modules::exports(module, platform) {
                exports.insert(name.to_string(), value.to_value());
            }
            modules.insert(module.to_string(), Value::Object(exports));
        }
        let mut defaults = Map::new();
        for (key, value) in catalog::defaults() {
            let value = catalog::availability::platform_default(platform, key).unwrap_or(value);
            defaults.insert(key.to_string(), value.to_json().to_value());
        }
        Self {
            general_sections: catalog::general_sections(platform)
                .iter()
                .map(section_def)
                .collect(),
            general_groups: catalog::GENERAL_GROUPS
                .iter()
                .map(|group| GeneralGroupDef {
                    id: group.id.to_string(),
                    title: group.title.to_string(),
                    sections: group.sections.iter().map(|id| id.to_string()).collect(),
                })
                .collect(),
            general_navigation: catalog::general_navigation()
                .iter()
                .map(|item| NavItemDef {
                    id: item.id.to_string(),
                    title: item.title.to_string(),
                })
                .collect(),
            extra_tabs: catalog::extra_pages(platform)
                .iter()
                .map(|page| ExtraTabDef {
                    id: page.id.to_string(),
                    title: page.title.to_string(),
                    sections: page.sections.iter().map(section_def).collect(),
                })
                .collect(),
            hotkeys: catalog::hotkey_definitions()
                .iter()
                .map(|definition| HotkeyDef {
                    id: definition.id.to_string(),
                    title: definition.title.to_string(),
                    description: definition.description.to_string(),
                    default_key: definition.default_key.to_string(),
                    default_key_label: definition.default_key_label(platform),
                    windows_linux_default_key: definition
                        .windows_linux_default_key
                        .filter(|key| !key.is_empty())
                        .map(str::to_string),
                })
                .collect(),
            modules,
            defaults,
        }
    }

    /// A named table of one of the catalog areas (`module::*`).
    pub(crate) fn module_value(&self, module: &str, name: &str) -> Option<&Value> {
        self.modules.get(module)?.get(name)
    }

    /// An option table (`*_OPTIONS`) of a catalog area.
    pub(crate) fn options(&self, module: &str, name: &str) -> Vec<SettingOption> {
        parse_options(self.module_value(module, name))
    }

    /// A numeric constant (`MIN_*`, `MAX_*`, `*_STEP*`) of a catalog area; 0 when missing.
    pub(crate) fn number(&self, module: &str, name: &str) -> f64 {
        self.module_value(module, name)
            .and_then(Value::as_f64)
            .unwrap_or_default()
    }

    pub(crate) fn flag(&self, module: &str, name: &str) -> bool {
        self.module_value(module, name).and_then(Value::as_bool) == Some(true)
    }

    pub(crate) fn text(&self, module: &str, name: &str) -> String {
        text(self.module_value(module, name))
    }

    /// `formatSidebarHotkeyLabel(hotkey)` for the few labels General prints.
    pub(crate) fn hotkey_label(&self, hotkey: &str) -> String {
        catalog::hotkey_label(hotkey, Platform::current())
    }

    /// `DEFAULT_GHOSTEX_SETTINGS`.
    pub(crate) fn defaults(&self) -> &Map<String, Value> {
        &self.defaults
    }

    pub(crate) fn default_value(&self, key: &str) -> Option<&Value> {
        self.defaults.get(key)
    }

    /// `isAdvancedMainSetting` (`ADVANCED_MAIN_SETTING_KEYS`).
    pub(crate) fn is_advanced(&self, key: &str) -> bool {
        catalog::is_advanced_setting(key)
    }

    /// `NOT_RECOMMENDED_SETTING_KEYS`: rows that carry a "Not recommended" label.
    pub(crate) fn is_not_recommended(&self, key: &str) -> bool {
        catalog::is_not_recommended_setting(key)
    }

    pub(crate) fn general_section(&self, id: &str) -> Option<&SearchSectionDef> {
        self.general_sections
            .iter()
            .find(|section| section.id == id)
    }

    pub(crate) fn general_group(&self, id: &str) -> Option<&GeneralGroupDef> {
        self.general_groups.iter().find(|group| group.id == id)
    }

    pub(crate) fn extra_tab(&self, id: &str) -> Option<&ExtraTabDef> {
        self.extra_tabs.iter().find(|tab| tab.id == id)
    }

    /// A string list or string-keyed record of string lists from a catalog area
    /// (`MAIN_SETTINGS_SECTION_SETTING_KEYS[group]`, and the like).
    pub(crate) fn string_list(&self, module: &str, name: &str, key: Option<&str>) -> Vec<String> {
        let value = self.module_value(module, name);
        let value = match key {
            Some(key) => value.and_then(|value| value.get(key)),
            None => value,
        };
        value
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}
