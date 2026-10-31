//! Settings search, ported from packages/settings-catalog/src/pages/: the same Fuse.js fuzzy
//! match (7.3.0, `threshold: 0.24`, `ignoreLocation`, keys title/subtitle/options), the same
//! section, group and page results, and the same Show Advanced visibility rules.
//!
//! Only whether a row matches matters to Settings (never the ranking), and Fuse includes a row
//! when any one of its keys matches, so this ports Fuse's Bitap matcher and its per-key
//! membership test, not its scoring.
use super::catalog::{SearchSectionDef, SettingRowDef, settings_catalog};
use std::collections::{HashMap, HashSet};

/// Fuse's `threshold` in `getSettingsSectionSearch`.
const SEARCH_THRESHOLD: f64 = 0.24;
/// Fuse's machine word size: longer patterns are searched in 32-character chunks.
const MAX_BITS: usize = 32;

/// JavaScript `String.prototype.trim`.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

/// A lower-cased Fuse pattern, split into the chunks Fuse searches.
struct BitapPattern {
    pattern: Vec<u16>,
    chunks: Vec<(Vec<u16>, HashMap<u16, u32>, usize)>,
}

impl BitapPattern {
    fn new(query: &str) -> Self {
        let pattern: Vec<u16> = query.to_lowercase().encode_utf16().collect();
        let mut chunks = Vec::new();
        let add = |chunks: &mut Vec<(Vec<u16>, HashMap<u16, u32>, usize)>,
                   chunk: &[u16],
                   start: usize| {
            let len = chunk.len();
            let mut alphabet: HashMap<u16, u32> = HashMap::new();
            for (index, unit) in chunk.iter().enumerate() {
                *alphabet.entry(*unit).or_default() |= 1u32 << (len - index - 1);
            }
            chunks.push((chunk.to_vec(), alphabet, start));
        };
        let len = pattern.len();
        if len > MAX_BITS {
            let remainder = len % MAX_BITS;
            let end = len - remainder;
            let mut index = 0;
            while index < end {
                add(&mut chunks, &pattern[index..index + MAX_BITS], index);
                index += MAX_BITS;
            }
            if remainder > 0 {
                let start = len - MAX_BITS;
                add(&mut chunks, &pattern[start..], start);
            }
        } else if len > 0 {
            add(&mut chunks, &pattern, 0);
        }
        Self { pattern, chunks }
    }

    /// `BitapSearch.searchIn(text).isMatch`.
    fn matches(&self, text: &str) -> bool {
        let text: Vec<u16> = text.to_lowercase().encode_utf16().collect();
        if self.pattern == text {
            return true;
        }
        self.chunks
            .iter()
            .any(|(pattern, alphabet, start)| bitap_is_match(&text, pattern, alphabet, *start))
    }
}

fn index_of(text: &[u16], pattern: &[u16], from: usize) -> Option<usize> {
    if pattern.is_empty() {
        return (from <= text.len()).then_some(from);
    }
    if pattern.len() > text.len() {
        return None;
    }
    (from..=text.len() - pattern.len())
        .find(|&index| &text[index..index + pattern.len()] == pattern)
}

/// Fuse's `search()` with `ignoreLocation: true`, `findAllMatches: false`, `minMatchCharLength: 1`
/// and `includeMatches: false`, reduced to its `isMatch`. With `ignoreLocation` a score is just
/// `errors / patternLen`; everything else (the location window, the early exits) is kept as Fuse
/// has it so the same texts match.
fn bitap_is_match(
    text: &[u16],
    pattern: &[u16],
    alphabet: &HashMap<u16, u32>,
    location: usize,
) -> bool {
    let pattern_len = pattern.len();
    let text_len = text.len();
    let expected_location = location.min(text_len) as i64;
    let score = |errors: usize| errors as f64 / pattern_len as f64;
    let mut current_threshold = SEARCH_THRESHOLD;
    let mut best_location = expected_location;
    // Exact matches first: each one lowers the threshold to 0.
    while let Some(index) = index_of(text, pattern, best_location.max(0) as usize) {
        current_threshold = current_threshold.min(0.0);
        best_location = (index + pattern_len) as i64;
    }
    best_location = -1;
    let mut last_bit_arr: Vec<u32> = Vec::new();
    let mut bin_max = (pattern_len + text_len) as i64;
    let mask: u32 = 1u32 << (pattern_len - 1);
    for errors in 0..pattern_len {
        let mut bin_min: i64 = 0;
        let mut bin_mid = bin_max;
        while bin_min < bin_mid {
            if score(errors) <= current_threshold {
                bin_min = bin_mid;
            } else {
                bin_max = bin_mid;
            }
            bin_mid = ((bin_max - bin_min) as f64 / 2.0 + bin_min as f64).floor() as i64;
        }
        bin_max = bin_mid;
        let mut start = (expected_location - bin_mid + 1).max(1);
        let finish = (expected_location + bin_mid).min(text_len as i64) + pattern_len as i64;
        let mut bit_arr: Vec<u32> = vec![0; (finish + 2).max(0) as usize];
        if let Some(last) = bit_arr.get_mut((finish + 1) as usize) {
            *last = (1u64 << errors).wrapping_sub(1) as u32;
        }
        let get = |array: &Vec<u32>, index: i64| -> u32 {
            if index < 0 {
                0
            } else {
                array.get(index as usize).copied().unwrap_or(0)
            }
        };
        let mut j = finish;
        while j >= start {
            let current_location = j - 1;
            let char_match = if current_location >= 0 {
                text.get(current_location as usize)
                    .and_then(|unit| alphabet.get(unit))
                    .copied()
                    .unwrap_or(0)
            } else {
                0
            };
            let mut value = ((get(&bit_arr, j + 1) << 1) | 1) & char_match;
            if errors > 0 {
                value |= ((get(&last_bit_arr, j + 1) | get(&last_bit_arr, j)) << 1)
                    | 1
                    | get(&last_bit_arr, j + 1);
            }
            bit_arr[j as usize] = value;
            if value & mask != 0 {
                let final_score = score(errors);
                if final_score <= current_threshold {
                    current_threshold = final_score;
                    best_location = current_location;
                    if best_location <= expected_location {
                        break;
                    }
                    start = (2 * expected_location - best_location).max(1);
                }
            }
            j -= 1;
        }
        if score(errors + 1) > current_threshold {
            break;
        }
        last_bit_arr = bit_arr;
    }
    best_location >= 0
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

fn fuse_item_matches(pattern: &BitapPattern, fields: &[&str], options: &[String]) -> bool {
    fields
        .iter()
        .filter(|field| !js_trim(field).is_empty())
        .any(|field| pattern.matches(field))
        || options
            .iter()
            .filter(|option| !js_trim(option).is_empty())
            .any(|option| pattern.matches(option))
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
    let pattern = BitapPattern::new(trimmed);
    let section_matches = fuse_item_matches(&pattern, &[section_title], &[]);
    let visible_keys = rows
        .iter()
        .filter(|row| {
            let options: Vec<String> = row
                .options
                .iter()
                .flat_map(|option| [option.label.clone(), option.value.clone()])
                .collect();
            fuse_item_matches(&pattern, &[&row.title, &row.subtitle], &options)
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
/// is off, such as the voice agent's call rows on Settings > Voice agent).
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
                .filter(|row| row.key == "debuggingMode")
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
    pub(crate) show_experimental: bool,
    /// Rows of a built-in extension that is off (the Space rows while Spaces is off).
    pub(crate) hidden_keys: Vec<&'static str>,
    /// `settingsSearch[sectionId]`.
    pub(crate) sections: HashMap<String, SectionSearch>,
    /// `mainSettingsGroupSearch[groupId]`.
    pub(crate) groups: HashMap<String, SectionSearch>,
}

impl GeneralSearch {
    pub(crate) fn new(
        query: &str,
        show_experimental: bool,
        hidden_keys: Vec<&'static str>,
    ) -> Self {
        let catalog = settings_catalog();
        let sections: HashMap<String, SectionSearch> = catalog
            .general_sections
            .iter()
            .map(|section: &SearchSectionDef| {
                let settings: Vec<SettingRowDef> = section
                    .settings
                    .iter()
                    .filter(|row| {
                        row.key != "terminalShadersEnabled"
                            || (cfg!(target_os = "macos") && show_experimental)
                    })
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
            show_experimental,
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
        if key == "terminalShadersEnabled" && !(cfg!(target_os = "macos") && self.show_experimental)
        {
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

    /// `mainSubsectionVisible(sectionId, settingsSearch[sectionId])`. `power_visible` is
    /// `showBetaFeatures`.
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
        GeneralSearch::new(query, true, Vec::new()).setting_visible(section, key)
    }

    #[test]
    fn the_monospace_font_is_found_by_its_name_and_its_old_one() {
        for query in ["monospace", "monospace font", "font family", "ghostty font"] {
            assert!(finds_row(query, "theming", "terminalFontFamily"), "{query}");
        }
    }

    #[test]
    fn the_moved_chat_rows_are_found_on_the_interface_page() {
        assert!(finds_row(
            "text strength",
            "theming",
            "sessionChatTextStrengthPercent"
        ));
        assert!(finds_row(
            "thinking in grey",
            "theming",
            "sessionChatMutedThinking"
        ));
    }

    #[test]
    fn the_interface_page_is_found_by_its_new_name_and_its_old_one() {
        for query in ["interface", "theme"] {
            assert!(
                GeneralSearch::new(query, true, Vec::new()).subsection_visible("theming", true),
                "{query}"
            );
        }
    }
}
