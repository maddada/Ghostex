//! The merged model pill's picker: tabs, rows, search ranking, favorites and the footer buttons.
//!
//! Port of `packages/shared/session-chat-presentation/model-menu.ts`.
//!
//! CDXC:SessionChat 2026-09-21 DECISION:
//! User: the composer's model and effort pills become one pill that opens one picker: agent tabs
//! with a favorites tab first, a model search, rows with a Cmd+number badge and a star, and a
//! footer for reasoning, context window and fast mode. Every renderer (the GPUI pop-up and the
//! phone's sheet) draws what this module decides, so tabs, row order, search ranking, favorites
//! and the footer can never differ between them.
//! SEE-ALSO: apps/desktop/src/app/native_chat/option_menu/model_menu/.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::menus::picker::agents::picker_agent;
use crate::menus::picker::model_picker::ModelPickerProvider;

/// The tab order, and the only providers a model menu can show.
pub const MODEL_MENU_PROVIDERS: [ModelPickerProvider; 6] = [
    ModelPickerProvider::Claude,
    ModelPickerProvider::Codex,
    ModelPickerProvider::Cursor,
    ModelPickerProvider::Grok,
    ModelPickerProvider::Antigravity,
    ModelPickerProvider::OpenCode,
];
pub const MODEL_MENU_FAVORITES_TAB: &str = "favorites";
/// `AUTO_MODEL_VALUE`: the row the agent picks for you, pinned first on its own tab.
const AUTO_MODEL_VALUE: &str = "auto";
pub const MODEL_MENU_SEARCH_PLACEHOLDER: &str = "Search models…";
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
    /// 1 to 9 for the rows Cmd+number reaches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<usize>,
    /// Favorites mix agents, so those rows name theirs on a second line.
    pub show_agent: bool,
    /// The reasoning levels this row's model offers, which Left and Right step through; empty for
    /// a model without levels. Filled by [`crate::menus::picker::projection::model_menu_projection`].
    pub efforts: Vec<ModelMenuEffort>,
    /// The level a keyboard pick of this row starts on; empty when `efforts` is.
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
    let agent_name = picker_agent(provider).name.to_string();
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

/// `matchRank`: 0 for a prefix, 1 for a substring, as the label; the description ranks two below.
fn match_rank(query: &str, text: &str) -> Option<usize> {
    let at = text.to_lowercase().find(query)?;
    Some(if at == 0 { 0 } else { 1 })
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
) -> Vec<ModelMenuRow> {
    let favorites_tab = tab == ModelMenuTabId::Favorites;
    let is_favorite = |entry: &ModelMenuEntry| {
        favorites.contains(&model_menu_favorite_key(
            entry.provider.as_str(),
            &entry.value,
        ))
    };
    let pool: Vec<&ModelMenuEntry> = if favorites_tab {
        MODEL_MENU_PROVIDERS
            .into_iter()
            .flat_map(|provider| entries_of(entries, provider.as_str()).iter())
            .filter(|entry| is_favorite(entry))
            .collect()
    } else {
        entries_of(entries, tab.as_str()).iter().collect()
    };
    let query = query.trim().to_lowercase();
    let mut ranked: Vec<(&ModelMenuEntry, bool, usize, usize)> = Vec::new();
    for (index, entry) in pool.iter().enumerate() {
        let favorite = is_favorite(entry);
        if query.is_empty() {
            ranked.push((entry, favorite, index, 0));
            continue;
        }
        let by_label = match_rank(&query, &entry.label);
        let by_description = match_rank(
            &query,
            &format!(
                "{} {}",
                entry.description.clone().unwrap_or_default(),
                entry.label
            ),
        );
        let rank = match (by_label, by_description) {
            (None, None) => continue,
            (Some(label), None) => label,
            (None, Some(description)) => description + 2,
            (Some(label), Some(description)) => label.min(description + 2),
        };
        ranked.push((entry, favorite, index, rank));
    }
    // `pinned`: the user's 2026-09-22 decision in `model-menu.ts` ("make Auto show up at the top
    // here"): on an agent's own tab Auto sorts above a starred model; a search still ranks it.
    let pinned = |entry: &ModelMenuEntry| !favorites_tab && entry.value == AUTO_MODEL_VALUE;
    // `sort` by rank, then Auto, then favorites first, then catalog order; stable, as
    // JavaScript's is.
    ranked.sort_by(|left, right| {
        left.3
            .cmp(&right.3)
            .then_with(|| pinned(right.0).cmp(&pinned(left.0)))
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let selected_entry = model_menu_entry_for(
        entries,
        current.provider.as_deref(),
        current.model.as_deref(),
    );
    ranked
        .into_iter()
        .enumerate()
        .map(|(index, (entry, favorite, _, _))| ModelMenuRow {
            key: model_menu_favorite_key(entry.provider.as_str(), &entry.value),
            favorite,
            selected: selected_entry.is_some_and(|current| {
                current.provider == entry.provider && current.value == entry.value
            }),
            shortcut: (index < MODEL_MENU_SHORTCUT_ROWS).then_some(index + 1),
            show_agent: favorites_tab,
            efforts: Vec::new(),
            effort: String::new(),
            entry: (*entry).clone(),
        })
        .collect()
}

/// `modelMenuEmptyText`.
pub fn model_menu_empty_text(tab: ModelMenuTabId, query: &str) -> String {
    if !query.trim().is_empty() {
        return "No models found".to_string();
    }
    if tab == ModelMenuTabId::Favorites {
        "No starred models yet. Star a model from an agent's tab.".to_string()
    } else {
        "No models".to_string()
    }
}

/// `modelMenuPickValue`: the value a row is picked with, which is the context window in use when
/// the row offers it, else the catalog's default twin.
pub fn model_menu_pick_value(
    entry: &ModelMenuEntry,
    current_model: Option<&str>,
    default_value: Option<&str>,
) -> String {
    if entry.variants.is_empty() {
        return entry.value.clone();
    }
    if let Some(current) = current_model {
        if entry
            .variants
            .iter()
            .any(|variant| variant.value == current)
        {
            return current.to_string();
        }
    }
    let long = entry
        .variants
        .iter()
        .find(|variant| variant.value.ends_with(LONG_CONTEXT_SUFFIX));
    if let (Some(long), Some(current)) = (long, current_model) {
        if current.ends_with(LONG_CONTEXT_SUFFIX) {
            return long.value.clone();
        }
    }
    entry
        .variants
        .iter()
        .find(|variant| Some(variant.value.as_str()) == default_value)
        .map(|variant| variant.value.clone())
        .unwrap_or_else(|| entry.value.clone())
}
