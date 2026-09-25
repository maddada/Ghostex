//! How a pill choice reaches the agent: as a queued daemon request, or as text and keystrokes
//! typed into the TUI.
//!
//! Port of `packages/shared/session-chat-controller/option-dispatch.ts`. The TypeScript awaited
//! each step; here the decision tree produces the ordered list of steps and the caller performs
//! them, because the core performs no I/O and holds no promise.

use crate::menus::option_catalog::{OptionDescriptor, OptionDispatch, SessionOptionCatalog};
use crate::menus::option_values::{bounded_key_steps, cyclic_key_steps, OptionState};

/// What a model or effort pick from these pills is queued as, when it can be queued at all.
#[derive(Clone, Debug, PartialEq)]
pub enum QueuedOption {
    /// `picker.selectOptions({ mode })` or `{ fastMode }`.
    SelectOptions {
        mode: Option<String>,
        fast_mode: Option<String>,
    },
    /// `picker.select({ model, effort }, undefined, scope)`.
    SelectModel {
        model: String,
        effort: String,
        scope: Option<String>,
    },
    /// The pick was claimed but there is nothing to send (no model known yet).
    Swallowed,
}

/// `queueSessionChatOption`: whether the choice belongs to the daemon's queue rather than the TUI.
///
/// CDXC:SessionChat 2026-09-09 DECISION: Every quick-picker provider needs the durable delivery callback, including on draft sessions (Grok Build models and efforts change directly from chat).
pub fn queue_session_chat_option(
    descriptor: &OptionDescriptor,
    value: Option<&str>,
    catalog: Option<&SessionOptionCatalog>,
    state: &OptionState,
    queued_controls: bool,
    quick_picker: bool,
    scope: Option<&str>,
) -> Option<QueuedOption> {
    if queued_controls {
        if let Some(value) = value {
            if descriptor.id == "mode" {
                return Some(QueuedOption::SelectOptions {
                    mode: Some(value.to_string()),
                    fast_mode: None,
                });
            }
            if descriptor.id == "fastMode" {
                return Some(QueuedOption::SelectOptions {
                    mode: None,
                    fast_mode: Some(if value == "on" { "on" } else { "off" }.to_string()),
                });
            }
        }
    }
    let (Some(value), Some(catalog)) = (value, catalog) else {
        return None;
    };
    if !quick_picker || (descriptor.id != catalog.model.id && descriptor.id != "effort") {
        return None;
    }
    let model = if descriptor.id == catalog.model.id {
        Some(value.to_string())
    } else {
        state
            .get(&catalog.model.id)
            .map(|entry| entry.value.clone())
    };
    let Some(model) = model.filter(|model| !model.is_empty()) else {
        return Some(QueuedOption::Swallowed);
    };
    let effort_option = catalog
        .options_for_model(&model)
        .into_iter()
        .find(|entry| entry.id == "effort");
    let preferred = if descriptor.id == "effort" {
        Some(value.to_string())
    } else {
        state.get("effort").map(|entry| entry.value.clone())
    };
    let offered = effort_option.as_ref().and_then(|option| {
        option
            .choice_list()
            .iter()
            .find(|entry| Some(entry.value.as_str()) == preferred.as_deref())
            .map(|entry| entry.value.clone())
    });
    let keeps_agent_effort = descriptor.id == catalog.model.id && catalog.agent_keeps_effort();
    let effort = offered
        .or_else(|| keeps_agent_effort.then(String::new))
        .or_else(|| {
            effort_option
                .as_ref()
                .and_then(|option| option.default_value.clone())
        })
        .or_else(|| {
            effort_option.as_ref().and_then(|option| {
                option
                    .choice_list()
                    .first()
                    .map(|entry| entry.value.clone())
            })
        })
        .unwrap_or_default();
    Some(QueuedOption::SelectModel {
        model,
        effort,
        scope: scope.map(str::to_string),
    })
}

/// One step of typing a choice into the TUI.
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchStep {
    /// Type this text into the agent as a message.
    Command { text: String },
    /// Write this raw key.
    Key { key: String, marker: String },
    /// Ask the daemon to drive the agent's own model picker.
    PickModel { model: String, effort: String },
    /// Show the terminal: the agent's own picker owns the change.
    SwitchToTerminal,
    /// Hold sending while a cyclic mode change walks the TUI.
    Switching(bool),
}

/// The whole plan for one choice.
#[derive(Clone, Debug, PartialEq)]
pub struct DispatchPlan {
    /// The values to show optimistically before the first step runs, empty for a model picker
    /// whose real values are only known once the pick resolves.
    pub optimistic: Vec<(String, String)>,
    /// The values to show once the model picker's own pick is decided.
    pub picker_optimistic: Vec<(String, String)>,
    pub steps: Vec<DispatchStep>,
    /// The refusal the TypeScript throws, which the composer shows as an operation error.
    pub error: Option<String>,
}

/// `dispatchSessionChatOption`, as a plan rather than a chain of awaits.
///
/// `can_pick_model` is `onPickModel !== undefined`: without a daemon route the agent's own picker
/// in the terminal is the only way to change the model, exactly as `agent-picker` behaves.
pub fn plan_dispatch(
    descriptor: &OptionDescriptor,
    value: Option<&str>,
    catalog: Option<&SessionOptionCatalog>,
    state: &OptionState,
    can_pick_model: bool,
) -> DispatchPlan {
    let mut plan = DispatchPlan {
        optimistic: Vec::new(),
        picker_optimistic: Vec::new(),
        steps: Vec::new(),
        error: None,
    };
    if let Some(value) = value {
        if !matches!(descriptor.dispatch, OptionDispatch::ModelPicker) {
            plan.optimistic
                .push((descriptor.id.clone(), value.to_string()));
        }
    }
    let typed = value.unwrap_or("");
    match &descriptor.dispatch {
        OptionDispatch::Command(build) => {
            plan.steps.push(DispatchStep::Command {
                text: build.build(typed),
            });
        }
        OptionDispatch::CommandConfirmPicker(build) => {
            plan.steps.push(DispatchStep::Command {
                text: build.build(typed),
            });
            plan.steps.push(DispatchStep::Key {
                key: "enter".to_string(),
                marker: String::new(),
            });
        }
        OptionDispatch::ToggleCommand { command } => {
            plan.steps.push(DispatchStep::Command {
                text: command.clone(),
            });
        }
        OptionDispatch::ModelPicker => {
            let Some(catalog) = catalog.filter(|_| can_pick_model && value.is_some()) else {
                plan.steps.push(DispatchStep::Command {
                    text: "/model".to_string(),
                });
                plan.steps.push(DispatchStep::SwitchToTerminal);
                return plan;
            };
            let current_model = state
                .get(&catalog.model.id)
                .map(|entry| entry.value.clone());
            let current_effort = state.get("effort").map(|entry| entry.value.clone());
            let model = if descriptor.id == catalog.model.id {
                value.map(str::to_string)
            } else {
                current_model
            };
            let effort = if descriptor.id == "effort" {
                value.map(str::to_string)
            } else {
                model
                    .as_deref()
                    .and_then(|model| catalog.picker_effort_for(model, current_effort.as_deref()))
            };
            match (
                model.filter(|model| !model.is_empty()),
                effort.filter(|effort| !effort.is_empty()),
            ) {
                (Some(model), Some(effort)) => {
                    plan.picker_optimistic
                        .push((catalog.model.id.clone(), model.clone()));
                    plan.picker_optimistic
                        .push(("effort".to_string(), effort.clone()));
                    plan.steps.push(DispatchStep::PickModel { model, effort });
                }
                _ => {
                    plan.error = Some(
                        "The current model is not known yet, so there is nothing to change it from."
                            .to_string(),
                    );
                }
            }
        }
        OptionDispatch::AgentPicker { command } => {
            plan.steps.push(DispatchStep::Command {
                text: command.clone(),
            });
            plan.steps.push(DispatchStep::SwitchToTerminal);
        }
        OptionDispatch::TerminalHandoff => {
            // Nothing is typed: the agent's own picker owns the change.
            plan.steps.push(DispatchStep::SwitchToTerminal);
        }
        OptionDispatch::BoundedKeySteps {
            decrease_key,
            increase_key,
        } => {
            for key in bounded_key_steps(
                descriptor.choice_list(),
                state.get(&descriptor.id).map(|entry| entry.value.as_str()),
                typed,
                decrease_key,
                increase_key,
            ) {
                plan.steps.push(DispatchStep::Key {
                    key,
                    marker: String::new(),
                });
            }
        }
        OptionDispatch::CyclicKeySteps { key } => {
            let keys = cyclic_key_steps(
                descriptor.choice_list(),
                state.get(&descriptor.id).map(|entry| entry.value.as_str()),
                typed,
                key,
            );
            if value.is_none() || keys.is_empty() {
                return plan;
            }
            plan.steps.push(DispatchStep::Switching(true));
            for key in keys {
                plan.steps.push(DispatchStep::Key {
                    key,
                    marker: String::new(),
                });
            }
            plan.steps.push(DispatchStep::Switching(false));
        }
        OptionDispatch::Key { key, marker } => {
            plan.steps.push(DispatchStep::Key {
                key: key.clone(),
                marker: marker.clone(),
            });
        }
    }
    plan
}

/// The descriptor a `selectOption` names, out of the model pill and the current option list.
pub fn descriptor_for(
    catalog: Option<&SessionOptionCatalog>,
    option_descriptors: &[OptionDescriptor],
    descriptor_id: &str,
) -> Option<OptionDescriptor> {
    if let Some(catalog) = catalog {
        if catalog.model.id == descriptor_id {
            return Some(catalog.model.clone());
        }
    }
    option_descriptors
        .iter()
        .find(|descriptor| descriptor.id == descriptor_id)
        .cloned()
}

/// `command.exitPlan`: leaving Plan mode is a Shift+Tab, not the `/plan` command again.
pub fn exit_plan_delivery(descriptor: &OptionDescriptor) -> OptionDescriptor {
    let mut delivery = descriptor.clone();
    delivery.dispatch = OptionDispatch::Key {
        key: "shift-tab".to_string(),
        marker: String::new(),
    };
    delivery
}

/// `modelPickScope(provider, secondary)`.
///
/// CDXC:SessionChat 2026-09-21 DECISION:
/// User: "left-clicking on a model selected, as always, the default. Right-clicking should just
/// apply that for that session." The Session-only model picks setting and the Also set as default
/// switch are removed.
pub fn model_pick_scope(provider: Option<&str>, secondary: bool) -> &'static str {
    match provider {
        Some(provider)
            if secondary && crate::menus::options::picker_supports_session_scope(provider) =>
        {
            "session"
        }
        _ => "default",
    }
}
