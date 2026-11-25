//! The model picker with no chat behind it: Settings > Autopilot's "Supervisor model" and "When
//! it's at its limit" rows open it to pick an agent, a model and an effort for a session that does
//! not exist yet. gxserver projects it (server/src/server/autopilot_http.rs `supervisorMenu`) so
//! the desktop, the web build and the phone draw the very `modelMenu` a new session's picker
//! draws, from the same lineup, rows, tabs, stars and search.
//!
//! CDXC:Autopilot 2026-11-26 DECISION:
//! Issue #765, user: "show the model picker we show when user is on new session, he can pick any agent and any model and any effort there" (mockup Q5008 picker, Q5009 fallback row, Q5010 the phone's model sheet). A pick here is no option change and no hand-off: it names the agent, model and effort a later start uses. So every tab is a plain list (no hand-off badge), the footer keeps only Reasoning and Context Window (no Fast mode, no Account: the supervisor starts on whichever account has room), and only the agents the caller lets through get a tab (the supervisor needs a "run without asking").

use serde_json::{json, Value};

use crate::menus::catalog::AgentModelCatalog;
use crate::menus::option_catalog::session_option_catalog;
use crate::menus::picker::model_menu::{
    model_menu_pick_value, ModelChoice, ModelMenuCatalog, ModelMenuCatalogs, ModelMenuTabId,
    ModelMenuView, MODEL_MENU_PROVIDERS,
};
use crate::menus::picker::model_picker::ModelPickerProvider;
use crate::menus::picker::projection::{
    model_menu_projection, model_menu_visible_rows, ModelMenuContext,
};
use crate::menus::picker::traits::{ModelMenuTrait, ModelMenuTraitChoice};

/// What the standalone picker has picked: an agent (a picker provider id such as `claude`,
/// `antigravity`), a model value of its lineup and an effort, empty for a model with none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StandalonePick {
    pub agent: String,
    pub model: String,
    pub effort: String,
}

/// The view a client keeps between calls: the open tab (`None` opens on the pick's agent), the
/// search text and the Legacy models fold.
pub fn standalone_view(tab: Option<&str>, query: &str, legacy_open: bool) -> ModelMenuView {
    ModelMenuView {
        tab: tab.map(ModelMenuTabId::from_wire),
        query: query.to_string(),
        legacy_open,
        refreshing: None,
    }
}

/// The lineups of `providers`, in the picker's own provider order.
fn catalogs(catalog: &AgentModelCatalog, providers: &[ModelPickerProvider]) -> ModelMenuCatalogs {
    let mut catalogs = ModelMenuCatalogs::new();
    for provider in MODEL_MENU_PROVIDERS {
        if !providers.contains(&provider) {
            continue;
        }
        let Some(lineup) = session_option_catalog(catalog, Some(provider.as_str())) else {
            continue;
        };
        catalogs.insert(
            provider.as_str().to_string(),
            ModelMenuCatalog {
                model_icon: lineup.model_icon.clone(),
                choices: lineup
                    .model
                    .choice_list()
                    .iter()
                    .map(|choice| ModelChoice {
                        value: choice.value.clone(),
                        label: choice.label.clone(),
                        description: choice.description.clone(),
                        legacy: choice.group.as_deref()
                            == Some(crate::menus::catalog::LEGACY_GROUP_ID),
                    })
                    .collect(),
                agent_name: None,
            },
        );
    }
    catalogs
}

fn context(pick: Option<&StandalonePick>) -> ModelMenuContext {
    let provider = pick.and_then(|pick| ModelPickerProvider::from_wire(&pick.agent));
    ModelMenuContext {
        provider,
        model_value: pick
            .map(|pick| pick.model.clone())
            .filter(|model| !model.is_empty()),
        effort_value: pick
            .map(|pick| pick.effort.clone())
            .filter(|effort| !effort.is_empty()),
        draft: true,
        raw: json!({}),
        ..Default::default()
    }
}

/// The pick's own row as the projection draws it, wherever its tab is.
fn picked_row(
    catalogs: &ModelMenuCatalogs,
    catalog: &AgentModelCatalog,
    pick: &StandalonePick,
) -> Option<Value> {
    let context = context(Some(pick));
    let view = ModelMenuView {
        tab: context.provider.map(ModelMenuTabId::Provider),
        legacy_open: true,
        ..Default::default()
    };
    let projected = model_menu_projection(&context, &view, catalogs, &[], catalog);
    projected["rows"]
        .as_array()?
        .iter()
        .find(|row| row["selected"] == true)
        .cloned()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

/// Reasoning (the picked model's levels) and Context Window (when it has two), as the new
/// session's picker draws them.
fn traits(row: Option<&Value>, pick: Option<&StandalonePick>) -> Vec<Value> {
    let effort = pick.map(|pick| pick.effort.as_str()).unwrap_or_default();
    let model = pick.map(|pick| pick.model.as_str()).unwrap_or_default();
    let efforts: Vec<&Value> = row
        .and_then(|row| row["efforts"].as_array())
        .map(|levels| levels.iter().collect())
        .unwrap_or_default();
    let row_effort = row.map(|row| text(&row["effort"])).unwrap_or_default();
    let reasoning = ModelMenuTrait {
        id: "effort".to_string(),
        label: "Reasoning".to_string(),
        value_label: Some(
            efforts
                .iter()
                .find(|level| level["value"] == effort)
                .map(|level| text(&level["label"]))
                .unwrap_or_else(|| "Default".to_string()),
        ),
        disabled: efforts.is_empty().then_some(true),
        choices: efforts
            .iter()
            .map(|level| ModelMenuTraitChoice {
                value: text(&level["value"]),
                label: text(&level["label"]),
                selected: level["value"] == effort,
                is_default: level["value"] == row_effort.as_str(),
                exit_plan: None,
            })
            .collect(),
        icon: Some("reasoning"),
        toggle: None,
        hint: None,
    };
    let mut traits = vec![reasoning.to_json()];
    let variants: Vec<&Value> = row
        .and_then(|row| row["variants"].as_array())
        .map(|variants| variants.iter().collect())
        .unwrap_or_default();
    if !variants.is_empty() {
        traits.push(
            ModelMenuTrait {
                id: "context".to_string(),
                label: "Context Window".to_string(),
                value_label: variants
                    .iter()
                    .find(|variant| variant["value"] == model)
                    .map(|variant| text(&variant["label"])),
                disabled: None,
                choices: variants
                    .iter()
                    .map(|variant| ModelMenuTraitChoice {
                        value: text(&variant["value"]),
                        label: text(&variant["label"]),
                        selected: variant["value"] == model,
                        is_default: crate::menus::picker::model_menu::is_long_context_value(
                            variant["value"].as_str().unwrap_or_default(),
                        ),
                        exit_plan: None,
                    })
                    .collect(),
                icon: Some("context"),
                toggle: None,
                hint: None,
            }
            .to_json(),
        );
    }
    traits
}

/// The `modelMenu` document for the standalone picker: the new session's picker over `providers`,
/// with `pick` selected.
pub fn standalone_model_menu(
    catalog: &AgentModelCatalog,
    providers: &[ModelPickerProvider],
    pick: Option<&StandalonePick>,
    view: &ModelMenuView,
    favorites: &[String],
) -> Value {
    let catalogs = catalogs(catalog, providers);
    let context = context(pick);
    let mut menu = model_menu_projection(&context, view, &catalogs, favorites, catalog);
    let row = pick.and_then(|pick| picked_row(&catalogs, catalog, pick));
    menu["traits"] = Value::Array(traits(row.as_ref(), pick));
    // A pick names a later start: no agent CLI to hand over to, no session scope.
    menu["sessionScope"] = json!(false);
    menu["scopeHint"] = Value::Null;
    menu["degradedNote"] = Value::Null;
    if let Some(tabs) = menu["tabs"].as_array_mut() {
        for tab in tabs {
            tab["handoff"] = json!(false);
        }
    }
    menu
}

/// What a click on row `key` of the open tab picks, carrying `effort` when the keyboard or the
/// Reasoning list moved one (else the row's own level).
pub fn standalone_pick_row(
    catalog: &AgentModelCatalog,
    providers: &[ModelPickerProvider],
    pick: Option<&StandalonePick>,
    view: &ModelMenuView,
    favorites: &[String],
    key: &str,
    effort: Option<&str>,
) -> Option<StandalonePick> {
    let catalogs = catalogs(catalog, providers);
    let context = context(pick);
    let visible = model_menu_visible_rows(&context, view, &catalogs, favorites);
    let row = visible.into_iter().find(|row| row.key == key)?;
    let same_agent = Some(row.entry.provider) == context.provider;
    let model = model_menu_pick_value(
        &row.entry,
        same_agent
            .then_some(context.model_value.as_deref())
            .flatten(),
    );
    // The row's levels and its starting level, as the projection works them out.
    let projected = model_menu_projection(&context, view, &catalogs, favorites, catalog);
    let projected_row = projected["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|candidate| candidate["key"] == key))
        .cloned()
        .unwrap_or(Value::Null);
    let offers = |level: &str| {
        projected_row["efforts"]
            .as_array()
            .is_some_and(|levels| levels.iter().any(|entry| entry["value"] == level))
    };
    let effort = effort
        .filter(|level| offers(level))
        .map(str::to_string)
        .or_else(|| projected_row["effort"].as_str().map(str::to_string))
        .unwrap_or_default();
    Some(StandalonePick {
        agent: row.entry.provider.as_str().to_string(),
        model,
        effort,
    })
}

/// A footer button's pick: Reasoning sets the effort, Context Window the model variant.
pub fn standalone_pick_trait(pick: &StandalonePick, id: &str, value: &str) -> StandalonePick {
    let mut next = pick.clone();
    match id {
        "effort" => next.effort = value.to_string(),
        "context" => next.model = value.to_string(),
        _ => {}
    }
    next
}
