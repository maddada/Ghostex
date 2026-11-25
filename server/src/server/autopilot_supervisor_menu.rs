//! Settings > Autopilot's two supervisor pickers: "Supervisor model" and "When it's at its limit".
//! gxserver projects the new session's model picker for them (gx-chat-core
//! `menus/picker/standalone.rs`) from the lineup it serves, so the desktop, the web build and the
//! phone draw the same tabs, rows, stars and Reasoning button a new session's picker draws.
//!
//! - `op: "supervisorMenu"`: `slot` (`supervisor` or `fallback`), the view the client keeps (`tab`,
//!   `query`, `legacyOpen`) and its starred models (`favorites`); answers `{ menu, automatic }`.
//! - `op: "supervisorPick"`: the same, plus one of `key` (a row of the open tab, with `effort`
//!   when the keyboard or the Reasoning list moved one), `trait: { id, value }` (Reasoning or
//!   Context Window) or `automatic: true` (the fallback row's "Automatic"). Only the user's own
//!   devices may pick (autopilot_http.rs `SUPERVISOR_ONLY_YOU`).
//!
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/autopilot/supervisor_picker.rs, apps/mobile/app/src/screens/settings/AutopilotSupervisorRows.tsx.

use ghostex_gx_chat_core::menus::catalog::{
    bundled_agent_model_catalog, parse_agent_model_catalog, AgentModelCatalog,
};
use ghostex_gx_chat_core::menus::picker::model_menu::MODEL_MENU_PROVIDERS;
use ghostex_gx_chat_core::menus::picker::model_picker::ModelPickerProvider;
use ghostex_gx_chat_core::menus::picker::standalone::{
    standalone_model_menu, standalone_pick_row, standalone_pick_trait, standalone_view,
    StandalonePick,
};
use serde_json::{json, Map, Value};

use crate::autopilot::{AutopilotConfig, SupervisorPick};
use crate::domain::{DomainRepository, DomainStateError};

/// Which of the two rows a call is about.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Supervisor,
    Fallback,
}

pub(super) fn slot(params: &Map<String, Value>) -> Slot {
    match params.get("slot").and_then(Value::as_str) {
        Some("fallback") => Slot::Fallback,
        _ => Slot::Supervisor,
    }
}

fn catalog() -> Option<AgentModelCatalog> {
    parse_agent_model_catalog(&crate::agent_model_catalog::current())
        .or_else(bundled_agent_model_catalog)
}

/// The agents the pickers offer: the picker's own, turned on and installed here (the agent list
/// every project stores), that Ghostex can start without asking.
fn providers(repository: &DomainRepository<'_>) -> Vec<ModelPickerProvider> {
    let enabled = repository
        .list_projects()
        .ok()
        .and_then(|projects| projects.into_iter().next())
        .map(|project| crate::agents::enabled_agent_icons(&project));
    MODEL_MENU_PROVIDERS
        .into_iter()
        .filter(|provider| super::autopilot_judge::supervisor_agent_allowed(provider.as_str()))
        .filter(|provider| {
            enabled.as_ref().is_none_or(|icons| {
                icons.iter().any(|icon| {
                    ghostex_gx_chat_core::menus::picker::model_picker::model_picker_provider(Some(
                        icon,
                    )) == Some(*provider)
                })
            })
        })
        .collect()
}

fn standalone(pick: &SupervisorPick) -> StandalonePick {
    StandalonePick {
        agent: pick.agent.clone(),
        model: pick.model.clone(),
        effort: pick.effort.clone(),
    }
}

fn slot_pick(config: &AutopilotConfig, slot: Slot) -> Option<SupervisorPick> {
    match slot {
        Slot::Supervisor => Some(config.supervisor_pick()),
        Slot::Fallback => config.supervisor_fallbacks.first().cloned(),
    }
}

fn favorites(params: &Map<String, Value>) -> Vec<String> {
    params
        .get("favorites")
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

fn view(params: &Map<String, Value>) -> ghostex_gx_chat_core::menus::picker::ModelMenuView {
    standalone_view(
        params.get("tab").and_then(Value::as_str),
        params
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        params.get("legacyOpen").and_then(Value::as_bool) == Some(true),
    )
}

/// `supervisorMenu`.
pub(super) fn menu(
    repository: &DomainRepository<'_>,
    config: &AutopilotConfig,
    params: &Map<String, Value>,
) -> Value {
    let slot = slot(params);
    let pick = slot_pick(config, slot);
    let menu = catalog().map(|catalog| {
        standalone_model_menu(
            &catalog,
            &providers(repository),
            pick.as_ref().map(standalone).as_ref(),
            &view(params),
            &favorites(params),
        )
    });
    json!({
        "menu": menu,
        "automatic": slot == Slot::Fallback && pick.is_none(),
    })
}

/// `supervisorPick`: the slot and its new pick (`None` is the fallback row's "Automatic").
pub(super) fn pick(
    repository: &DomainRepository<'_>,
    config: &AutopilotConfig,
    params: &Map<String, Value>,
) -> Result<(Slot, Option<SupervisorPick>), DomainStateError> {
    let slot = slot(params);
    if params.get("automatic").and_then(Value::as_bool) == Some(true) {
        return match slot {
            Slot::Fallback => Ok((slot, None)),
            Slot::Supervisor => Err(DomainStateError::bad_request(
                "Only the fallback row has Automatic.",
            )),
        };
    }
    let current = slot_pick(config, slot).map(|pick| standalone(&pick));
    let next = if let Some(change) = params.get("trait").and_then(Value::as_object) {
        let current = current.ok_or_else(|| {
            DomainStateError::bad_request("Pick a model before its reasoning or context window.")
        })?;
        standalone_pick_trait(
            &current,
            change.get("id").and_then(Value::as_str).unwrap_or_default(),
            change
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
    } else {
        let key = params
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| DomainStateError::bad_request("key, trait or automatic is required."))?;
        let catalog = catalog()
            .ok_or_else(|| DomainStateError::bad_request("Ghostex has no model lineup yet."))?;
        standalone_pick_row(
            &catalog,
            &providers(repository),
            current.as_ref(),
            &view(params),
            &favorites(params),
            key,
            params.get("effort").and_then(Value::as_str),
        )
        .ok_or_else(|| DomainStateError::bad_request("That model isn't in the picker."))?
    };
    Ok((
        slot,
        Some(SupervisorPick::new(&next.agent, &next.model, &next.effort)),
    ))
}

/// Applies a `supervisorPick` to the saved choices.
pub(super) fn apply(config: &mut AutopilotConfig, slot: Slot, pick: Option<SupervisorPick>) {
    match (slot, pick) {
        (Slot::Supervisor, Some(pick)) => config.supervisor_pick = Some(pick),
        (Slot::Supervisor, None) => {}
        (Slot::Fallback, pick) => config.supervisor_fallbacks = pick.into_iter().collect(),
    }
}

/// How a pick reads on its row's button and in the status line: its agent's logo, the model's
/// name and the effort ("Opus 5.5 · Medium"), and the agent's name.
pub(super) fn pick_label(pick: &SupervisorPick) -> Value {
    let row = catalog().and_then(|catalog| {
        let menu = standalone_model_menu(
            &catalog,
            &MODEL_MENU_PROVIDERS,
            Some(&standalone(pick)),
            &standalone_view(Some(&pick.agent), "", true),
            &[],
        );
        menu["rows"]
            .as_array()?
            .iter()
            .find(|row| row["selected"] == true)
            .cloned()
    });
    let model = row
        .as_ref()
        .and_then(|row| row["label"].as_str().map(str::to_string))
        .unwrap_or_else(|| pick.model.clone());
    let effort = row.as_ref().and_then(|row| {
        row["efforts"]
            .as_array()?
            .iter()
            .find(|level| level["value"] == pick.effort.as_str())
            .and_then(|level| level["label"].as_str().map(str::to_string))
    });
    let agent = crate::sidebar_hud::default_agent_name(&pick.agent)
        .map(str::to_string)
        .unwrap_or_else(|| pick.agent.clone());
    let icon = row
        .as_ref()
        .and_then(|row| row["icon"].as_str().map(str::to_string))
        .unwrap_or_else(|| pick.agent.clone());
    let label = match (&effort, model.is_empty()) {
        (_, true) => agent.clone(),
        (Some(effort), false) => format!("{model} · {effort}"),
        (None, false) => model.clone(),
    };
    json!({
        "agent": pick.agent,
        "agentName": agent,
        "icon": icon,
        "model": model,
        "effort": effort,
        "label": label,
    })
}

/// The two rows' labels, for the read answer.
pub(super) fn labels(config: &AutopilotConfig) -> Value {
    json!({
        "supervisor": pick_label(&config.supervisor_pick()),
        "fallback": config.supervisor_fallbacks.first().map(pick_label),
    })
}
