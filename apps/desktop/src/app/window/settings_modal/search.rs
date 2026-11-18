//! Settings search, ported from packages/settings-catalog/src/pages/: the same section, group
//! and page results (a row matches when its title, subtitle, one of its choices or one of its
//! hidden `keywords` matches), and the same Show Advanced visibility rules. Whether a text
//! matches is decided word by word (`SearchPattern`), no longer by Fuse.js's fuzzy match.
use super::catalog::{SearchSectionDef, SettingRowDef, settings_catalog};
use std::collections::{HashMap, HashSet};

/// The shortest query word a single typo is forgiven in. Shorter words would let one typo reach
/// a different word: "spaces" is one letter from "SpaceO", "theme" from "there".
const MIN_TYPO_WORD_LEN: usize = 7;

/// JavaScript `String.prototype.trim`.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

/// The lower-cased words of a text: runs of letters and digits.
fn words_of(text: &str) -> Vec<Vec<char>> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect()
}

/// Whether two words are at most one edit apart (one letter added, dropped, changed, or two
/// neighbours swapped).
fn within_one_edit(a: &[char], b: &[char]) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if long.len() - short.len() > 1 {
        return false;
    }
    let prefix = short.iter().zip(long).take_while(|(x, y)| x == y).count();
    if short.len() == long.len() {
        let rest = prefix + 1;
        // One letter changed, or two neighbours swapped.
        short[rest..] == long[rest..]
            || (rest < short.len()
                && short[prefix] == long[rest]
                && short[rest] == long[prefix]
                && short[rest + 1..] == long[rest + 1..])
    } else {
        // One letter dropped from the longer word.
        short[prefix..] == long[prefix + 1..]
    }
}

/// Whether query word `query` finds text word `word`: the word starts with it (so a word being
/// typed already finds its settings), it is the plural of a name (`plural`: "spaces" finds the
/// title "Go to Space 1"), or, for a long query word, the two (or the start of the word) are one
/// typo apart.
fn word_matches(query: &[char], word: &[char], plural: bool) -> bool {
    if word.starts_with(query) {
        return true;
    }
    if plural && query.len() > 3 && query.last() == Some(&'s') {
        let singular = &query[..query.len() - 1];
        if word == singular
            || (singular.last() == Some(&'e') && word == &singular[..singular.len() - 1])
        {
            return true;
        }
    }
    if query.len() < MIN_TYPO_WORD_LEN || query.first() != word.first() {
        return false;
    }
    (query.len() - 1..=query.len() + 1)
        .filter(|len| *len <= word.len())
        .any(|len| within_one_edit(query, &word[..len]))
}

/// A Settings search query, split into the words a text must hold to match.
///
/// CDXC:Settings 2026-11-15 DECISION:
/// User: "searching for spaces shows a lot of sidebar pages that dont have spaces in the sidebar of
/// settings pls fix". The search was a port of Fuse.js (Bitap, `threshold: 0.24`,
/// `ignoreLocation`): it matched the query anywhere inside a text and forgave one typo in six
/// letters, so "spaces" found "workspaces", "Monospace", "SpaceO", "Spacegray", "spacing" and
/// every passing "a space", and also listed Interface, Chat, Auto Sleep, Editor, Integrations,
/// Open In, Projects, Connectors, Dictation, Voice Agent and Updates. A text now matches when every
/// query word finds one of its words: from the word's start, as the plural of a word in a name
/// (not in a description), or (query words of `MIN_TYPO_WORD_LEN` letters and more) one typo
/// away. The phone's copy of the old matcher
/// (apps/mobile/app/src/screens/settings/search/fuzzyMatch.ts) is unchanged.
struct SearchPattern {
    words: Vec<Vec<char>>,
    /// A query with no letters or digits ("+", "/") matches as plain text.
    raw: String,
}

impl SearchPattern {
    fn new(query: &str) -> Self {
        Self {
            words: words_of(query),
            raw: query.to_lowercase(),
        }
    }

    /// Whether `text` holds every query word. A plural query word finds its singular in a name
    /// (a title, choice or search word), not in a description (`prose`), whose passing "a space"
    /// or "empty space" is not about Spaces.
    fn matches(&self, text: &str, prose: bool) -> bool {
        if self.words.is_empty() {
            return !self.raw.is_empty() && text.to_lowercase().contains(&self.raw);
        }
        let text_words = words_of(text);
        self.words.iter().all(|query| {
            text_words
                .iter()
                .any(|word| word_matches(query, word, !prose))
        })
    }
}

/// `SettingsSectionSearchResult`.
#[derive(Clone, Debug, Default)]
pub(crate) struct SectionSearch {
    pub(crate) is_searching: bool,
    pub(crate) section_matches: bool,
    /// Set by a grouped search: the group title itself matched.
    pub(crate) group_title_matches: bool,
    pub(crate) visible_keys: HashSet<String>,
}

impl SectionSearch {
    /// `hasVisibleSettingsSearchResult`.
    pub(crate) fn has_visible(&self) -> bool {
        self.section_matches || !self.visible_keys.is_empty()
    }
}

fn item_matches(
    pattern: &SearchPattern,
    title: &str,
    subtitle: &str,
    options: &[String],
    keywords: &[String],
) -> bool {
    let name = |text: &&str| !js_trim(text).is_empty() && pattern.matches(text, false);
    [title].iter().any(name)
        || (!js_trim(subtitle).is_empty() && pattern.matches(subtitle, true))
        || keywords
            .iter()
            .map(String::as_str)
            .any(|keyword| name(&keyword))
        || options
            .iter()
            .map(String::as_str)
            .any(|option| name(&option))
}

/// `getSettingsSectionSearch(query, sectionTitle, settings)`.
pub(crate) fn section_search(
    query: &str,
    section_title: &str,
    rows: &[SettingRowDef],
) -> SectionSearch {
    let trimmed = js_trim(query);
    if trimmed.is_empty() {
        return SectionSearch {
            is_searching: false,
            section_matches: true,
            group_title_matches: false,
            visible_keys: rows.iter().map(|row| row.key.clone()).collect(),
        };
    }
    let pattern = SearchPattern::new(trimmed);
    let section_matches = item_matches(&pattern, section_title, "", &[], &[]);
    let visible_keys = rows
        .iter()
        .filter(|row| {
            let options: Vec<String> = row
                .options
                .iter()
                .flat_map(|option| [option.label.clone(), option.value.clone()])
                .collect();
            item_matches(&pattern, &row.title, &row.subtitle, &options, &row.keywords)
        })
        .map(|row| row.key.clone())
        .collect();
    SectionSearch {
        is_searching: true,
        section_matches,
        group_title_matches: false,
        visible_keys,
    }
}

/// `getSettingsSectionSearch(query, title, []).sectionMatches`: whether a page or group title matches.
pub(crate) fn title_matches(query: &str, title: &str) -> bool {
    section_search(query, title, &[]).section_matches
}

/// `getGroupedSettingsSectionSearch(query, sectionTitle, sections)`.
pub(crate) fn grouped_search(
    query: &str,
    title: &str,
    sections: &[&SectionSearch],
) -> SectionSearch {
    let group = section_search(query, title, &[]);
    let mut visible_keys = group.visible_keys.clone();
    for section in sections {
        visible_keys.extend(section.visible_keys.iter().cloned());
    }
    SectionSearch {
        group_title_matches: group.section_matches,
        is_searching: group.is_searching || sections.iter().any(|section| section.is_searching),
        section_matches: group.section_matches
            || sections.iter().any(|section| section.section_matches),
        visible_keys,
    }
}

/// CDXC:Settings 2026-10-29 DECISION:
/// User: "we should remove show advanced from bottom left, would be good idea to move to group
/// advanced settings at the bottom of each page together", in "a new section that's collapsed by
/// default at the bottom ... called Advanced". The rail's Show Advanced switch and the
/// `showAdvancedSettings` key are gone; the rows of `ADVANCED_MAIN_SETTING_KEYS` render in a
/// second pass into a collapsed Advanced fold at the bottom of their page (tabs/general/mod.rs,
/// tabs/about.rs). A search shows every matching row in its own section, as before, and the fold
/// stays out. This supersedes Q6 ("keep mode") of the same day.
///
/// Which rows a render pass draws: the everyday rows (the page body) or the advanced rows (the
/// Advanced fold). `None` draws both, for the search's own bookkeeping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowTier {
    Everyday,
    Advanced,
}

/// `shouldShowSettingsSection(result, tier)`: whether a section has a row for this pass.
pub(crate) fn should_show_section(result: &SectionSearch, tier: Option<RowTier>) -> bool {
    if !result.has_visible() {
        return false;
    }
    if result.is_searching {
        return tier != Some(RowTier::Advanced);
    }
    let catalog = settings_catalog();
    match tier {
        None => true,
        Some(RowTier::Everyday) => result
            .visible_keys
            .iter()
            .any(|key| !catalog.is_advanced(key)),
        Some(RowTier::Advanced) => result
            .visible_keys
            .iter()
            .any(|key| catalog.is_advanced(key)),
    }
}

/// `shouldShowSetting(result, settingKey, tier)`: whether a row belongs to this pass.
pub(crate) fn should_show_setting(
    result: &SectionSearch,
    key: &str,
    tier: Option<RowTier>,
) -> bool {
    if result.is_searching {
        return tier != Some(RowTier::Advanced)
            && (result.section_matches || result.visible_keys.contains(key));
    }
    match tier {
        None => true,
        Some(RowTier::Everyday) => !settings_catalog().is_advanced(key),
        Some(RowTier::Advanced) => settings_catalog().is_advanced(key),
    }
}

/// `SettingsTabSearch` for the pages other than General, Theme and Hotkeys.
#[derive(Clone, Debug, Default)]
pub(crate) struct TabSearch {
    pub(crate) sections: HashMap<String, SectionSearch>,
    pub(crate) tab: SectionSearch,
}

impl TabSearch {
    /// `settingsTabSearchHasMatches`.
    pub(crate) fn has_matches(&self) -> bool {
        self.tab.has_visible()
    }

    /// The search result of one of this page's sections; everything visible when it is unknown.
    pub(crate) fn section(&self, id: &str) -> SectionSearch {
        self.sections.get(id).cloned().unwrap_or(SectionSearch {
            section_matches: true,
            ..SectionSearch::default()
        })
    }

    /// A page's row visibility: `result.isSearching ? shouldShowSetting(result, key) : true`.
    pub(crate) fn row_visible(&self, section: &str, key: &str) -> bool {
        let result = self.section(section);
        !result.is_searching || result.section_matches || result.visible_keys.contains(key)
    }
}

/// `getExtraSettingsTabSearch(query, tab, debuggingMode)`.
/// `hidden_keys` are rows a search leaves out on every page (a built-in extension's rows while it
/// is off, such as the voice agent's call rows on Settings > Voice Agent).
pub(crate) fn extra_tab_search(
    query: &str,
    tab: &str,
    debugging_mode: bool,
    hidden_keys: &[&str],
) -> TabSearch {
    let catalog = settings_catalog();
    let Some(definition) = catalog.extra_tab(tab) else {
        return TabSearch::default();
    };
    let tab_title_matches = title_matches(query, &definition.title);
    let mut sections = HashMap::new();
    for section in &definition.sections {
        let rows: Vec<SettingRowDef> = if tab == "debugging" && !debugging_mode {
            section
                .settings
                .iter()
                // Use agent hooks shows whatever Show debug UI controls says (debugging.rs).
                .filter(|row| matches!(row.key.as_str(), "debuggingMode" | "useAgentHooks"))
                .cloned()
                .collect()
        } else {
            section.settings.clone()
        };
        // Rows this platform does not have (SpaceO's off macOS), and rows of a built-in extension
        // that is off, never match a search.
        let rows: Vec<SettingRowDef> = rows
            .into_iter()
            .filter(|row| {
                !hidden_keys.contains(&row.key.as_str())
                    && !ghostex_settings_catalog::availability::row_hidden(
                        ghostex_settings_catalog::Platform::current(),
                        &row.key,
                        |_| None,
                    )
            })
            .collect();
        let title = if rows.is_empty() {
            ""
        } else {
            section.title.as_str()
        };
        let mut result = section_search(query, title, &rows);
        if tab_title_matches {
            result.section_matches = true;
        }
        sections.insert(section.id.clone(), result);
    }
    let ordered: Vec<&SectionSearch> = definition
        .sections
        .iter()
        .filter_map(|section| sections.get(&section.id))
        .collect();
    let tab_result = grouped_search(query, &definition.title, &ordered);
    TabSearch {
        sections,
        tab: tab_result,
    }
}

/// The search of the pages General split into (and Theme): every catalog section, the page groups,
/// and the visibility rules of `createMainSettingsVisibility`.
#[derive(Clone, Debug, Default)]
pub(crate) struct GeneralSearch {
    pub(crate) query: String,
    /// The rows this copy of the search shows (`RowTier`); `None` for every row.
    pub(crate) tier: Option<RowTier>,
    /// Rows of a built-in extension that is off, and the Space rows while Spaces is off.
    pub(crate) hidden_keys: Vec<&'static str>,
    /// `settingsSearch[sectionId]`.
    pub(crate) sections: HashMap<String, SectionSearch>,
    /// `mainSettingsGroupSearch[groupId]`.
    pub(crate) groups: HashMap<String, SectionSearch>,
}

impl GeneralSearch {
    pub(crate) fn new(query: &str, hidden_keys: Vec<&'static str>) -> Self {
        let catalog = settings_catalog();
        let sections: HashMap<String, SectionSearch> = catalog
            .general_sections
            .iter()
            .map(|section: &SearchSectionDef| {
                let settings: Vec<SettingRowDef> = section
                    .settings
                    .iter()
                    .filter(|row| !hidden_keys.contains(&row.key.as_str()))
                    .cloned()
                    .collect();
                (
                    section.id.clone(),
                    section_search(query, &section.title, &settings),
                )
            })
            .collect();
        let groups = catalog
            .general_groups
            .iter()
            .map(|group| {
                let result = if group.sections.len() == 1 {
                    sections
                        .get(&group.sections[0])
                        .cloned()
                        .unwrap_or_default()
                } else {
                    let members: Vec<&SectionSearch> = group
                        .sections
                        .iter()
                        .filter_map(|id| sections.get(id))
                        .collect();
                    grouped_search(query, &group.title, &members)
                };
                (group.id.clone(), result)
            })
            .collect();
        Self {
            query: query.to_string(),
            tier: None,
            hidden_keys,
            sections,
            groups,
        }
    }

    /// This search limited to one pass's rows.
    pub(crate) fn with_tier(mut self, tier: RowTier) -> Self {
        self.tier = Some(tier);
        self
    }

    pub(crate) fn is_searching(&self) -> bool {
        !js_trim(&self.query).is_empty()
    }

    /// Whether this is the Advanced fold's pass. A row that is not a setting (a link to another
    /// page, a button strip) belongs to the page body's pass only, or it draws twice with the
    /// same element ids once the fold opens.
    pub(crate) fn is_advanced_pass(&self) -> bool {
        self.tier == Some(RowTier::Advanced)
    }

    /// Whether a row that stands for several settings (Card details, Animations, Transcript
    /// width) belongs to this pass: while browsing, the tier of its first key decides, so the row
    /// renders once; a search shows it when any of its keys matches.
    pub(crate) fn any_visible(&self, section: &str, keys: &[&str]) -> bool {
        if self.is_searching() {
            return keys.iter().any(|key| self.setting_visible(section, key));
        }
        keys.first()
            .is_some_and(|key| self.setting_visible(section, key))
    }

    pub(crate) fn section(&self, id: &str) -> SectionSearch {
        self.sections.get(id).cloned().unwrap_or_default()
    }

    fn group(&self, id: &str) -> SectionSearch {
        self.groups.get(id).cloned().unwrap_or_default()
    }

    /// `settingMatchesGroupedSectionTitle`: a query that names a multi-section group reveals all of its rows.
    fn setting_matches_group_title(&self, key: &str) -> bool {
        let catalog = settings_catalog();
        catalog.general_groups.iter().any(|group| {
            self.groups
                .get(&group.id)
                .is_some_and(|result| result.group_title_matches)
                && catalog
                    .string_list(
                        super::catalog::module::SETTINGS_TYPES,
                        "MAIN_SETTINGS_SECTION_SETTING_KEYS",
                        Some(&group.id),
                    )
                    .iter()
                    .any(|group_key| group_key == key)
        })
    }

    /// `mainSettingVisible(settingsSearch[section], key)`.
    /// CDXC:Settings 2026-10-24 WHY: `hidden_keys` (a built-in extension that is off, a row this platform lacks) is checked here for every page, not only through the search filter: before, a page with no search typed drew those rows anyway, so a Mac with Read aloud off showed Voice, Speed and the Linux/Windows note that this computer can't speak.
    pub(crate) fn setting_visible(&self, section: &str, key: &str) -> bool {
        if self.hidden_keys.contains(&key) {
            return false;
        }
        if self.is_searching() && self.setting_matches_group_title(key) {
            return true;
        }
        should_show_setting(&self.section(section), key, self.tier)
    }

    /// `mainSectionVisible(groupId, result)` for a page's group: whether anything of it shows.
    pub(crate) fn group_visible(&self, group: &str) -> bool {
        should_show_section(&self.group(group), self.tier)
    }

    /// `mainSubsectionVisible(sectionId, settingsSearch[sectionId])`. `power_visible` is whether
    /// Keep Awake is on (its Power settings: CDXC:Extensions 2026-11-09 in the settings catalog).
    pub(crate) fn subsection_visible(&self, section: &str, power_visible: bool) -> bool {
        if section == "power" && !power_visible {
            return false;
        }
        let catalog = settings_catalog();
        if self.is_searching()
            && catalog
                .string_list(
                    super::catalog::module::SETTINGS_TYPES,
                    "MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS",
                    Some(section),
                )
                .iter()
                .any(|key| self.setting_matches_group_title(key))
        {
            return true;
        }
        should_show_section(&self.section(section), self.tier)
    }

    /// `mainSectionVisible(sectionId, settingsSearch[sectionId])` for a section with no rail
    /// subsections of its own (Chat, Status Indicators).
    pub(crate) fn section_visible(&self, section: &str) -> bool {
        should_show_section(&self.section(section), self.tier)
    }
}

/// The anchor a General scroll target lands on (`mainSettingsSectionRefs`): a page's group id
/// scrolls to its first section (Files & Links to File opening, Power & Sleep to Auto Sleep,
/// Notifications to Sounds, the old Advanced group to Sleeping Sessions), a section to itself.
pub(crate) fn general_scroll_anchor(target: &str) -> &str {
    match target {
        "tools" => "fileOpening",
        "system" => "autoSleep",
        "sounds" => "notifications",
        "advanced" => "sleepingSessions",
        other => other,
    }
}

/// The page General split into that renders scroll target `target` (a group id such as `chat`,
/// a section id such as `sessionCards`, or a section anchor such as `notifications`), for deep
/// links (CDXC:Settings 2026-10-29 in model.rs).
pub(crate) fn general_page_of_section(target: &str) -> Option<super::model::SettingsTabId> {
    use super::model::SettingsTabId;
    let group = general_group_of(target);
    SettingsTabId::general_page(&group).or_else(|| {
        settings_catalog()
            .general_groups
            .iter()
            .find(|group| group.sections.iter().any(|section| section == target))
            .and_then(|group| SettingsTabId::general_page(&group.id))
    })
}

/// `getMainSettingsSectionGroupId`: the rail group (now a page's group id) that owns a General
/// scroll target.
pub(crate) fn general_group_of(scroll_target: &str) -> String {
    settings_catalog()
        .module_value(
            super::catalog::module::SETTINGS_TYPES,
            "MAIN_SETTINGS_SUBSECTION_PARENT_IDS",
        )
        .and_then(|parents| parents.get(scroll_target))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(scroll_target)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finds_row(query: &str, section: &str, key: &str) -> bool {
        GeneralSearch::new(query, Vec::new()).setting_visible(section, key)
    }

    #[test]
    fn the_monospace_font_is_found_by_its_name_and_its_old_one() {
        for query in ["monospace", "monospace font", "font family", "ghostty font"] {
            assert!(
                finds_row(query, "textFonts", "terminalFontFamily"),
                "{query}"
            );
        }
    }

    #[test]
    fn the_moved_chat_rows_are_found_on_the_interface_pages() {
        assert!(finds_row(
            "text strength",
            "textFonts",
            "sessionChatTextStrengthPercent"
        ));
        assert!(finds_row("chat zoom", "sizeZoom", "sessionChatZoomPercent"));
        assert!(finds_row(
            "thinking in grey",
            "textFonts",
            "sessionChatMutedThinking"
        ));
    }

    #[test]
    fn the_interface_page_is_found_by_its_new_name_and_its_old_one() {
        for query in ["interface", "theme"] {
            assert!(
                GeneralSearch::new(query, Vec::new()).subsection_visible("theming", true),
                "{query}"
            );
        }
    }

    /// CDXC:Settings 2026-11-15: "spaces" lists only the pages that talk about Spaces, not the ones
    /// holding "workspaces", "Monospace", "SpaceO", "Spacegray" or a passing "empty space".
    #[test]
    fn spaces_finds_only_the_pages_about_spaces() {
        let general = GeneralSearch::new("spaces", Vec::new());
        assert!(general.subsection_visible("spaces", true));
        assert!(general.subsection_visible("sidebar", true));
        for section in [
            "theming",
            "textFonts",
            "chat",
            "autoSleep",
            "terminal",
            "editor",
        ] {
            assert!(!general.subsection_visible(section, true), "{section}");
        }
        for tab in ["sync", "workspaces", "agents"] {
            assert!(
                extra_tab_search("spaces", tab, false, &[]).has_matches(),
                "{tab}"
            );
        }
        for tab in [
            "integrations",
            "openTargets",
            "projects",
            "connectors",
            "dictation",
            "voiceAgent",
            "updates",
        ] {
            assert!(
                !extra_tab_search("spaces", tab, false, &[]).has_matches(),
                "{tab}"
            );
        }
    }

    #[test]
    fn a_word_matches_from_its_start_as_a_plural_name_or_one_typo_off() {
        let finds = |query: &str, text: &str| SearchPattern::new(query).matches(text, false);
        assert!(finds("notif", "Notifications"));
        assert!(finds("notifcations", "Notifications"));
        assert!(finds("spaces", "Go to Space 1"));
        assert!(!SearchPattern::new("spaces").matches("empty sidebar space", true));
        assert!(!finds("spaces", "Workspaces"));
        assert!(!finds("spaces", "SpaceO"));
        assert!(!finds("theme", "Show them there"));
        assert!(finds("font family", "Font family for terminals"));
        assert!(!finds("font zebra", "Font family for terminals"));
        assert!(finds("+", "Cmd+K"));
    }
}
