//! Everything both renderers draw for the model picker, from one context and one view state.
//!
//! Port of `packages/shared/session-chat-controller/model-menu.ts`. `ModelMenuContext` is family
//! e1's `computeNativeChatOptions` output; what it can be built from here is
//! [`ModelMenuContext`], which carries the same fields.

use serde_json::{json, Value};

use crate::menus::catalog::AgentModelCatalog;
use crate::menus::option_catalog::{session_option_catalog, SessionOptionCatalog};
use crate::menus::picker::model_menu::{
    model_menu_empty_text, model_menu_entries, model_menu_legacy_count, model_menu_opening_tab,
    model_menu_pick_value, model_menu_rows, model_menu_search_enabled, model_menu_tabs,
    ModelMenuCatalogs, ModelMenuCurrent, ModelMenuEffort, ModelMenuEntries, ModelMenuRow,
    ModelMenuTabId, ModelMenuView, MODEL_MENU_FAVORITES_TAB, MODEL_MENU_SEARCH_PLACEHOLDER,
};
use crate::menus::picker::model_picker::{
    model_picker_scope_reason, model_picker_supports_session_scope, ModelPickerProvider,
    MODEL_PICKER_DEFAULT_SCOPE_ONLY_REASON,
};
use crate::menus::picker::traits::{
    model_menu_pill_labels, model_menu_traits, OptionRows, ResolvedOptionDescriptor,
};

/// The model menu's inputs, published as `modelMenuContext` and consumed by every pick.
///
/// Built by family e1's `computeNativeChatOptions`; only its fields are read here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelMenuContext {
    pub provider: Option<ModelPickerProvider>,
    /// The catalog's model descriptor id, which is what a pick dispatches against.
    pub model_id: Option<String>,
    /// The pill's own label when no entry matches.
    pub model_label: Option<String>,
    /// The visible option descriptors, Shift+Tab mode cycler already removed, resolved against
    /// the session's option state.
    pub descriptors: Vec<ResolvedOptionDescriptor>,
    /// `state[modelId]?.value`: the model this session is running.
    pub model_value: Option<String>,
    /// `state.effort?.value`, which a hand-off to another agent starts from.
    pub effort_value: Option<String>,
    /// The reason the last selection was abandoned.
    pub selection_error: Option<String>,
    /// Picks are refused while an option command is being typed into a working agent.
    pub disabled: bool,
    /// `sessionOptions.sessionKey`: the picker checks it again when it finishes, because the
    /// session can change under an open picker.
    pub session_key: Option<String>,
    /// A draft has no conversation yet, so another agent's model switches the draft instead of
    /// handing off.
    pub draft: bool,
    /// An orchestrator's chat while Show all effort levels is off: every row offers only the
    /// orchestrator effort levels (`orchestrator_efforts`), plus the level the session is on.
    pub orchestrator: bool,
    /// The value `modelMenuContext` itself is published as.
    ///
    /// Its `descriptors` and `state` are family e1's own shapes, which e2 does not model, so e1
    /// hands the whole object over verbatim and e2 only reads the fields above out of it.
    pub raw: Value,
}

impl ModelMenuContext {
    /// The `modelMenuContext` document key.
    pub fn to_json(&self) -> Value {
        self.raw.clone()
    }
}

/// What a click on a model row means for this session.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelMenuPick {
    /// The row belongs to this session's own agent, so it is an ordinary option pick. `effort` is
    /// set when the pick carries a reasoning level (the keyboard's Left and Right); a plain click
    /// leaves it out.
    Select {
        value: String,
        effort: Option<String>,
    },
    /// Another agent's model: hand the conversation over, or switch a draft's agent.
    Handoff {
        provider: ModelPickerProvider,
        model: String,
        effort: String,
    },
}

/// `modelMenuPick`.
///
/// `effort_for` is `modelMenuEffortFor(provider, model, currentEffort)`: the effort a model from
/// another agent starts on, which family e1's catalog answers because it knows that agent's
/// options. It is only consulted on the hand-off path.
pub fn model_menu_pick(
    row: &ModelMenuRow,
    context: &ModelMenuContext,
    effort: Option<String>,
    effort_for: impl FnOnce(ModelPickerProvider, &str, Option<&str>) -> String,
) -> ModelMenuPick {
    if Some(row.entry.provider) == context.provider {
        return ModelMenuPick::Select {
            value: model_menu_pick_value(&row.entry, context.model_value.as_deref()),
            effort,
        };
    }
    let model = model_menu_pick_value(&row.entry, None);
    let effort = effort
        .unwrap_or_else(|| effort_for(row.entry.provider, &model, context.effort_value.as_deref()));
    ModelMenuPick::Handoff {
        provider: row.entry.provider,
        model,
        effort,
    }
}

/// `modelMenuRowEfforts`: the reasoning levels a row's model offers and the one its pick starts
/// on. For the session's own model the levels come from the session's live descriptor, so they
/// match what the Reasoning button lists today; any other row asks its agent's catalog about the
/// model the pick would run.
///
/// `provider_catalogs` holds each provider's session option catalog for the whole menu: building
/// one per row was most of the document's cost on every publish.
///
/// CDXC:SessionChat 2026-11-03 WHY:
/// Both renderers draw the session's own row's start as its current level, so falling back to the model's default or middle level would claim a level the agent has not reported (for example OMP's Auto is not an offered fixed level). When the row's catalog `agent_keeps_effort`, its own row starts on no level (`""`) while the reported level is missing or unoffered: no level is marked, a pick without choosing a level leaves it to the agent, and Left and Right on the desktop start from an end of the list. Agents whose model switch always sets a level keep the session-level, default, then middle fallback, since an unmarked row would hide the level its pick sets.
/// SEE-ALSO: `SessionOptionCatalog::agent_keeps_effort` in packages/gx-chat-core/src/menus/option_catalog.rs; `effort_for` and `reasoning_for` in apps/desktop/src/app/native_chat/option_menu/model_menu/state.rs and the phone's `modelMenuState.ts` read the row's level; server/src/session_chat_pi_models.rs `omp_transcript_level` reports OMP's level.
fn model_menu_row_efforts(
    row: &ModelMenuRow,
    context: &ModelMenuContext,
    catalog: &AgentModelCatalog,
    provider_catalogs: &mut std::collections::BTreeMap<String, Option<SessionOptionCatalog>>,
) -> (Vec<ModelMenuEffort>, String) {
    let session_effort = context
        .descriptors
        .iter()
        .find(|descriptor| descriptor.id == "effort");
    let (choices, default_value) = match session_effort {
        Some(descriptor) if row.selected => match &descriptor.rows {
            OptionRows::Choices { choices, .. } => (
                choices
                    .iter()
                    .map(|choice| (choice.value.clone(), choice.label.clone()))
                    .collect(),
                descriptor.default_value.clone(),
            ),
            _ => (Vec::new(), None),
        },
        _ => {
            let pick_value = match model_menu_pick(row, context, None, |_, _, _| String::new()) {
                ModelMenuPick::Select { value, .. } => value,
                ModelMenuPick::Handoff { model, .. } => model,
            };
            provider_catalogs
                .entry(row.entry.provider.as_str().to_string())
                .or_insert_with(|| {
                    session_option_catalog(catalog, Some(row.entry.provider.as_str()))
                })
                .as_ref()
                .and_then(|options| {
                    options
                        .options_for_model(&pick_value)
                        .into_iter()
                        .find(|descriptor| descriptor.id == "effort")
                })
                .map(|descriptor| {
                    (
                        descriptor
                            .choice_list()
                            .iter()
                            .map(|choice| (choice.value.clone(), choice.label.clone()))
                            .collect(),
                        descriptor.default_value.clone(),
                    )
                })
                .unwrap_or_default()
        }
    };
    let choices: Vec<(String, String)> = if context.orchestrator {
        let values: Vec<&str> = choices.iter().map(|(value, _)| value.as_str()).collect();
        let offered = crate::menus::orchestrator_effort::orchestrator_efforts(
            &values,
            false,
            context.effort_value.as_deref(),
        );
        choices
            .into_iter()
            .filter(|(value, _)| offered.contains(value))
            .collect()
    } else {
        choices
    };
    let efforts: Vec<ModelMenuEffort> = choices
        .into_iter()
        .map(|(value, label)| {
            let named = catalog.effort_label(&value);
            ModelMenuEffort {
                label: if named.is_empty() { label } else { named },
                value,
            }
        })
        .collect();
    if efforts.is_empty() {
        return (efforts, String::new());
    }
    let offered = |value: Option<&str>| {
        value
            .filter(|value| efforts.iter().any(|effort| effort.value == *value))
            .map(str::to_string)
    };
    let current = offered(context.effort_value.as_deref());
    if current.is_none()
        && row.selected
        && provider_catalogs
            .entry(row.entry.provider.as_str().to_string())
            .or_insert_with(|| session_option_catalog(catalog, Some(row.entry.provider.as_str())))
            .as_ref()
            .is_some_and(SessionOptionCatalog::agent_keeps_effort)
    {
        return (efforts, String::new());
    }
    let effort = current
        .or_else(|| offered(default_value.as_deref()))
        .unwrap_or_else(|| efforts[efforts.len() / 2].value.clone());
    (efforts, effort)
}

/// The search text the open tab filters by: none on a tab too short to show the search field, so
/// a query typed on a long tab never hides rows on a short one.
fn search_query<'a>(
    entries: &ModelMenuEntries,
    tab: ModelMenuTabId,
    view: &'a ModelMenuView,
    favorites: &[String],
) -> &'a str {
    if model_menu_search_enabled(entries, tab, favorites) {
        &view.query
    } else {
        ""
    }
}

/// `modelMenuProjection`: the whole `modelMenu` document key.
pub fn model_menu_projection(
    context: &ModelMenuContext,
    view: &ModelMenuView,
    catalogs: &ModelMenuCatalogs,
    favorites: &[String],
    catalog: &AgentModelCatalog,
) -> Value {
    let entries = model_menu_entries(catalogs);
    let tab = view
        .tab
        .unwrap_or_else(|| model_menu_opening_tab(&entries, context.provider));
    let search_enabled = model_menu_search_enabled(&entries, tab, favorites);
    let query = search_query(&entries, tab, view, favorites);
    let traits = model_menu_traits(
        &entries,
        &context.descriptors,
        context.provider.map(|provider| provider.as_str()),
        context.model_value.as_deref(),
        context
            .raw
            .pointer("/state/fastMode/value")
            .and_then(Value::as_str),
        catalog,
    );
    let current = ModelMenuCurrent {
        provider: context
            .provider
            .map(|provider| provider.as_str().to_string()),
        model: context.model_value.clone(),
    };
    let mut provider_catalogs = std::collections::BTreeMap::new();
    let rows: Vec<ModelMenuRow> =
        model_menu_rows(&entries, tab, query, favorites, &current, view.legacy_open)
            .into_iter()
            .map(|mut row| {
                (row.efforts, row.effort) =
                    model_menu_row_efforts(&row, context, catalog, &mut provider_catalogs);
                row
            })
            .collect();
    let session_scope = context
        .provider
        .is_some_and(model_picker_supports_session_scope);
    json!({
        "tab": tab.as_str(),
        "query": query,
        "searchEnabled": search_enabled,
        "placeholder": MODEL_MENU_SEARCH_PLACEHOLDER,
        "tabs": model_menu_tabs(&entries, tab)
            .into_iter()
            .map(|mut entry| {
                entry.handoff = entry.id != MODEL_MENU_FAVORITES_TAB
                    && context.provider.map(|provider| provider.as_str()) != Some(entry.id.as_str())
                    && !context.draft;
                entry
            })
            .collect::<Vec<_>>(),
        "rows": rows,
        // The tab's Legacy models list: a toggle below the current models, whose rows follow
        // them in `rows` while it is open.
        "legacy": match model_menu_legacy_count(&entries, tab, query, favorites, &current) {
            0 => Value::Null,
            count => json!({
                "label": crate::menus::catalog::LEGACY_GROUP_LABEL,
                "count": count,
                "open": view.legacy_open,
            }),
        },
        "emptyText": rows
            .is_empty()
            .then(|| model_menu_empty_text(tab, query)),
        "traits": traits.iter().map(|row| row.to_json()).collect::<Vec<_>>(),
        "pill": model_menu_pill_labels(
            &entries,
            context.provider.map(|provider| provider.as_str()),
            context.model_value.as_deref(),
            context.model_label.as_deref(),
            &traits,
        ),
        "error": context.selection_error,
        "disabled": context.disabled,
        // Right-click applies to this session only where the agent can; elsewhere the hint says
        // why not.
        "sessionScope": session_scope,
        "scopeHint": if session_scope {
            "Click saves as default · Right-click applies to this session only"
        } else {
            context
                .provider
                .map_or(MODEL_PICKER_DEFAULT_SCOPE_ONLY_REASON, model_picker_scope_reason)
        },
    })
}

/// The rows the open menu currently shows, which is what a `modelMenuPick` action looks a key up
/// in.
pub fn model_menu_visible_rows(
    context: &ModelMenuContext,
    view: &ModelMenuView,
    catalogs: &ModelMenuCatalogs,
    favorites: &[String],
) -> Vec<ModelMenuRow> {
    let entries = model_menu_entries(catalogs);
    let tab = view
        .tab
        .unwrap_or_else(|| model_menu_opening_tab(&entries, context.provider));
    model_menu_rows(
        &entries,
        tab,
        search_query(&entries, tab, view, favorites),
        favorites,
        &ModelMenuCurrent {
            provider: context
                .provider
                .map(|provider| provider.as_str().to_string()),
            model: context.model_value.clone(),
        },
        view.legacy_open,
    )
}
