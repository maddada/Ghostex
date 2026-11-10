//! The agent-facing settings catalog and the Ghostex Help reference files built from it.
//!
//! CDXC:AgentSkills 2026-09-09 WHY:
//! The Ghostex Help skill answers "what does this setting do" and "change X for me" from a catalog that is generated from the Settings window's own search rows, option tables, and defaults, plus the hotkey catalog. Generating it (instead of hand-writing a second list) is what keeps the skill, `ghostex settings`, and `ghostex guide` from drifting away from the UI: `cargo xtask help-check` fails when the committed output is stale.
//! SEE-ALSO: server/src/ghostex_cli/settings.rs, server/src/ghostex_cli/guide.rs, tooling/xtask/src/help.rs.

mod render;
mod supplemental;

use std::collections::{HashMap, HashSet};

use crate::availability::{availability_note, platform_defaults};
use crate::data::{
    ADVANCED_MAIN_SETTING_KEYS, MAIN_SETTINGS_SECTION_SETTING_KEYS,
    MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT_STEP, MAX_AGENT_MANAGER_ZOOM_PERCENT,
    MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX, MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    MAX_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT, MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES,
    MAX_SESSION_CHAT_TEXT_STRENGTH_PERCENT, MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
    MAX_SESSION_CHAT_ZOOM_PERCENT, MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
    MAX_SIDEBAR_DEFAULT_WIDTH_PX, MAX_SIDEBAR_TOOLTIP_DELAY_MS, MAX_TERMINAL_PANE_PADDING_PX,
    MAX_TERMINAL_VIEW_WIDTH_PERCENT, MIN_AGENT_MANAGER_ZOOM_PERCENT,
    MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX, MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    MIN_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT, MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES,
    MIN_SESSION_CHAT_TEXT_STRENGTH_PERCENT, MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
    MIN_SESSION_CHAT_ZOOM_PERCENT, MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
    MIN_SIDEBAR_DEFAULT_WIDTH_PX, MIN_SIDEBAR_TOOLTIP_DELAY_MS, MIN_TERMINAL_PANE_PADDING_PX,
    MIN_TERMINAL_VIEW_WIDTH_PERCENT, SESSION_CHAT_TEXT_STRENGTH_PERCENT_STEP,
    SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP, SESSION_CHAT_ZOOM_PERCENT_STEP,
    SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS, SIDEBAR_TOOLTIP_DELAY_STEP_MS,
    TERMINAL_VIEW_WIDTH_PERCENT_STEP,
};
use crate::json::{Json, ToJson, J};
use crate::layout::{general_group, general_navigation, GENERAL_GROUPS};
use crate::rows::{SettingOption, SettingRow};
use crate::{default_value, defaults, extra_pages, general_sections, hotkey_definitions, Platform};
use supplemental::{Tab, ABOUT, SUPPLEMENTAL_ROWS, THEME};

pub use render::{hotkeys_markdown, settings_markdown};

/// Named in the generated files' headers so a reader knows where to make a change.
pub const GENERATOR: &str = "packages/settings-catalog";

/// What a catalog row's value is, as `ghostex settings` and the Help files describe it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Boolean,
    Number,
    String,
    Enum,
    /// A structured value; changed in Settings, not with `ghostex settings set`.
    Json,
    /// A Settings row with no settings key.
    Ui,
}

impl ValueType {
    pub fn as_str(self) -> &'static str {
        match self {
            ValueType::Boolean => "boolean",
            ValueType::Number => "number",
            ValueType::String => "string",
            ValueType::Enum => "enum",
            ValueType::Json => "json",
            ValueType::Ui => "ui",
        }
    }
}

/// One row of the agent-facing settings catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogEntry {
    pub key: String,
    pub title: String,
    pub subtitle: String,
    pub tab: String,
    pub tab_title: String,
    pub group: Option<String>,
    pub group_title: Option<String>,
    pub section: String,
    pub section_title: String,
    pub value_type: ValueType,
    pub default: Option<&'static J>,
    pub options: Vec<SettingOption>,
    pub range: Option<NumberRange>,
    pub advanced: bool,
    pub agent_writable: bool,
    /// Defaults that differ from `default` on one platform (`availability::platform_default`).
    pub platform_defaults: Vec<(Platform, &'static J)>,
    /// A sentence when the row is not shown everywhere (`availability::availability_note`).
    pub availability: Option<String>,
    /// The row's choices come from the running app (`rows::RuntimeOptions`).
    pub runtime_options: Option<crate::rows::RuntimeOptions>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

/// One hotkey of the catalog, as the Help files list it.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogHotkey {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub default_key: &'static str,
    pub windows_linux_default_key: Option<&'static str>,
    /// `ghostex settings hotkeys` resolves a saved map exactly like the desktop: a retired default moves to the current one.
    pub retired_default_keys: &'static [&'static str],
    /// Whether it has a normal binding (outside a terminal) and a Terminal focused one.
    pub normal: bool,
    pub terminal: bool,
    /// The Terminal focused default on macOS and, where it differs, on Windows and Linux.
    pub terminal_default_key: &'static str,
    pub windows_linux_terminal_default_key: Option<&'static str>,
}

pub struct Catalog {
    pub settings: Vec<CatalogEntry>,
    pub hotkeys: Vec<CatalogHotkey>,
}

/// Keys whose value must never be read or written by an agent, whatever their type.
fn agent_denied_key(key: &str) -> bool {
    let lowered = key.to_lowercase();
    ["token", "password", "secret", "apikey", "credential"]
        .iter()
        .any(|word| lowered.contains(word))
}

/// Tabs whose rows are account or pairing state, never plain preferences.
const AGENT_DENIED_TABS: &[&str] = &["accounts", "remote"];

/*
CDXC:Security 2026-11-10 DECISION:
User (Q2140 = B): agents may change every setting again; gxserver's API and `ghostex settings set` write every agent-writable key and the Help marks them so. This supersedes the 2026-11-08 decision (Q1866 "settings only") that made the keys loosening security the owner's alone.
*/

fn number_range(key: &str) -> Option<NumberRange> {
    let range = |min: f64, max: f64| NumberRange {
        min,
        max,
        step: None,
    };
    let stepped = |min: f64, max: f64, step: f64| NumberRange {
        min,
        max,
        step: Some(step),
    };
    Some(match key {
        // Dollars a month; 0 is no cap. Idle and delete take any whole number in range, the
        // Settings menus list the common ones.
        "cloudBoxesMonthlyCapUsd" => stepped(0.0, 10_000.0, 1.0),
        "cloudBoxesIdleStopMinutes" => stepped(1.0, 24.0 * 60.0, 1.0),
        "cloudBoxesDeleteStoppedAfterDays" => stepped(1.0, 365.0, 1.0),
        "agentManagerZoomPercent" => range(
            MIN_AGENT_MANAGER_ZOOM_PERCENT,
            MAX_AGENT_MANAGER_ZOOM_PERCENT,
        ),
        "commandsPanelDefaultHeightPx" => range(
            MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
            MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
        ),
        "customSidebarTitlebarBackgroundDarknessPercent" => range(
            MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
            MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
        ),
        // Files zoom and Interface scale step the same range as Chat zoom.
        "sessionChatZoomPercent" | "filesZoomPercent" | "interfaceScalePercent" => stepped(
            MIN_SESSION_CHAT_ZOOM_PERCENT,
            MAX_SESSION_CHAT_ZOOM_PERCENT,
            SESSION_CHAT_ZOOM_PERCENT_STEP,
        ),
        "sessionChatTextStrengthPercent" => stepped(
            MIN_SESSION_CHAT_TEXT_STRENGTH_PERCENT,
            MAX_SESSION_CHAT_TEXT_STRENGTH_PERCENT,
            SESSION_CHAT_TEXT_STRENGTH_PERCENT_STEP,
        ),
        "markedHandoversHeadingStrength" => stepped(
            MIN_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT,
            MAX_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT,
            MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT_STEP,
        ),
        "sessionChatTranscriptWidthPercent" => stepped(
            MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
            MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
            SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP,
        ),
        "sidebarCollapseAnimationDurationMs" => stepped(
            MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
            MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
            SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS,
        ),
        "projectSwitchKeepAliveMinutes" => range(
            MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES,
            MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES,
        ),
        "sidebarDefaultWidthPx" => {
            range(MIN_SIDEBAR_DEFAULT_WIDTH_PX, MAX_SIDEBAR_DEFAULT_WIDTH_PX)
        }
        "sidebarTooltipDelayMs" => stepped(
            MIN_SIDEBAR_TOOLTIP_DELAY_MS,
            MAX_SIDEBAR_TOOLTIP_DELAY_MS,
            SIDEBAR_TOOLTIP_DELAY_STEP_MS,
        ),
        "terminalPaneHorizontalPaddingPx" | "terminalPaneVerticalPaddingPx" => {
            range(MIN_TERMINAL_PANE_PADDING_PX, MAX_TERMINAL_PANE_PADDING_PX)
        }
        "terminalViewWidthPercent" => stepped(
            MIN_TERMINAL_VIEW_WIDTH_PERCENT,
            MAX_TERMINAL_VIEW_WIDTH_PERCENT,
            TERMINAL_VIEW_WIDTH_PERCENT_STEP,
        ),
        _ => return None,
    })
}

fn value_type(key: &str, has_options: bool) -> ValueType {
    match default_value(key) {
        None => ValueType::Ui,
        Some(J::Bool(_)) => ValueType::Boolean,
        Some(J::Num(_)) => ValueType::Number,
        Some(J::Str(_)) if has_options => ValueType::Enum,
        Some(J::Str(_)) => ValueType::String,
        Some(_) => ValueType::Json,
    }
}

/// Where a row sits: page, group (only the Theme page's `appearance` group), section.
struct Location {
    tab: Tab,
    group: Option<(&'static str, &'static str)>,
    section: String,
    section_title: String,
}

fn catalog_entry(row: &SettingRow, location: &Location) -> CatalogEntry {
    let value_type = value_type(row.key, !row.options.is_empty());
    let primitive = matches!(
        value_type,
        ValueType::Boolean | ValueType::Number | ValueType::String | ValueType::Enum
    );
    let agent_writable =
        primitive && !AGENT_DENIED_TABS.contains(&location.tab.0) && !agent_denied_key(row.key);
    CatalogEntry {
        key: row.key.to_string(),
        title: row.title.clone(),
        subtitle: row.subtitle.clone(),
        tab: location.tab.0.to_string(),
        tab_title: location.tab.1.to_string(),
        group: location.group.map(|(id, _)| id.to_string()),
        group_title: location.group.map(|(_, title)| title.to_string()),
        section: location.section.clone(),
        section_title: location.section_title.clone(),
        value_type,
        default: if value_type == ValueType::Ui {
            None
        } else {
            default_value(row.key)
        },
        options: row.options.clone(),
        range: if value_type == ValueType::Number {
            number_range(row.key)
        } else {
            None
        },
        advanced: row.advanced || ADVANCED_MAIN_SETTING_KEYS.contains(&row.key),
        agent_writable,
        platform_defaults: platform_defaults(row.key),
        availability: availability_note(row.key).or_else(|| {
            crate::built_in_extensions::feature_owning_page(location.tab.0)
                .and_then(crate::built_in_extensions::availability_note)
        }),
        runtime_options: row.runtime_options,
    }
}

fn group_of(id: &'static str) -> Option<(&'static str, &'static str)> {
    general_group(id).map(|group| (group.id, group.title))
}

/// The catalog the Help files and `ghostex settings` describe: macOS wording, every page in Settings order.
pub fn catalog() -> Catalog {
    let platform = Platform::MacOs;
    let mut entries: Vec<CatalogEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |entry: CatalogEntry, entries: &mut Vec<CatalogEntry>| {
        if seen.insert(entry.key.clone()) {
            entries.push(entry);
        }
    };

    let sections = general_sections(platform);
    let section = |id: &str| sections.iter().find(|section| section.id == id);
    let mut place_group =
        |tab: Option<Tab>, group_id: &'static str, entries: &mut Vec<CatalogEntry>| {
            let Some(group) = general_group(group_id) else {
                return;
            };
            for section_id in group.sections {
                let Some(section) = section(section_id) else {
                    continue;
                };
                // A General page is its own tab (CDXC:Settings 2026-10-29 in layout.rs); only the
                // Theme page still renders a group of this catalog.
                let location = Location {
                    tab: tab.unwrap_or((group.id, group.title)),
                    group: tab.map(|_| (group.id, group.title)),
                    section: section.id.to_string(),
                    section_title: section.title.to_string(),
                };
                for row in &section.settings {
                    push(catalog_entry(row, &location), entries);
                }
            }
        };
    for item in general_navigation() {
        place_group(None, item.id, &mut entries);
    }
    // The `appearance` group renders on the Theme page, after the General pages.
    place_group(Some(THEME), "appearance", &mut entries);

    let grouped: HashSet<&str> = GENERAL_GROUPS
        .iter()
        .flat_map(|group| group.sections.iter().copied())
        .collect();
    for section in sections
        .iter()
        .filter(|section| !grouped.contains(section.id))
    {
        // A section outside the navigation groups still renders on one of the
        // General pages; find the page by the keys it lists so the reference matches the
        // Settings layout. A section no page lists goes under About.
        let first_key = section.settings.first().map(|row| row.key);
        let page = first_key.and_then(|key| {
            GENERAL_GROUPS.iter().find(|group| {
                MAIN_SETTINGS_SECTION_SETTING_KEYS
                    .iter()
                    .any(|(id, keys)| *id == group.id && keys.contains(&key))
            })
        });
        let location = Location {
            tab: page.map_or(ABOUT, |page| (page.id, page.title)),
            group: None,
            section: section.id.to_string(),
            section_title: section.title.to_string(),
        };
        for row in &section.settings {
            push(catalog_entry(row, &location), &mut entries);
        }
    }

    for page in extra_pages(platform) {
        for section in &page.sections {
            let location = Location {
                tab: (page.id, page.title),
                group: None,
                section: section.id.to_string(),
                section_title: section.title.to_string(),
            };
            for row in &section.settings {
                push(catalog_entry(row, &location), &mut entries);
            }
        }
    }

    for supplemental in SUPPLEMENTAL_ROWS {
        assert!(
            default_value(supplemental.key).is_some(),
            "SUPPLEMENTAL_ROWS names \"{}\", which is not a settings key.",
            supplemental.key
        );
        let row = crate::rows::row(supplemental.key, supplemental.title, supplemental.subtitle)
            .options(supplemental.options);
        let location = Location {
            tab: supplemental.tab,
            group: supplemental.group.and_then(group_of),
            section: supplemental.section.to_string(),
            section_title: supplemental.section_title.to_string(),
        };
        let mut entry = catalog_entry(&row, &location);
        if supplemental.user_only {
            entry.agent_writable = false;
        }
        push(entry, &mut entries);
    }

    let internal_state = Location {
        tab: ABOUT,
        group: None,
        section: INTERNAL_STATE_SECTION.to_string(),
        section_title: "Internal state (not user settings)".to_string(),
    };
    for (key, _) in defaults() {
        let row = crate::rows::row(
            key,
            *key,
            "App-managed state saved with the settings; not a user preference.",
        );
        let mut entry = catalog_entry(&row, &internal_state);
        entry.agent_writable = false;
        push(entry, &mut entries);
    }

    entries.retain(|entry| crate::built_in_extensions::key_exists_somewhere(&entry.key));
    sort_by_page(&mut entries);

    let hotkeys = hotkey_definitions()
        .iter()
        .filter(|definition| crate::built_in_extensions::hotkey_exists_somewhere(definition.id))
        .map(|definition| CatalogHotkey {
            id: definition.id,
            title: definition.title,
            description: definition.description,
            default_key: definition.default_key,
            windows_linux_default_key: definition.windows_linux_default_key,
            retired_default_keys: definition.retired_default_keys,
            normal: crate::hotkeys::hotkey_has_normal_binding(definition.id),
            terminal: crate::hotkeys::hotkey_has_terminal_binding(definition),
            terminal_default_key: crate::hotkeys::terminal_default_key(definition, Platform::MacOs),
            windows_linux_terminal_default_key: crate::hotkeys::windows_linux_terminal_default_key(
                definition,
            ),
        })
        .collect();

    Catalog {
        settings: entries,
        hotkeys,
    }
}

const INTERNAL_STATE_SECTION: &str = "internalState";

/// Keep every row of a tab, group, and section together in first-seen
/// order, so supplemental rows land beside the page they belong to.
fn sort_by_page(entries: &mut [CatalogEntry]) {
    let group_key = |entry: &CatalogEntry| {
        format!(
            "{}/{}",
            entry.tab,
            entry.group.as_deref().unwrap_or_default()
        )
    };
    let section_key = |entry: &CatalogEntry| format!("{}/{}", group_key(entry), entry.section);
    let mut tab_order: HashMap<String, usize> = HashMap::new();
    let mut group_order: HashMap<String, usize> = HashMap::new();
    let mut section_order: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter() {
        let next = tab_order.len();
        tab_order.entry(entry.tab.clone()).or_insert(next);
        let next = group_order.len();
        group_order.entry(group_key(entry)).or_insert(next);
        let next = section_order.len();
        section_order.entry(section_key(entry)).or_insert(next);
    }
    entries.sort_by_key(|entry| {
        (
            tab_order[&entry.tab],
            group_order[&group_key(entry)],
            if entry.section == INTERNAL_STATE_SECTION {
                usize::MAX
            } else {
                section_order[&section_key(entry)]
            },
        )
    });
}

/// The platform's name in the catalog JSON.
pub fn platform_id(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

impl CatalogEntry {
    fn to_json(&self) -> Json {
        let mut fields = vec![
            ("key", Json::str(&self.key)),
            ("title", Json::str(&self.title)),
            ("subtitle", Json::str(&self.subtitle)),
            ("tab", Json::str(&self.tab)),
            ("tabTitle", Json::str(&self.tab_title)),
        ];
        if let (Some(group), Some(group_title)) = (&self.group, &self.group_title) {
            fields.push(("group", Json::str(group)));
            fields.push(("groupTitle", Json::str(group_title)));
        }
        fields.push(("section", Json::str(&self.section)));
        fields.push(("sectionTitle", Json::str(&self.section_title)));
        fields.push(("type", Json::str(self.value_type.as_str())));
        if let Some(default) = self.default {
            fields.push(("default", default.to_json()));
        }
        if !self.options.is_empty() {
            fields.push((
                "options",
                Json::Arr(
                    self.options
                        .iter()
                        .map(|option| {
                            Json::obj([
                                ("label", Json::str(&option.label)),
                                ("value", Json::str(&option.value)),
                            ])
                        })
                        .collect(),
                ),
            ));
        }
        if let Some(range) = self.range {
            fields.push(("min", Json::Num(range.min)));
            fields.push(("max", Json::Num(range.max)));
            if let Some(step) = range.step.filter(|step| *step != 0.0) {
                fields.push(("step", Json::Num(step)));
            }
        }
        if self.advanced {
            fields.push(("advanced", Json::Bool(true)));
        }
        fields.push(("agentWritable", Json::Bool(self.agent_writable)));
        if !self.platform_defaults.is_empty() {
            fields.push((
                "platformDefaults",
                Json::obj(
                    self.platform_defaults
                        .iter()
                        .map(|(platform, value)| (platform_id(*platform), value.to_json())),
                ),
            ));
        }
        if let Some(note) = &self.availability {
            fields.push(("availability", Json::str(note)));
        }
        if let Some(source) = self.runtime_options {
            fields.push(("runtimeOptions", Json::str(source.as_str())));
        }
        Json::obj(fields)
    }
}

impl CatalogHotkey {
    fn to_json(&self) -> Json {
        let mut fields = vec![
            ("id", Json::str(self.id)),
            ("title", Json::str(self.title)),
            ("description", Json::str(self.description)),
            ("defaultKey", Json::str(self.default_key)),
        ];
        if let Some(key) = self.windows_linux_default_key.filter(|key| !key.is_empty()) {
            fields.push(("windowsLinuxDefaultKey", Json::str(key)));
        }
        if !self.retired_default_keys.is_empty() {
            fields.push(("retiredDefaultKeys", self.retired_default_keys.to_json()));
        }
        if !self.normal {
            fields.push(("terminalOnly", Json::Bool(true)));
        }
        if self.terminal {
            fields.push(("terminalDefaultKey", Json::str(self.terminal_default_key)));
            if let Some(key) = self.windows_linux_terminal_default_key {
                fields.push(("windowsLinuxTerminalDefaultKey", Json::str(key)));
            }
        } else {
            fields.push(("noTerminalBinding", Json::Bool(true)));
        }
        Json::obj(fields)
    }
}

impl Catalog {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::Num(1.0)),
            ("generatedBy", Json::str(GENERATOR)),
            (
                "settings",
                Json::Arr(self.settings.iter().map(CatalogEntry::to_json).collect()),
            ),
            (
                "hotkeys",
                Json::Arr(self.hotkeys.iter().map(CatalogHotkey::to_json).collect()),
            ),
        ])
    }
}

/// `skills/ghostex-help/references/settings-catalog.json`.
pub fn catalog_json() -> String {
    format!("{}\n", catalog().to_json().to_pretty_string())
}

/// Settings keys the committed Help files leave out until the feature they belong to ships in a
/// release; the catalog itself (Settings, `ghostex settings`) keeps them.
///
/// CDXC:GBots 2026-11-07 DECISION:
/// User (Q1544): GBots merges to main off by default, and merging must publish nothing about GBots to installed copies before a release ships it. The Help files under `skills/` reach every install from `main` on its next start (agent_skills_remote.rs), so their GBots rows are held here and the hand-written section waits in the held help patch; the release that ships GBots empties this list and regenerates.
///
/// CDXC:Collab 2026-11-07 DECISION:
/// The coordinator, for the user: Collab merges to main off by default and its Help is held the way GBots' is ("hold Collab's generated Help ... out of the committed Help files ... so merging to main publishes nothing about Collab before a release ships it"); the hand-written section waits in the held help patch.
///
/// CDXC:Changes 2026-11-07 DECISION:
/// The coordinator, for the user: the Changes view's Hide test files setting is held the same way until the release that ships it; the hand-written paragraph waits in the held help patch.
///
/// CDXC:RemoteMachines 2026-11-09 DECISION:
/// The coordinator, for the user (#518): Merge copies of the same repository is held the same way until the release that ships it; the hand-written paragraph waits in the held help patch.
///
/// CDXC:Extensions 2026-11-09 DECISION:
/// The coordinator, for the user: the Grok extension (official_extensions.rs) is held the same way until the release that ships it; its hand-written paragraph waits in the held help patch.
///
/// CDXC:Security 2026-11-09 DECISION:
/// Security logs (Extensions > Experimental) is held the same way; its help, and the help that names Settings > Security, wait in the held help patch.
///
/// CDXC:AppModal 2026-11-10 DECISION:
/// The coordinator, for the user: Help must not change on main before the next release, so Close buttons on the right (Settings > Interface > Advanced, macOS) is held the same way; its hand-written paragraph waits in the held help patch (release-11.3.1/help/close-buttons-left.patch).
pub const HELD_FROM_HELP_FILES: &[&str] = &["closeButtonsOnRight"];

/// Whether gxserver's API (and `ghostex settings set`) may write `key`: the catalog's
/// `agentWritable` (CDXC:Security 2026-11-10 above `number_range`).
pub fn api_may_write(_key: &str, agent_writable: bool) -> bool {
    agent_writable
}

/// Rows whose new wording is held out of the committed Help files until a release ships it: the
/// key, then the title, subtitle and Advanced placement the Help files keep meanwhile. The release
/// that ships the change empties this list and regenerates.
///
/// CDXC:Coordinators 2026-11-09 DECISION:
/// The coordinator, for the user: Settings > Chat > Playbook overrides became the Coordinator
/// playbook row (out of Advanced, with View playbook and Edit overrides); its Help waits in the held
/// help patch like any other, so installed copies keep reading the row they have.
///
/// CDXC:SessionChat 2026-11-10 DECISION:
/// The coordinator, for the user (#535, merged as it came): Transcript width's new subtitle (a share of the window and only a maximum) waits in the held help patch with the rest of that change's Help.
pub const HELD_ROW_WORDING: &[(&str, &str, &str, bool)] = &[];

/// Rows that moved to another page, held at their old place in the committed Help files until a
/// release ships the move: the key, the tab id and title, the section id and title, the title and
/// subtitle the Help files keep, and the key of the row they followed there.
///
/// CDXC:Security 2026-11-10 DECISION:
/// User (Q2173): the Security logs switch moves from Settings > Extensions > Experimental to Settings > Security; installed copies keep reading it where they have it until the release that ships the move empties this list and regenerates (security-b/release-steps.md).
pub const HELD_ROW_PLACEMENT: &[(&str, (&str, &str), (&str, &str), &str, &str, &str)] = &[];

/// The catalog the committed Help files describe: [`catalog`] without [`HELD_FROM_HELP_FILES`],
/// with [`HELD_ROW_WORDING`]'s rows as they read before, and [`HELD_ROW_PLACEMENT`]'s rows back
/// where they were.
pub fn published_catalog() -> Catalog {
    let mut catalog = catalog();
    for (key, (tab, tab_title), (section, section_title), title, subtitle, after) in
        HELD_ROW_PLACEMENT
    {
        let Some(at) = catalog.settings.iter().position(|entry| entry.key == *key) else {
            continue;
        };
        let mut entry = catalog.settings.remove(at);
        entry.tab = (*tab).to_string();
        entry.tab_title = (*tab_title).to_string();
        entry.section = (*section).to_string();
        entry.section_title = (*section_title).to_string();
        entry.title = (*title).to_string();
        entry.subtitle = (*subtitle).to_string();
        let to = catalog
            .settings
            .iter()
            .position(|entry| entry.key == *after)
            .map_or(catalog.settings.len(), |found| found + 1);
        catalog.settings.insert(to, entry);
    }
    catalog
        .settings
        .retain(|entry| !HELD_FROM_HELP_FILES.contains(&entry.key.as_str()));
    for entry in &mut catalog.settings {
        if let Some((_, title, subtitle, advanced)) = HELD_ROW_WORDING
            .iter()
            .find(|(key, ..)| *key == entry.key.as_str())
        {
            entry.title = (*title).to_string();
            entry.subtitle = (*subtitle).to_string();
            entry.advanced = *advanced;
        }
    }
    catalog
}

/// The committed `settings-catalog.json`: [`catalog_json`] without the held keys.
pub fn published_catalog_json() -> String {
    format!("{}\n", published_catalog().to_json().to_pretty_string())
}
