//! The merged model pill's picker: tabs, rows, search ranking, favorites and the footer buttons.
//!
//! Port of `packages/shared/session-chat-presentation/model-menu.ts`.
//!
//! CDXC:SessionChat 2026-09-21 DECISION:
//! User: the composer's model and effort pills become one pill that opens one picker: agent tabs
//! with a favorites tab first, a model search, rows with an Option+number badge and a star, and a
//! footer for reasoning, context window and fast mode. Every renderer (the GPUI pop-up and the
//! phone's sheet) draws what this module decides, so tabs, row order, search ranking, favorites
//! and the footer can never differ between them.
//! SEE-ALSO: apps/desktop/src/app/native_chat/option_menu/model_menu/.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::menus::picker::agents::picker_agent;
use crate::menus::picker::model_picker::ModelPickerProvider;

/// The tab order, and the only providers a model menu can show.
pub const MODEL_MENU_PROVIDERS: [ModelPickerProvider; 11] = [
    ModelPickerProvider::Claude,
    ModelPickerProvider::Codex,
    ModelPickerProvider::Cursor,
    ModelPickerProvider::Grok,
    ModelPickerProvider::Antigravity,
    ModelPickerProvider::OpenCode,
    ModelPickerProvider::Hermes,
    ModelPickerProvider::Pi,
    ModelPickerProvider::Omp,
    ModelPickerProvider::Empryo,
    ModelPickerProvider::Dsh,
];
pub const MODEL_MENU_FAVORITES_TAB: &str = "favorites";
/// `AUTO_MODEL_VALUE`: the row the agent picks for you, pinned first on its own tab.
const AUTO_MODEL_VALUE: &str = "auto";
pub const MODEL_MENU_SEARCH_PLACEHOLDER: &str = "Search models…";
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User: "for opencode and others that have lots of models please add a filter bar at the top of the model picker so i can search for a model ... let's say if there's more than 6 models then enable these 2 things". A tab listing more than this many models gets the search field (see [`model_menu_search_enabled`]); a shorter one keeps the plain list. This supersedes the 2026-09-24/25 removal of the picker's search ("just remove the search it's useless") for long lists only.
pub const MODEL_MENU_SEARCH_THRESHOLD: usize = 6;
pub const MODEL_MENU_SHORTCUT_ROWS: usize = 9;

/// The long-context twin of a model is the same row with another Context Window choice.
const LONG_CONTEXT_SUFFIX: &str = "[1m]";
const CONTEXT_LABEL_STANDARD: &str = "200K";
const CONTEXT_LABEL_LONG: &str = "1M";

/// Which tab is open: the favorites tab, or one agent's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelMenuTabId {
    Favorites,
    Provider(ModelPickerProvider),
}

impl ModelMenuTabId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Favorites => MODEL_MENU_FAVORITES_TAB,
            Self::Provider(provider) => provider.as_str(),
        }
    }

    /// The tab with this id. An unknown id is the favorites tab, which is what the TypeScript's
    /// `entries[params.tab] ?? []` degrades to.
    pub fn from_wire(value: &str) -> Self {
        ModelPickerProvider::from_wire(value)
            .map(Self::Provider)
            .unwrap_or(Self::Favorites)
    }
}

/// What the picker remembers between frames: the open tab and the search text.
///
/// `tab` of `None` is "not opened yet", which is what makes [`model_menu_opening_tab`] pick the
/// session's own agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelMenuView {
    pub tab: Option<ModelMenuTabId>,
    pub query: String,
    /// Whether the open tab lists its Legacy models below the current ones.
    pub legacy_open: bool,
}

/// One model choice as family e1's session option catalog lists it.
///
/// This is the slice of `SessionChatOptionChoice` the model menu reads. Family e1 owns the
/// catalog that produces it (`crate::menus::option_catalog`, ported from
/// `packages/core-ui/chat/session-chat-session-options.ts`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The catalog put the model in its Legacy models group.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legacy: bool,
}

/// One agent's model lineup, as the menu needs it.
///
/// `sessionChatSessionOptionCatalog(provider)` answers this for each provider; e1 fills it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelMenuCatalog {
    /// `catalog.modelIcon`: the agent logo every row of this tab draws.
    pub model_icon: String,
    /// `catalog.model.choices ?? []`, in catalog order.
    pub choices: Vec<ModelChoice>,
    /// The tab's name when it is not the agent's own: a Hermes profile's bot, such as Harry.
    pub agent_name: Option<String>,
}

/// The catalogs, keyed by provider id, in the order [`MODEL_MENU_PROVIDERS`] lists them.
pub type ModelMenuCatalogs = BTreeMap<String, ModelMenuCatalog>;

/// `ModelMenuVariant`: the model value one context window is picked with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuVariant {
    pub value: String,
    pub label: String,
}

/// One model the menu can list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuEntry {
    pub provider: ModelPickerProvider,
    pub icon: String,
    pub agent_name: String,
    /// Row identity and favorites key; the standard-context value where the model has two.
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub variants: Vec<ModelMenuVariant>,
    /// Listed under the tab's Legacy models, below the current models.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legacy: bool,
}

/// One row of the open tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuRow {
    #[serde(flatten)]
    pub entry: ModelMenuEntry,
    pub key: String,
    pub favorite: bool,
    pub selected: bool,
    /// 1 to 9 for the rows Option+number reaches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<usize>,
    /// Favorites mix agents, so those rows name theirs on a second line.
    pub show_agent: bool,
    /// The reasoning levels this row's model offers, which Left and Right step through; empty for
    /// a model without levels. Filled by [`crate::menus::picker::projection::model_menu_projection`].
    pub efforts: Vec<ModelMenuEffort>,
    /// The level a keyboard pick of this row starts on; empty when `efforts` is, and on the
    /// session's own row when its catalog's `agent_keeps_effort` permits omission and the reported
    /// level is missing or unoffered (no level is marked as current). Other rows use the session
    /// level, else the model's default, else the middle of its list.
    pub effort: String,
}

/// `ModelMenuEffort`: one reasoning level a row offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuEffort {
    pub value: String,
    pub label: String,
}

/// One tab of the picker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuTab {
    pub id: String,
    /// Agent logo file name; absent on the favorites tab, which draws a star.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub name: String,
    pub active: bool,
    /// Another agent's tab in a started session: its models hand the conversation off to that
    /// agent's CLI. Set by [`crate::menus::picker::projection::model_menu_projection`].
    #[serde(default)]
    pub handoff: bool,
}

/// `modelMenuFavoriteKey`.
pub fn model_menu_favorite_key(provider: &str, value: &str) -> String {
    format!("{provider}:{value}")
}

/// `entriesFor`: one agent's rows, with the long-context twin folded into its standard row.
fn entries_for(provider: ModelPickerProvider, catalog: &ModelMenuCatalog) -> Vec<ModelMenuEntry> {
    let agent_name = catalog
        .agent_name
        .clone()
        .unwrap_or_else(|| picker_agent(provider).name.to_string());
    let values: Vec<&str> = catalog
        .choices
        .iter()
        .map(|choice| choice.value.as_str())
        .collect();
    let mut entries = Vec::new();
    for choice in &catalog.choices {
        let long = choice.value.ends_with(LONG_CONTEXT_SUFFIX);
        let base = if long {
            &choice.value[..choice.value.len() - LONG_CONTEXT_SUFFIX.len()]
        } else {
            choice.value.as_str()
        };
        if long && values.contains(&base) {
            continue;
        }
        let twin = format!("{}{LONG_CONTEXT_SUFFIX}", choice.value);
        let variants = if !long && values.contains(&twin.as_str()) {
            vec![
                ModelMenuVariant {
                    value: choice.value.clone(),
                    label: CONTEXT_LABEL_STANDARD.to_string(),
                },
                ModelMenuVariant {
                    value: twin,
                    label: CONTEXT_LABEL_LONG.to_string(),
                },
            ]
        } else {
            Vec::new()
        };
        entries.push(ModelMenuEntry {
            provider,
            icon: catalog.model_icon.clone(),
            agent_name: agent_name.clone(),
            value: choice.value.clone(),
            label: choice.label.clone(),
            description: choice.description.clone(),
            variants,
            legacy: choice.legacy,
        });
    }
    entries
}

/// Every agent the catalog knows, in tab order.
pub type ModelMenuEntries = BTreeMap<String, Vec<ModelMenuEntry>>;

/// `modelMenuEntries()`.
pub fn model_menu_entries(catalogs: &ModelMenuCatalogs) -> ModelMenuEntries {
    let mut entries = ModelMenuEntries::new();
    for provider in MODEL_MENU_PROVIDERS {
        if let Some(catalog) = catalogs.get(provider.as_str()) {
            entries.insert(
                provider.as_str().to_string(),
                entries_for(provider, catalog),
            );
        }
    }
    entries
}

fn entries_of<'a>(entries: &'a ModelMenuEntries, provider: &str) -> &'a [ModelMenuEntry] {
    entries
        .get(provider)
        .map(Vec::as_slice)
        .unwrap_or(&[] as &[ModelMenuEntry])
}

/// `modelMenuEntryFor`.
pub fn model_menu_entry_for<'a>(
    entries: &'a ModelMenuEntries,
    provider: Option<&str>,
    model: Option<&str>,
) -> Option<&'a ModelMenuEntry> {
    let (provider, model) = (provider?, model?);
    entries_of(entries, provider).iter().find(|entry| {
        entry.value == model || entry.variants.iter().any(|variant| variant.value == model)
    })
}

/// `modelMenuTabs`.
pub fn model_menu_tabs(entries: &ModelMenuEntries, tab: ModelMenuTabId) -> Vec<ModelMenuTab> {
    let mut tabs = vec![ModelMenuTab {
        id: MODEL_MENU_FAVORITES_TAB.to_string(),
        icon: None,
        name: "Favorites".to_string(),
        active: tab == ModelMenuTabId::Favorites,
        handoff: false,
    }];
    for provider in MODEL_MENU_PROVIDERS {
        let rows = entries_of(entries, provider.as_str());
        let Some(first) = rows.first() else {
            continue;
        };
        tabs.push(ModelMenuTab {
            id: provider.as_str().to_string(),
            icon: Some(first.icon.clone()),
            name: first.agent_name.clone(),
            active: tab == ModelMenuTabId::Provider(provider),
            handoff: false,
        });
    }
    tabs
}

/// `modelMenuOpeningTab`: the session's own agent, where a pick applies in place; favorites
/// already float to the top of every tab.
pub fn model_menu_opening_tab(
    entries: &ModelMenuEntries,
    provider: Option<ModelPickerProvider>,
) -> ModelMenuTabId {
    if let Some(provider) = provider {
        if !entries_of(entries, provider.as_str()).is_empty() {
            return ModelMenuTabId::Provider(provider);
        }
    }
    MODEL_MENU_PROVIDERS
        .into_iter()
        .find(|provider| !entries_of(entries, provider.as_str()).is_empty())
        .map(ModelMenuTabId::Provider)
        .unwrap_or(ModelMenuTabId::Favorites)
}

/// Whether the open tab lists enough models to get the search field: the agent's whole lineup
/// (legacy models included), or every starred model on the favorites tab.
pub fn model_menu_search_enabled(
    entries: &ModelMenuEntries,
    tab: ModelMenuTabId,
    favorites: &[String],
) -> bool {
    let count = match tab {
        ModelMenuTabId::Favorites => MODEL_MENU_PROVIDERS
            .into_iter()
            .flat_map(|provider| entries_of(entries, provider.as_str()).iter())
            .filter(|entry| is_favorite_entry(entry, favorites))
            .count(),
        ModelMenuTabId::Provider(provider) => entries_of(entries, provider.as_str()).len(),
    };
    count > MODEL_MENU_SEARCH_THRESHOLD
}

/// A row's star is saved under its own value, or under its 1M twin's, which is where a model
/// starred while it came in 1M only (Opus 5.5 before 2026-09-27) keeps it.
fn is_favorite_entry(entry: &ModelMenuEntry, favorites: &[String]) -> bool {
    std::iter::once(&entry.value)
        .chain(entry.variants.iter().map(|variant| &variant.value))
        .any(|value| favorites.contains(&model_menu_favorite_key(entry.provider.as_str(), value)))
}

/// `matchRank`: every word of the query must appear in `text`, case-insensitively, so "gpt 6"
/// finds "GPT-6 Luna"; 0 when `text` starts with the first word, else 1.
fn match_rank(words: &[&str], text: &str) -> Option<usize> {
    let text = text.to_lowercase();
    if words.is_empty() || !words.iter().all(|word| text.contains(word)) {
        return None;
    }
    Some(if text.starts_with(words[0]) { 0 } else { 1 })
}

/// What the current session is running, for the `selected` flag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelMenuCurrent {
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// `modelMenuRows`.
pub fn model_menu_rows(
    entries: &ModelMenuEntries,
    tab: ModelMenuTabId,
    query: &str,
    favorites: &[String],
    current: &ModelMenuCurrent,
    legacy_open: bool,
) -> Vec<ModelMenuRow> {
    let favorites_tab = tab == ModelMenuTabId::Favorites;
    let is_favorite = |entry: &ModelMenuEntry| is_favorite_entry(entry, favorites);
    let pool: Vec<&ModelMenuEntry> = if favorites_tab {
        MODEL_MENU_PROVIDERS
            .into_iter()
            .flat_map(|provider| entries_of(entries, provider.as_str()).iter())
            .filter(|entry| is_favorite(entry))
            .collect()
    } else {
        entries_of(entries, tab.as_str()).iter().collect()
    };
    let selected_entry = model_menu_entry_for(
        entries,
        current.provider.as_deref(),
        current.model.as_deref(),
    );
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    // The last field marks a row drawn inside the Legacy models list.
    let mut ranked: Vec<(&ModelMenuEntry, bool, usize, usize, bool)> = Vec::new();
    for (index, entry) in pool.iter().enumerate() {
        let favorite = is_favorite(entry);
        if query.is_empty() {
            // An agent's tab lists its legacy models after the current ones, and only once its
            // Legacy models list is open. A starred one also stays with the favorites at the top,
            // and the session's own model also stays in view, and each is still listed in the
            // Legacy models list (star lit), so the list always holds every legacy model and a
            // star can be taken off in either place.
            if !favorites_tab && entry.legacy {
                if legacy_open {
                    ranked.push((entry, favorite, index, 1, true));
                }
                if favorite || selected_entry.is_some_and(|selected| selected == *entry) {
                    ranked.push((entry, favorite, index, 0, false));
                }
                continue;
            }
            ranked.push((entry, favorite, index, 0, false));
            continue;
        }
        // The label ranks first, then the model id ("gpt-6-luna"), then words split between them.
        let rank = match_rank(&words, &entry.label)
            .or_else(|| match_rank(&words, &entry.value).map(|rank| rank + 2))
            .or_else(|| match_rank(&words, &format!("{} {}", entry.label, entry.value)).map(|_| 4));
        let Some(rank) = rank else {
            continue;
        };
        ranked.push((entry, favorite, index, rank, false));
    }
    // CDXC:SessionChat 2026-11-02 DECISION:
    // User: "we need the ability to add models to favorite to show them at the top of the list (shown in order alphabetical order at the top of the models list)". Starred rows come first, sorted by label case-insensitively (the favorites tab too), then the rest in their usual order (search rank, Auto, catalog order); a search keeps matching stars first. This supersedes Auto sorting above the stars (2026-09-22) and stars keeping catalog order.
    let pinned = |entry: &ModelMenuEntry| !favorites_tab && entry.value == AUTO_MODEL_VALUE;
    ranked.sort_by(|left, right| {
        // The Legacy models list follows every other row, in catalog order.
        if left.4 || right.4 {
            return left.4.cmp(&right.4).then_with(|| left.2.cmp(&right.2));
        }
        right
            .1
            .cmp(&left.1)
            .then_with(|| {
                if left.1 {
                    left.0
                        .label
                        .to_lowercase()
                        .cmp(&right.0.label.to_lowercase())
                } else {
                    left.3
                        .cmp(&right.3)
                        .then_with(|| pinned(right.0).cmp(&pinned(left.0)))
                }
            })
            .then_with(|| left.2.cmp(&right.2))
    });
    ranked
        .into_iter()
        .enumerate()
        .map(|(index, (entry, favorite, _, _, folded))| ModelMenuRow {
            key: model_menu_favorite_key(entry.provider.as_str(), &entry.value),
            favorite,
            selected: selected_entry.is_some_and(|current| {
                current.provider == entry.provider && current.value == entry.value
            }),
            shortcut: (index < MODEL_MENU_SHORTCUT_ROWS).then_some(index + 1),
            show_agent: favorites_tab,
            efforts: Vec::new(),
            effort: String::new(),
            entry: ModelMenuEntry {
                // Only the rows inside the Legacy models list say so, so a client can find where
                // that list starts (the phone puts its toggle before the first of them).
                legacy: folded,
                ..(*entry).clone()
            },
        })
        .collect()
}

/// How many legacy models the open tab folds into its Legacy models list: none on the favorites
/// tab or during a search, which list every match. A starred or in-use legacy model counts too:
/// it is listed there as well as where it stands out.
pub fn model_menu_legacy_count(
    entries: &ModelMenuEntries,
    tab: ModelMenuTabId,
    query: &str,
) -> usize {
    if tab == ModelMenuTabId::Favorites || !query.trim().is_empty() {
        return 0;
    }
    entries_of(entries, tab.as_str())
        .iter()
        .filter(|entry| entry.legacy)
        .count()
}

/// `modelMenuEmptyText`.
pub fn model_menu_empty_text(tab: ModelMenuTabId, query: &str) -> String {
    if !query.trim().is_empty() {
        return "No models match".to_string();
    }
    if tab == ModelMenuTabId::Favorites {
        "No starred models yet. Star a model from an agent's tab.".to_string()
    } else {
        "No models".to_string()
    }
}

/// Whether a model value is the long-context twin of another (`opus[1m]`).
pub(crate) fn is_long_context_value(value: &str) -> bool {
    value.ends_with(LONG_CONTEXT_SUFFIX)
}

/// The long-context twin of a value (`opus` is `opus[1m]`), whether or not the catalog has it.
pub(crate) fn long_context_value(value: &str) -> String {
    format!("{value}{LONG_CONTEXT_SUFFIX}")
}

/// `modelMenuPickValue`: the value a row is picked with.
///
/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: "when user switches to opus we need to always default to 1M since it's just plain
/// better". Moving to a model that comes in 200K and 1M from any other model, or from another
/// agent, starts it on 1M; picking it again while the session already runs it at 200K keeps
/// 200K, so a deliberate 200K choice is only undone through the Context Window button. This
/// supersedes carrying the previous model's window over and falling back to the catalog default.
pub fn model_menu_pick_value(entry: &ModelMenuEntry, current_model: Option<&str>) -> String {
    if let Some(current) = current_model {
        if entry
            .variants
            .iter()
            .any(|variant| variant.value == current)
        {
            return current.to_string();
        }
    }
    entry
        .variants
        .iter()
        .find(|variant| is_long_context_value(&variant.value))
        .map_or_else(|| entry.value.clone(), |variant| variant.value.clone())
}
