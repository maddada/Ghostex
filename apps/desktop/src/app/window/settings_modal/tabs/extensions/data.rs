//! The data the Extensions page reads, ported from the TypeScript it used: the built-in
//! descriptors (packages/shared/ghostex-official-extensions.ts (deleted 2026-10-01), read from the generated catalog),
//! the page filter (extensions-modal/extension-filter.ts (deleted 2026-10-01)), the installed and catalog entries of the
//! gxserver wire contract (packages/shared/ghostex-extensions.ts), view scopes
//! (ghostex-settings/view-scopes.ts), the view order (ghostex-settings/titlebar-view-order.ts) and
//! the custom views and templates (ghostex-settings/custom-views.ts, project-views.ts).
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::icon;
use super::super::super::store::SettingsValues;
use serde_json::{Map, Value};
use std::sync::OnceLock;

// ---- the built-in descriptors ------------------------------------------------------------

/// One `GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES` entry.
#[derive(Clone, Debug)]
pub(crate) struct OfficialCategory {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) type_label: String,
}

/// One `GHOSTEX_OFFICIAL_EXTENSIONS` entry.
#[derive(Clone, Debug)]
pub(crate) struct OfficialExtension {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) settings_key: String,
    /// `view`, `titlebar-button` or `sidebar`.
    pub(crate) placement: String,
    pub(crate) category: String,
    pub(crate) app_wide: bool,
    pub(crate) requires_agent_cli: Option<String>,
    pub(crate) requires_extension: Option<String>,
}

pub(crate) fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn optional_text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn string_list(value: Option<&Value>) -> Vec<String> {
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

pub(crate) fn official_categories() -> &'static [OfficialCategory] {
    static CATEGORIES: OnceLock<Vec<OfficialCategory>> = OnceLock::new();
    CATEGORIES.get_or_init(|| {
        settings_catalog()
            .module_value(
                module::OFFICIAL_EXTENSIONS,
                "GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES",
            )
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| OfficialCategory {
                        id: text(item, "id"),
                        label: text(item, "label"),
                        type_label: text(item, "typeLabel"),
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

pub(crate) fn official_extensions() -> &'static [OfficialExtension] {
    static EXTENSIONS: OnceLock<Vec<OfficialExtension>> = OnceLock::new();
    EXTENSIONS.get_or_init(|| {
        settings_catalog()
            .module_value(module::OFFICIAL_EXTENSIONS, "GHOSTEX_OFFICIAL_EXTENSIONS")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| OfficialExtension {
                        id: text(item, "id"),
                        title: text(item, "title"),
                        description: text(item, "description"),
                        settings_key: text(item, "settingsKey"),
                        placement: text(item, "placement"),
                        category: text(item, "category"),
                        app_wide: item.get("appWide").and_then(Value::as_bool) == Some(true),
                        requires_agent_cli: optional_text(item, "requiresAgentCli"),
                        requires_extension: optional_text(item, "requiresExtension"),
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// `OFFICIAL_EXTENSION_AGENT_CLIS`: the agent CLIs some entries need before they are offered.
pub(crate) fn official_agent_clis() -> Vec<String> {
    let mut clis: Vec<String> = Vec::new();
    for extension in official_extensions() {
        if let Some(cli) = &extension.requires_agent_cli
            && !clis.contains(cli)
        {
            clis.push(cli.clone());
        }
    }
    clis
}

pub(crate) fn official_category(
    extension: &OfficialExtension,
) -> Option<&'static OfficialCategory> {
    official_categories()
        .iter()
        .find(|category| category.id == extension.category)
}

/// `isOfficialExtensionEnabled`: the catalog's one rule (an inverted "hidden" key, or a
/// `settingsKeyEnables` key such as Spaces').
pub(crate) fn is_official_enabled(values: &SettingsValues, extension: &OfficialExtension) -> bool {
    ghostex_settings_catalog::built_in_extensions::enabled_with(&extension.id, |key| {
        Some(values.bool(key))
    })
}

/// The value the switch writes to `extension.settings_key` to turn it on (`next`) or off.
pub(crate) fn official_switch_value(extension: &OfficialExtension, next: bool) -> bool {
    match ghostex_settings_catalog::built_in_extensions::switch_key(&extension.id) {
        Some((_, true)) => next,
        _ => !next,
    }
}

/// `officialExtensionBlockedBy`: the entry that must be enabled first, while it is off.
pub(crate) fn official_blocked_by(
    values: &SettingsValues,
    extension: &OfficialExtension,
) -> Option<&'static OfficialExtension> {
    let required = extension.requires_extension.as_deref()?;
    official_extensions()
        .iter()
        .find(|candidate| candidate.id == required)
        .filter(|required| !is_official_enabled(values, required))
}

/// `OFFICIAL_EXTENSION_ICONS`: the Tabler icon each built-in card draws.
pub(crate) fn official_icon(id: &str) -> &'static str {
    match id {
        "github" | "gitActions" => "modals/settings/git-commit.svg",
        "sentry" => "modals/settings/bug.svg",
        "figma" => icon::PALETTE,
        "vercel" => "modals/settings/cloud.svg",
        "supabase" => "modals/settings/database.svg",
        "github-actions" | "kanban" | "quickActions" => icon::PLAYER_PLAY,
        "spaces" => "modals/settings/stack-2.svg",
        "cloudBoxes" => "modals/settings/box.svg",
        "workspaces" => "modals/settings/briefcase.svg",
        "posthog" => "modals/settings/chart-bar.svg",
        "automate" => "modals/settings/bolt.svg",
        "botAutomations" => "modals/settings/rss.svg",
        "bots" => "modals/settings/robot.svg",
        "claudeWeb" => "agent-icons/claude.svg",
        "code" => "modals/space-editor/brand-vscode.svg",
        "storybook" => "modals/settings/code-dots.svg",
        "docs" => "modals/settings/file-text.svg",
        "changes" => "modals/settings/git-commit.svg",
        "extensionsButton" => "modals/settings/puzzle.svg",
        "connectors" => "modals/settings/plug-connected.svg",
        "osIntegration" => "modals/settings/device-desktop.svg",
        "debugging" => "modals/settings/bug.svg",
        "help" => "modals/settings/help-circle.svg",
        "notifications" => "modals/settings/bell.svg",
        "openIn" => icon::FOLDER_OPEN,
        "resources" => "modals/settings/device-desktop.svg",
        "terminal" => "modals/settings/terminal-2.svg",
        "tips" => icon::INFO_CIRCLE,
        _ => "modals/settings/world.svg",
    }
}

/// `BUILT_IN_CATEGORY_LABELS`.
pub(crate) fn built_in_category_labels() -> Vec<String> {
    official_categories()
        .iter()
        .map(|category| category.label.clone())
        .collect()
}

// ---- the page filter -----------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceFilter {
    All,
    BuiltIn,
    Installed,
    Store,
    Custom,
}

impl SourceFilter {
    pub(crate) fn id(self) -> &'static str {
        match self {
            SourceFilter::All => "all",
            SourceFilter::BuiltIn => "built-in",
            SourceFilter::Installed => "installed",
            SourceFilter::Store => "store",
            SourceFilter::Custom => "custom",
        }
    }

    pub(crate) fn from_id(id: &str) -> Self {
        match id {
            "built-in" => SourceFilter::BuiltIn,
            "installed" => SourceFilter::Installed,
            "store" => SourceFilter::Store,
            "custom" => SourceFilter::Custom,
            _ => SourceFilter::All,
        }
    }

    /// `extensionSourceLabel`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            SourceFilter::All => "All sources",
            SourceFilter::BuiltIn => "Built-in",
            SourceFilter::Installed => "Installed",
            SourceFilter::Store => "Available",
            SourceFilter::Custom => "Your views",
        }
    }
}

/// `EXTENSION_TYPE_FILTERS`.
pub(crate) const EXTENSION_TYPE_FILTERS: [&str; 8] = [
    "all",
    "view",
    "header-button",
    "menu-item",
    "chat-bar",
    "popup",
    "modal",
    "terminal-pane",
];

/// `extensionTypeLabel`.
pub(crate) fn extension_type_label(value: &str) -> String {
    match value {
        "all" => "All types".to_string(),
        "chat-bar" => "Chat bar".to_string(),
        "terminal-pane" => "Terminal pane".to_string(),
        "header-button" => "Header button".to_string(),
        "menu-item" => "Menu item".to_string(),
        other => capitalize(other),
    }
}

pub(crate) fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// `ExtensionFilter`: one filter the page owns over every extension group.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExtensionFilter {
    pub(crate) category: String,
    pub(crate) query: String,
    pub(crate) source: SourceFilter,
    pub(crate) type_: String,
}

impl Default for ExtensionFilter {
    /// `DEFAULT_EXTENSION_FILTER`.
    fn default() -> Self {
        Self {
            category: "all".to_string(),
            query: String::new(),
            source: SourceFilter::All,
            type_: "all".to_string(),
        }
    }
}

impl ExtensionFilter {
    /// `isExtensionFilterActive`.
    pub(crate) fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
            || self.source != SourceFilter::All
            || self.type_ != "all"
            || self.category != "all"
    }

    /// `extensionFilterMatches`.
    pub(crate) fn matches(&self, subject: &FilterSubject) -> bool {
        if self.source != SourceFilter::All && self.source != subject.source {
            return false;
        }
        if self.type_ != "all" && !subject.types.contains(&self.type_) {
            return false;
        }
        if self.category != "all" && !subject.categories.contains(&self.category) {
            return false;
        }
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let mut haystack = vec![subject.title.clone()];
        haystack.extend(subject.search_text.iter().cloned());
        haystack.extend(subject.categories.iter().cloned());
        haystack.join(" ").to_lowercase().contains(&query)
    }
}

/// `ExtensionFilterSubject`: what one card offers to the filter.
#[derive(Clone, Debug)]
pub(crate) struct FilterSubject {
    pub(crate) categories: Vec<String>,
    pub(crate) search_text: Vec<String>,
    pub(crate) source: SourceFilter,
    pub(crate) title: String,
    pub(crate) types: Vec<String>,
}

/// `builtInFilterSubject`.
pub(crate) fn built_in_filter_subject(extension: &OfficialExtension) -> FilterSubject {
    let category = official_category(extension);
    let types = if extension.placement == "sidebar" {
        Vec::new()
    } else if extension.placement == "view" {
        vec!["view".to_string()]
    } else if category.is_some_and(|category| category.id == "header-buttons") {
        vec!["header-button".to_string()]
    } else {
        vec!["menu-item".to_string()]
    };
    FilterSubject {
        categories: category
            .map(|category| vec![category.label.clone()])
            .unwrap_or_default(),
        search_text: vec![extension.description.clone()],
        source: SourceFilter::BuiltIn,
        title: extension.title.clone(),
        types,
    }
}

// ---- installed extensions and catalog entries ------------------------------------------------

/// `GhostexInstalledExtension`, read from gxserver's JSON.
#[derive(Clone, Debug)]
pub(crate) struct InstalledExtension {
    pub(crate) raw: Value,
}

impl InstalledExtension {
    pub(crate) fn id(&self) -> String {
        text(&self.raw, "id")
    }

    pub(crate) fn manifest(&self) -> &Value {
        &self.raw["manifest"]
    }

    pub(crate) fn state(&self) -> &Value {
        &self.raw["state"]
    }

    pub(crate) fn title(&self) -> String {
        text(self.manifest(), "title")
    }

    pub(crate) fn description(&self) -> String {
        text(self.manifest(), "description")
    }

    pub(crate) fn author(&self) -> String {
        text(self.manifest(), "author")
    }

    pub(crate) fn icon(&self) -> String {
        text(self.manifest(), "icon")
    }

    pub(crate) fn version(&self) -> String {
        text(self.state(), "version")
    }

    pub(crate) fn enabled(&self) -> bool {
        self.state()["enabled"].as_bool() == Some(true)
    }

    pub(crate) fn pinned(&self) -> bool {
        self.state()["pinned"].as_bool() == Some(true)
    }

    pub(crate) fn chat_bar_auto_open(&self) -> bool {
        self.state()["chatBarAutoOpen"].as_bool() == Some(true)
    }

    pub(crate) fn is_terminal_pane(&self) -> bool {
        self.manifest()["kind"].as_str() == Some("terminal-pane")
    }

    pub(crate) fn placements(&self) -> Vec<String> {
        string_list(self.manifest().get("placements"))
    }

    pub(crate) fn categories(&self) -> Vec<String> {
        string_list(self.manifest().get("categories"))
    }

    pub(crate) fn placement(&self) -> String {
        optional_text(self.state(), "placement")
            .unwrap_or_else(|| text(self.manifest(), "defaultPlacement"))
    }

    pub(crate) fn state_placement(&self) -> Option<String> {
        optional_text(self.state(), "placement")
    }

    pub(crate) fn terminal_placement(&self) -> String {
        optional_text(self.state(), "terminalPlacement").unwrap_or_else(|| "splitRight".into())
    }

    pub(crate) fn granted_permissions(&self) -> Vec<String> {
        string_list(self.state().get("grantedPermissions"))
    }

    pub(crate) fn preferences(&self) -> Vec<Preference> {
        parse_preferences(self.manifest().get("preferences"))
    }

    pub(crate) fn stored_preferences(&self) -> Map<String, Value> {
        self.state()["preferences"]
            .as_object()
            .cloned()
            .unwrap_or_default()
    }

    /// `placementLabel`: the card footer's placement.
    pub(crate) fn placement_label(&self) -> String {
        if self.is_terminal_pane() {
            return if self.terminal_placement() == "tab" {
                "New terminal tab".into()
            } else {
                "Terminal split".into()
            };
        }
        let placement = self.placement();
        if placement == "chat-bar" {
            "Chat bar".into()
        } else {
            capitalize(&placement)
        }
    }

    /// `installedFilterSubject`.
    pub(crate) fn filter_subject(&self) -> FilterSubject {
        FilterSubject {
            categories: self.categories(),
            search_text: vec![self.description(), self.author()],
            source: SourceFilter::Installed,
            title: self.title(),
            types: catalog_types(self.manifest()),
        }
    }
}

/// `GhostexExtensionCatalogEntry`.
#[derive(Clone, Debug)]
pub(crate) struct CatalogEntry {
    pub(crate) raw: Value,
}

impl CatalogEntry {
    pub(crate) fn name(&self) -> String {
        text(&self.raw, "name")
    }

    pub(crate) fn title(&self) -> String {
        text(&self.raw, "title")
    }

    pub(crate) fn description(&self) -> String {
        text(&self.raw, "description")
    }

    pub(crate) fn author(&self) -> String {
        text(&self.raw, "author")
    }

    pub(crate) fn version(&self) -> String {
        text(&self.raw, "version")
    }

    pub(crate) fn icon(&self) -> String {
        text(&self.raw, "icon")
    }

    pub(crate) fn readme(&self) -> String {
        text(&self.raw, "readme")
    }

    pub(crate) fn changelog(&self) -> String {
        text(&self.raw, "changelog")
    }

    pub(crate) fn categories(&self) -> Vec<String> {
        string_list(self.raw.get("categories"))
    }

    pub(crate) fn screenshots(&self) -> Vec<String> {
        string_list(self.raw.get("screenshots"))
    }

    pub(crate) fn permissions(&self) -> Vec<String> {
        string_list(self.raw.get("permissions"))
    }

    /// `server` runs a command (`'command' in entry.server`).
    pub(crate) fn runs_background_process(&self) -> bool {
        self.raw["server"].get("command").is_some()
    }

    /// The fixed remote page of a `{ url }` server.
    pub(crate) fn remote_url(&self) -> Option<String> {
        self.raw["server"]
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    /// `catalogFilterSubject`.
    pub(crate) fn filter_subject(&self) -> FilterSubject {
        FilterSubject {
            categories: self.categories(),
            search_text: vec![self.description(), self.author()],
            source: SourceFilter::Store,
            title: self.title(),
            types: catalog_types(&self.raw),
        }
    }
}

/// `catalogTypes`.
pub(crate) fn catalog_types(manifest: &Value) -> Vec<String> {
    if manifest["kind"].as_str() == Some("terminal-pane") {
        vec!["terminal-pane".to_string()]
    } else {
        string_list(manifest.get("placements"))
    }
}

/// `storeCategories`: every category of the catalog and the installed list, first seen first.
pub(crate) fn store_categories(
    catalog: &[CatalogEntry],
    installed: &[InstalledExtension],
) -> Vec<String> {
    let mut categories: Vec<String> = Vec::new();
    for category in catalog
        .iter()
        .flat_map(CatalogEntry::categories)
        .chain(installed.iter().flat_map(InstalledExtension::categories))
    {
        if !categories.contains(&category) {
            categories.push(category);
        }
    }
    categories
}

/// `filterStoreExtensions`: the installed list and the not-yet-installed catalog entries that
/// match, as indexes into `installed` and `catalog`.
pub(crate) fn filter_store(
    filter: &ExtensionFilter,
    catalog: &[CatalogEntry],
    installed: &[InstalledExtension],
) -> (Vec<usize>, Vec<usize>) {
    let installed_ids: Vec<String> = installed.iter().map(InstalledExtension::id).collect();
    let installed_matches = installed
        .iter()
        .enumerate()
        .filter(|(_, extension)| filter.matches(&extension.filter_subject()))
        .map(|(index, _)| index)
        .collect();
    let store_matches = catalog
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            !installed_ids.contains(&entry.name()) && filter.matches(&entry.filter_subject())
        })
        .map(|(index, _)| index)
        .collect();
    (installed_matches, store_matches)
}

/// `replaceInstalled`: swaps in (or appends) an extension and keeps the list sorted by title.
pub(crate) fn replace_installed(list: &mut Vec<InstalledExtension>, next: InstalledExtension) {
    let id = next.id();
    if let Some(existing) = list.iter_mut().find(|extension| extension.id() == id) {
        *existing = next;
    } else {
        list.push(next);
    }
    sort_installed(list);
}

/// `left.manifest.title.localeCompare(right.manifest.title)`.
pub(crate) fn sort_installed(list: &mut [InstalledExtension]) {
    list.sort_by(|left, right| locale_compare(&left.title(), &right.title()));
}

/// An approximation of `String.prototype.localeCompare` for the default ICU collation:
/// case-insensitive first, lower case before upper case on a tie.
pub(crate) fn locale_compare(left: &str, right: &str) -> std::cmp::Ordering {
    let folded = left.to_lowercase().cmp(&right.to_lowercase());
    if folded != std::cmp::Ordering::Equal {
        return folded;
    }
    right.cmp(left)
}

/// `isVersionNewer(candidate, current)`.
pub(crate) fn is_version_newer(candidate: &str, current: &str) -> bool {
    fn parts(version: &str) -> Vec<i64> {
        version
            .split('-')
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|part| {
                let digits: String = part
                    .trim_start()
                    .chars()
                    .enumerate()
                    .take_while(|(index, character)| {
                        character.is_ascii_digit()
                            || (*index == 0 && (*character == '-' || *character == '+'))
                    })
                    .map(|(_, character)| character)
                    .collect();
                digits.parse::<i64>().unwrap_or(0)
            })
            .collect()
    }
    let candidate = parts(candidate);
    let current = parts(current);
    for index in 0..candidate.len().max(current.len()) {
        let difference =
            candidate.get(index).copied().unwrap_or(0) - current.get(index).copied().unwrap_or(0);
        if difference != 0 {
            return difference > 0;
        }
    }
    false
}

/// `permissionLabel`.
pub(crate) fn permission_label(permission: &str) -> String {
    match permission {
        "cli" => "Ghostex CLI",
        "clipboard" => "Clipboard",
        "exec" => "System commands",
        "network" => "Network access",
        "ssh" => "SSH access",
        other => other,
    }
    .to_string()
}

/// `PERMISSION_DESCRIPTIONS` of the install consent.
pub(crate) fn permission_description(permission: &str) -> &'static str {
    match permission {
        "cli" => "Control Ghostex through its command-line interface.",
        "clipboard" => "Read from or write to your clipboard.",
        "exec" => "Run system commands on this machine.",
        "network" => "Connect to services on your network or the internet.",
        "ssh" => "Use configured SSH access for remote machines.",
        _ => "",
    }
}

// ---- preferences ---------------------------------------------------------------------------

/// `GhostexExtensionPreference`.
#[derive(Clone, Debug)]
pub(crate) struct Preference {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    /// `textfield`, `password`, `checkbox`, `dropdown`, `file` or `directory`.
    pub(crate) kind: String,
    pub(crate) required: bool,
    pub(crate) default: Option<Value>,
    pub(crate) placeholder: Option<String>,
    /// `data`: `(title, value)` options of a dropdown.
    pub(crate) options: Vec<(String, String)>,
}

fn parse_preferences(value: Option<&Value>) -> Vec<Preference> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| Preference {
                    name: text(item, "name"),
                    title: text(item, "title"),
                    description: text(item, "description"),
                    kind: text(item, "type"),
                    required: item["required"].as_bool() == Some(true),
                    default: item
                        .get("default")
                        .filter(|value| !value.is_null())
                        .cloned(),
                    placeholder: optional_text(item, "placeholder"),
                    options: item["data"]
                        .as_array()
                        .map(|options| {
                            options
                                .iter()
                                .map(|option| (text(option, "title"), text(option, "value")))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `preferenceValues(definitions, stored)`: the defaults under the stored values.
pub(crate) fn preference_values(
    definitions: &[Preference],
    stored: &Map<String, Value>,
) -> Map<String, Value> {
    let mut values = Map::new();
    for definition in definitions {
        if let Some(default) = &definition.default {
            values.insert(definition.name.clone(), default.clone());
        }
    }
    for (key, value) in stored {
        values.insert(key.clone(), value.clone());
    }
    values
}

/// `missingRequiredPreferences`.
pub(crate) fn missing_required_preferences(
    definitions: &[Preference],
    values: &Map<String, Value>,
) -> Vec<String> {
    definitions
        .iter()
        .filter(|definition| definition.required)
        .filter(|definition| {
            let value = values.get(&definition.name);
            if definition.kind == "checkbox" {
                value.and_then(Value::as_bool) != Some(true)
            } else {
                value
                    .and_then(Value::as_str)
                    .is_none_or(|text| text.trim().is_empty())
            }
        })
        .map(|definition| definition.name.clone())
        .collect()
}

/// `String(value)` of a preference value.
pub(crate) fn preference_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => {
            super::super::super::catalog::js_number_string(number.as_f64().unwrap_or_default())
        }
        _ => String::new(),
    }
}

pub(crate) use super::views_data::*;
