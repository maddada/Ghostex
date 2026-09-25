//! Session tags: built-in labels and glyphs, the custom tag catalog, tag filters, and the
//! user-ordered tag filter list from settings.
//!
//! SEE-ALSO: packages/shared/session-tags.ts. (The Quick Access half, the deleted
//! `native-quick-access/tag-presentation.ts`, is ported here too.)

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_protocol::CustomSessionTagsState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::text::{collapse_js_whitespace, js_trim, utf16_prefix};

/// The filter value that matches sessions without a tag. Filter chrome only, never a stored tag.
pub const UNTAGGED_TAG_FILTER: &str = "untagged";

const BUILTIN_TAGS: &[&str] = &[
    "favorite",
    "high-priority",
    "low-priority",
    "todo",
    "research",
    "in-progress",
    "testing",
    "blocked",
    "on-hold",
    "done",
    "bug",
    "feature",
    "design",
];

/// `(section, tag, label)` in `SIDEBAR_SESSION_TAG_SECTIONS` order.
const TAG_OPTIONS: &[(u8, &str, &str)] = &[
    (0, "favorite", "Favorite"),
    (0, "high-priority", "High Priority"),
    (0, "low-priority", "Low Priority"),
    (1, "todo", "Todo"),
    (1, "in-progress", "In Progress"),
    (1, "testing", "Testing"),
    (1, "blocked", "Blocked"),
    (1, "on-hold", "On Hold"),
    (1, "done", "Done"),
    (2, "research", "Research"),
    (2, "bug", "Bug"),
    (2, "feature", "Feature"),
    (2, "design", "Design"),
];

/// Built-in glyph and color of each tag (`tag-presentation.ts`).
fn builtin_presentation(tag: &str) -> Option<(&'static str, &'static str)> {
    Some(match tag {
        "favorite" => ("star-filled", "#f3cd5f"),
        "high-priority" => ("alert-triangle", "#ff8b6b"),
        "low-priority" => ("arrow-down", "#8e949d"),
        "research" => ("microscope", "#8fb8ff"),
        "todo" => ("checkbox", "#d9dee6"),
        "in-progress" => ("player-play", "#4ee6b8"),
        "testing" => ("test-pipe", "#59d9ff"),
        "blocked" => ("barrier-block", "#ff5f73"),
        "on-hold" => ("player-pause", "#d2a7ff"),
        "done" => ("circle-check", "#95d7f6"),
        "bug" => ("bug", "#a54646"),
        "feature" => ("puzzle", "#f0c66e"),
        "design" => ("palette", "#ff9ee7"),
        "untagged" => ("tag-off", "#acb6c0"),
        _ => return None,
    })
}

/// Filters the first-run tag list hides and disables.
const DEFAULT_OFF_TAGS: &[&str] = &["high-priority", "low-priority", "todo", "bug", "feature"];

const SEPARATOR_IDS: &[&str] = &[
    "separator-priority-progress",
    "separator-progress-type",
    "separator-type-untagged",
];

/// `SESSION_TAG_COLOR_PRESETS` values, the fallback rotation for a custom tag without a color.
const TAG_COLOR_PRESETS: &[&str] = &[
    "#f3cc5f", "#ff8b6b", "#f0c66e", "#4ee6b8", "#59d9ff", "#95d7f6", "#8fb8ff", "#d2a7ff",
    "#ff9ee7", "#ff5f73", "#a54646", "#d9dee6", "#8e949d",
];

const MAX_CUSTOM_TAGS: usize = 64;
const MAX_CUSTOM_TAG_NAME_UTF16: usize = 40;

/// Icon and color a tagged row draws.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagPresentation {
    pub icon: String,
    pub icon_color: String,
}

/// One custom tag after client normalization.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CustomTag {
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) color: String,
}

/// A custom tag catalog: one machine's document, or several merged for a lookup.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// The TypeScript uses three different catalogs and the difference only shows with a second
/// machine, so this crate used one for all three until M4d made remote catalogs reachable.
/// `findCustomSessionTag` resolves a tag id against EVERY machine's catalog with this computer's
/// first (`getSessionTagCatalogs`), which is what a label and a tag icon read;
/// `normalizeSidebarSessionTagListItems` is handed THIS COMPUTER's alone, which is what decides
/// which filter rows the Sort & Filter menu offers and which ticked filters survive a prune; and a
/// row's own tag submenu is handed the catalog of the machine that row is on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TagCatalog {
    /// In catalog order.
    pub(crate) order: Vec<String>,
    pub(crate) tags: BTreeMap<String, CustomTag>,
}

impl TagCatalog {
    /// Several catalogs as one, for a LOOKUP. A tag id that several machines define resolves to
    /// the first catalog that has it, which is `findCustomSessionTag`'s rule with this computer's
    /// catalog passed first.
    pub(crate) fn merged<'a>(
        states: impl IntoIterator<Item = Option<&'a CustomSessionTagsState>>,
    ) -> Self {
        let mut merged = Self::default();
        for state in states {
            let catalog = Self::from_state(state);
            for id in &catalog.order {
                if merged.tags.contains_key(id) {
                    continue;
                }
                let Some(tag) = catalog.tags.get(id) else {
                    continue;
                };
                merged.order.push(id.clone());
                merged.tags.insert(id.clone(), tag.clone());
            }
        }
        merged
    }

    /// `normalizeCustomSessionTagsState`: the order array is authoritative, tags missing from it
    /// follow in map order, and every kept tag has a bounded name, an icon, and a lowercase
    /// `#rrggbb` color.
    pub(crate) fn from_state(state: Option<&CustomSessionTagsState>) -> Self {
        let Some(state) = state else {
            return Self::default();
        };
        let mut candidates: Vec<(String, &ghostex_gx_protocol::CustomSessionTag)> = Vec::new();
        // A tag the `order` array does not name follows in id order here, where `Object.entries`
        // gives the TypeScript the document's own order, so the two can list the tag filters in a
        // different sequence. Accepted rather than fixed: the wire type is a `BTreeMap`, so the
        // document order is gone before this runs, and every document the daemon writes has an
        // `order` array naming every tag it stores.
        for (raw_id, tag) in &state.tags {
            let tag_id = js_trim(raw_id);
            if is_custom_tag_id(tag_id) && !candidates.iter().any(|(id, _)| id == tag_id) {
                candidates.push((tag_id.to_string(), tag));
            }
        }
        let mut ordered: Vec<String> = Vec::new();
        for entry in &state.order {
            let id = js_trim(entry);
            if candidates.iter().any(|(candidate, _)| candidate == id)
                && !ordered.iter().any(|seen| seen == id)
            {
                ordered.push(id.to_string());
            }
        }
        for (id, _) in &candidates {
            if !ordered.contains(id) {
                ordered.push(id.clone());
            }
        }
        let mut catalog = Self::default();
        for tag_id in ordered {
            if catalog.order.len() >= MAX_CUSTOM_TAGS {
                break;
            }
            let Some((_, raw)) = candidates.iter().find(|(id, _)| *id == tag_id) else {
                continue;
            };
            let icon = js_trim(&raw.icon);
            let icon = if icon.is_empty() {
                "sparkles".to_string()
            } else {
                utf16_prefix(icon, 64).to_string()
            };
            let color = normalize_tag_color(&raw.color, catalog.order.len());
            let name = collapse_js_whitespace(&raw.name);
            let name = if name.is_empty() {
                "Tag".to_string()
            } else {
                name
            };
            let name = utf16_prefix(&name, MAX_CUSTOM_TAG_NAME_UTF16).to_string();
            catalog
                .tags
                .insert(tag_id.clone(), CustomTag { name, icon, color });
            catalog.order.push(tag_id);
        }
        catalog
    }

    pub(crate) fn find(&self, tag_id: &str) -> Option<&CustomTag> {
        if !is_custom_tag_id(tag_id) {
            return None;
        }
        self.tags.get(tag_id)
    }
}

fn normalize_tag_color(value: &str, fallback_index: usize) -> String {
    let color = js_trim(value).to_lowercase();
    let is_hex = color.len() == 7
        && color.starts_with('#')
        && color[1..]
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if is_hex {
        color
    } else {
        TAG_COLOR_PRESETS[fallback_index % TAG_COLOR_PRESETS.len()].to_string()
    }
}

/// `^custom-[a-z0-9]{4,40}$`.
pub(crate) fn is_custom_tag_id(value: &str) -> bool {
    value.strip_prefix("custom-").is_some_and(|token| {
        (4..=40).contains(&token.len())
            && token
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    })
}

fn is_builtin_tag(value: &str) -> bool {
    BUILTIN_TAGS.contains(&value)
}

fn is_session_tag(value: &str) -> bool {
    is_builtin_tag(value) || is_custom_tag_id(value)
}

/// `getEffectiveSidebarSessionTag`: the stored tag, else `favorite` for a legacy favorite. The
/// stored value is taken as the daemon sent it, without validation.
pub(crate) fn effective_tag(session_tag: Option<&str>, is_favorite: bool) -> Option<String> {
    match session_tag {
        Some(tag) => Some(tag.to_string()),
        None if is_favorite => Some("favorite".to_string()),
        None => None,
    }
}

/// `getSidebarSessionTagLabel`.
pub(crate) fn tag_label(tag: Option<&str>, catalog: &TagCatalog) -> Option<String> {
    let tag = tag?;
    if tag == UNTAGGED_TAG_FILTER {
        return Some("No tag".to_string());
    }
    if is_custom_tag_id(tag) {
        return Some(
            catalog
                .find(tag)
                .map_or_else(|| "Custom tag".to_string(), |custom| custom.name.clone()),
        );
    }
    TAG_OPTIONS
        .iter()
        .find(|(_, value, _)| *value == tag)
        .map(|(_, _, label)| (*label).to_string())
}

/// `nativeTagPresentation(tag ?? '')`: a custom tag's own glyph and color, else the built-in one.
pub(crate) fn tag_presentation(tag: Option<&str>, catalog: &TagCatalog) -> Option<TagPresentation> {
    let tag = tag.unwrap_or("");
    if let Some(custom) = catalog.find(tag) {
        return Some(TagPresentation {
            icon: custom.icon.clone(),
            icon_color: custom.color.clone(),
        });
    }
    builtin_presentation(tag).map(|(icon, color)| TagPresentation {
        icon: icon.to_string(),
        icon_color: color.to_string(),
    })
}

/// `sessionMatchesSidebarTagFilters`.
pub(crate) fn matches_tag_filters(effective_tag: Option<&str>, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    match effective_tag {
        None => filters.iter().any(|filter| filter == UNTAGGED_TAG_FILTER),
        Some(tag) => filters.iter().any(|filter| filter == tag),
    }
}

/// One row of the tag filter list in settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagListItem {
    pub id: String,
    pub kind: TagListItemKind,
    pub enabled: bool,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TagListItemKind {
    Tag,
    Separator,
    Untagged,
}

impl TagListItem {
    /// `getSidebarSessionTagListItemFilter`.
    fn filter(&self) -> Option<&str> {
        match self.kind {
            TagListItemKind::Tag => Some(&self.id),
            TagListItemKind::Untagged => Some(UNTAGGED_TAG_FILTER),
            TagListItemKind::Separator => None,
        }
    }
}

fn default_tag_list_items() -> Vec<TagListItem> {
    let tag = |id: &str| {
        let off = DEFAULT_OFF_TAGS.contains(&id);
        TagListItem {
            id: id.to_string(),
            kind: TagListItemKind::Tag,
            enabled: !off,
            visible: !off,
        }
    };
    let separator = |id: &str| TagListItem {
        id: id.to_string(),
        kind: TagListItemKind::Separator,
        enabled: true,
        visible: true,
    };
    let mut items = Vec::new();
    for section in 0..3u8 {
        items.extend(
            TAG_OPTIONS
                .iter()
                .filter(|(option_section, _, _)| *option_section == section)
                .map(|(_, value, _)| tag(value)),
        );
        items.push(separator(SEPARATOR_IDS[usize::from(section)]));
    }
    items.push(TagListItem {
        id: UNTAGGED_TAG_FILTER.to_string(),
        kind: TagListItemKind::Untagged,
        enabled: true,
        visible: true,
    });
    items
}

/// `normalizeSidebarSessionTagListItems`: known rows in the stored order, the defaults appended
/// for anything missing, and (with a catalog) custom tags the daemon no longer knows dropped and
/// new ones inserted before the No tag separator.
pub(crate) fn normalize_tag_list_items(
    candidate: &Value,
    catalog: Option<&TagCatalog>,
) -> Vec<TagListItem> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut items: Vec<TagListItem> = Vec::new();
    for raw in candidate.as_array().into_iter().flatten() {
        let Some(item) = normalize_tag_list_item(raw) else {
            continue;
        };
        if seen.contains(&item.id) {
            continue;
        }
        if let Some(catalog) = catalog {
            if item.kind == TagListItemKind::Tag
                && is_custom_tag_id(&item.id)
                && !catalog.tags.contains_key(&item.id)
            {
                continue;
            }
        }
        seen.insert(item.id.clone());
        items.push(item);
    }
    for item in default_tag_list_items() {
        if !seen.contains(&item.id) {
            items.push(item);
        }
    }
    if let Some(catalog) = catalog {
        let missing: Vec<&String> = catalog
            .order
            .iter()
            .filter(|tag_id| !seen.contains(*tag_id))
            .collect();
        if !missing.is_empty() {
            let insert_at = items
                .iter()
                .position(|item| item.id == "separator-type-untagged")
                .or_else(|| {
                    items
                        .iter()
                        .position(|item| item.kind == TagListItemKind::Untagged)
                })
                .unwrap_or(items.len());
            let inserted = missing.into_iter().map(|tag_id| TagListItem {
                id: tag_id.clone(),
                kind: TagListItemKind::Tag,
                enabled: true,
                visible: true,
            });
            items.splice(insert_at..insert_at, inserted);
        }
    }
    items
}

fn normalize_tag_list_item(candidate: &Value) -> Option<TagListItem> {
    let object = candidate.as_object()?;
    let bool_field = |key: &str| object.get(key).and_then(Value::as_bool).unwrap_or(true);
    let id = object.get("id").and_then(Value::as_str).unwrap_or("");
    let tag = object
        .get("tag")
        .and_then(Value::as_str)
        .filter(|tag| is_session_tag(tag))
        .or_else(|| is_session_tag(id).then_some(id));
    if let Some(tag) = tag {
        return Some(TagListItem {
            id: tag.to_string(),
            kind: TagListItemKind::Tag,
            enabled: bool_field("enabled"),
            visible: bool_field("visible"),
        });
    }
    if id == UNTAGGED_TAG_FILTER || object.get("type").and_then(Value::as_str) == Some("untagged") {
        return Some(TagListItem {
            id: UNTAGGED_TAG_FILTER.to_string(),
            kind: TagListItemKind::Untagged,
            enabled: bool_field("enabled"),
            visible: bool_field("visible"),
        });
    }
    SEPARATOR_IDS.contains(&id).then(|| TagListItem {
        id: id.to_string(),
        kind: TagListItemKind::Separator,
        enabled: bool_field("enabled"),
        visible: bool_field("visible"),
    })
}

/// One heading of the Tag As menu and the tags under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TagSection {
    pub(crate) label: &'static str,
    /// `(tag id, label)` in menu order.
    pub(crate) options: Vec<(String, String)>,
}

const SECTION_LABELS: [&str; 3] = ["Priority", "Progress", "Type"];

/// `getEnabledVisibleSidebarSessionTagSections`: the built-in sections filtered to the tags the
/// user left enabled and visible, then a Custom section, with empty sections dropped.
/// `include_tags` puts a tag the user has already set back on the menu even when they hid it, so
/// it can be taken off again.
pub(crate) fn enabled_visible_tag_sections(
    settings_items: &Value,
    catalog: &TagCatalog,
    include_tags: &[&str],
) -> Vec<TagSection> {
    let mut visible: Vec<String> = Vec::new();
    let mut visible_custom: Vec<String> = Vec::new();
    for item in normalize_tag_list_items(settings_items, Some(catalog)) {
        if item.kind == TagListItemKind::Tag && item.enabled && item.visible {
            if !visible.iter().any(|seen| *seen == item.id) {
                visible.push(item.id.clone());
                if is_custom_tag_id(&item.id) {
                    visible_custom.push(item.id);
                }
            }
        }
    }
    for tag in include_tags {
        if visible.iter().any(|seen| seen == *tag) {
            continue;
        }
        visible.push((*tag).to_string());
        if is_custom_tag_id(tag) {
            visible_custom.push((*tag).to_string());
        }
    }
    let mut sections: Vec<TagSection> = SECTION_LABELS
        .iter()
        .enumerate()
        .map(|(index, label)| TagSection {
            label,
            options: TAG_OPTIONS
                .iter()
                .filter(|(section, value, _)| {
                    usize::from(*section) == index && visible.iter().any(|tag| tag == value)
                })
                .map(|(_, value, label)| ((*value).to_string(), (*label).to_string()))
                .collect(),
        })
        .collect();
    sections.push(TagSection {
        label: "Custom",
        options: visible_custom
            .into_iter()
            .filter_map(|tag_id| {
                catalog
                    .tags
                    .get(&tag_id)
                    .map(|tag| (tag_id.clone(), tag.name.clone()))
            })
            .collect(),
    });
    sections.retain(|section| !section.options.is_empty());
    sections
}

/// `getSidebarSessionTagListItemLabel`.
pub(crate) fn tag_list_item_label(item: &TagListItem, catalog: &TagCatalog) -> String {
    match item.kind {
        TagListItemKind::Tag => {
            tag_label(Some(&item.id), catalog).unwrap_or_else(|| item.id.clone())
        }
        TagListItemKind::Untagged => "No tag".to_string(),
        TagListItemKind::Separator => "Separator".to_string(),
    }
}

/// `getSidebarSessionTagListItemFilter`, for a menu builder outside this module.
pub(crate) fn tag_list_item_filter(item: &TagListItem) -> Option<&str> {
    item.filter()
}

/// `getEnabledVisibleSidebarSessionTagFilters(normalizeSidebarSessionTagListItems(items,
/// catalog))`: the filters the Sort & Filter menu offers, which is what a selected filter is pruned
/// to on every projection.
pub(crate) fn enabled_visible_tag_filters(
    settings_items: &Value,
    catalog: &TagCatalog,
) -> Vec<String> {
    // `getEnabledVisibleSidebarSessionTagFilters` normalizes a second time without the catalog.
    // Over an already normalized list that is the identity: every id is known, none repeats, and
    // the defaults were appended by the first pass.
    normalize_tag_list_items(settings_items, Some(catalog))
        .into_iter()
        .filter(|item| item.enabled && item.visible)
        .filter_map(|item| item.filter().map(str::to_string))
        .collect()
}
