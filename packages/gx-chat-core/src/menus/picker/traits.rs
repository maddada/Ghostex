//! The picker's footer buttons and the merged pill's labels.
//!
//! Port of `modelMenuTraits` and `modelMenuPillLabels` from
//! `packages/shared/session-chat-presentation/model-menu.ts`.
//!
//! The TypeScript called `sessionChatOptionRows(descriptor, state, caps)` and
//! `sessionChatOptionValueLabel(descriptor, state)` for each descriptor. Those two live in family
//! e1's session option catalog, so the input here is the already-resolved rows and labels:
//! [`ResolvedOptionDescriptor`] is exactly what e1 answers for one descriptor, in the order
//! `others` has after `isShiftTabModeCycler` has been filtered out.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::menus::catalog::{AgentModelCatalog, FAST_MODE_SCOPE_DEFAULT};
use crate::menus::picker::model_menu::{
    is_long_context_value, model_menu_entry_for, ModelMenuEntries,
};

/// One choice of an option, flattened out of `rows.sections`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionChoice {
    pub value: String,
    pub label: String,
}

/// What `sessionChatOptionRows` answered for one descriptor.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionRows {
    /// `kind: 'action'`: no footer button, because there is no value to show.
    Action,
    /// `kind: 'toggle'`.
    Toggle {
        checked: bool,
        /// Where the toggle would go, so it names the choice that is NOT selected.
        value: String,
        disabled: bool,
        /// Leaving Codex plan mode is a key press, not a value.
        exit_plan: bool,
    },
    /// `kind: 'choices'`, with `rows.sections` already flattened.
    Choices {
        choices: Vec<OptionChoice>,
        current: Option<String>,
    },
}

/// One of family e1's option descriptors, resolved against the session's option state.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedOptionDescriptor {
    pub id: String,
    /// `descriptor.label`.
    pub label: String,
    /// `descriptor.actionLabel`.
    pub action_label: Option<String>,
    /// `descriptor.defaultValue`.
    pub default_value: Option<String>,
    /// `state[descriptor.id]?.value`.
    pub held_value: Option<String>,
    /// `sessionChatOptionValueLabel(descriptor, state)`.
    pub value_label: Option<String>,
    pub rows: OptionRows,
}

/// One choice of a footer button.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuTraitChoice {
    pub value: String,
    pub label: String,
    pub selected: bool,
    pub is_default: bool,
    /// Leaving Codex plan mode is a key press, not a value; the option command carries this
    /// through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_plan: Option<bool>,
}

/// One footer button.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelMenuTrait {
    /// `context` picks a model variant; anything else is the option descriptor with this id.
    pub id: String,
    pub label: String,
    pub value_label: Option<String>,
    pub disabled: Option<bool>,
    pub choices: Vec<ModelMenuTraitChoice>,
    /// The glyph drawn beside the value: `reasoning`, `context` or `fast`.
    pub icon: Option<&'static str>,
    /// Present when the button has at most two values: the one a click moves to.
    pub toggle: Option<(String, bool)>,
    /// A note the button's ⓘ shows on hover or tap, kept out of the row itself.
    pub hint: Option<String>,
}

impl ModelMenuTrait {
    /// The document's shape for one button, with `undefined` keys left out.
    pub fn to_json(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("id".into(), json!(self.id));
        object.insert("label".into(), json!(self.label));
        object.insert("valueLabel".into(), json!(self.value_label));
        if let Some(disabled) = self.disabled {
            object.insert("disabled".into(), json!(disabled));
        }
        object.insert(
            "choices".into(),
            serde_json::to_value(&self.choices).unwrap_or(Value::Null),
        );
        if let Some(icon) = self.icon {
            object.insert("icon".into(), json!(icon));
        }
        if let Some((value, exit_plan)) = &self.toggle {
            let mut toggle = serde_json::Map::new();
            toggle.insert("value".into(), json!(value));
            if *exit_plan {
                toggle.insert("exitPlan".into(), json!(true));
            }
            object.insert("toggle".into(), Value::Object(toggle));
        }
        if let Some(hint) = &self.hint {
            object.insert("hint".into(), json!(hint));
        }
        Value::Object(object)
    }
}

/// `TRAIT_LABELS`.
fn trait_label(id: &str) -> Option<&'static str> {
    match id {
        "effort" => Some("Reasoning"),
        _ => None,
    }
}

/// `TRAIT_ICONS`.
fn trait_icon(id: &str) -> Option<&'static str> {
    match id {
        "effort" => Some("reasoning"),
        "context" => Some("context"),
        "fastMode" => Some("fast"),
        _ => None,
    }
}

/// CDXC:SessionChat 2026-09-22 DECISION:
/// User: "always show the 3 bottom buttons in all cases, make them disabled when they don't make
/// sense and say Default for options that don't have options, or N/A where makes sense."
/// A model with one reasoning level or one context window runs on its default, so those read
/// Default; an agent or model without a fast mode reads Off (the user asked for Off rather than
/// N/A on 2026-09-22, which supersedes the N/A this comment carried).
fn unavailable(id: &str, label: &str, value_label: &str) -> ModelMenuTrait {
    ModelMenuTrait {
        id: id.to_string(),
        label: label.to_string(),
        value_label: Some(value_label.to_string()),
        disabled: Some(true),
        choices: Vec::new(),
        icon: None,
        toggle: None,
        hint: None,
    }
}

/// CDXC:SessionChat 2026-09-21 DECISION:
/// User: Reasoning, Context Window and Fast Mode are three buttons along the bottom of the
/// picker, each an icon beside its value (brain, chart bars, bolt); clicking Fast or the Context
/// Window toggles it, since it usually has only two values. This supersedes the full-width footer
/// rows that each opened a side list; only a button with more than two values, such as Reasoning,
/// still opens one.
fn as_button(mut trait_row: ModelMenuTrait) -> ModelMenuTrait {
    let next = if trait_row.choices.len() <= 2 {
        trait_row
            .choices
            .iter()
            .find(|choice| !choice.selected)
            .map(|choice| (choice.value.clone(), choice.exit_plan == Some(true)))
    } else {
        None
    };
    trait_row.icon = trait_icon(&trait_row.id);
    trait_row.toggle = next;
    trait_row
}

/// `option(descriptor)`: one descriptor's footer button, or `None` when it has no value to show.
fn option(
    descriptor: &ResolvedOptionDescriptor,
    catalog: &AgentModelCatalog,
) -> Option<ModelMenuTrait> {
    let label = trait_label(&descriptor.id)
        .map(str::to_string)
        .or_else(|| descriptor.action_label.clone())
        .unwrap_or_else(|| descriptor.label.clone());
    match &descriptor.rows {
        OptionRows::Toggle {
            checked,
            value,
            disabled,
            exit_plan,
        } => {
            let held = descriptor.held_value.clone();
            let on = if *checked {
                held.clone().unwrap_or_else(|| "on".to_string())
            } else {
                value.clone()
            };
            let off = if *checked {
                value.clone()
            } else {
                held.unwrap_or_else(|| "off".to_string())
            };
            Some(ModelMenuTrait {
                id: descriptor.id.clone(),
                label,
                value_label: Some(if *checked { "On" } else { "Off" }.to_string()),
                disabled: Some(*disabled),
                choices: vec![
                    ModelMenuTraitChoice {
                        value: on,
                        label: "On".to_string(),
                        selected: *checked,
                        is_default: false,
                        exit_plan: None,
                    },
                    ModelMenuTraitChoice {
                        value: off,
                        label: "Off".to_string(),
                        selected: !*checked,
                        is_default: true,
                        exit_plan: Some(*exit_plan),
                    },
                ],
                icon: None,
                toggle: None,
                hint: None,
            })
        }
        OptionRows::Choices { choices, current } => Some(ModelMenuTrait {
            id: descriptor.id.clone(),
            label,
            value_label: match (descriptor.id.as_str(), current.as_deref()) {
                ("effort", Some(current)) => Some(catalog.effort_label(current)),
                _ => descriptor.value_label.clone(),
            },
            disabled: None,
            choices: choices
                .iter()
                .map(|choice| ModelMenuTraitChoice {
                    value: choice.value.clone(),
                    label: choice.label.clone(),
                    selected: Some(choice.value.as_str()) == current.as_deref(),
                    is_default: Some(choice.value.as_str()) == descriptor.default_value.as_deref(),
                    exit_plan: None,
                })
                .collect(),
            icon: None,
            toggle: None,
            hint: None,
        }),
        OptionRows::Action => None,
    }
}

impl ModelMenuTrait {
    fn with_hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
    }
}

/// CDXC:SessionChat 2026-10-28 DECISION:
/// User: Codex's Fast is labelled as a default, "make label small i icon not long text, keep ui not
/// crowded". Codex saves Fast to `~/.codex/config.toml`, so turning it on in one chat starts every
/// new Codex session on Fast (found testing Codex on Linux). The behaviour stays Codex's; the Fast
/// button carries this note behind an ⓘ (desktop and web `model_menu/render.rs`, the phone's
/// `ModelMenuTray.tsx`) and nothing more in the row.
fn fast_mode_hint(provider: Option<&str>, catalog: &AgentModelCatalog) -> Option<String> {
    let agent = catalog.agents.get(provider?)?;
    (agent.fast_mode.scope.as_deref() == Some(FAST_MODE_SCOPE_DEFAULT)).then(|| {
        let name = agent.name.trim_end_matches(" CLI");
        format!("{name} keeps Fast for new {name} sessions too.")
    })
}

/// `modelMenuTraits`: the footer buttons for the session's current model.
///
/// Reasoning, the context window when the model has two, then every other option the old Options
/// pill listed. Mode stays on its own pill. `others` is `params.descriptors` with the Shift+Tab
/// mode cycler already filtered out, which is family e1's `isShiftTabModeCycler`.
pub fn model_menu_traits(
    entries: &ModelMenuEntries,
    others: &[ResolvedOptionDescriptor],
    provider: Option<&str>,
    model: Option<&str>,
    detected_fast: Option<&str>,
    catalog: &AgentModelCatalog,
) -> Vec<ModelMenuTrait> {
    let entry = model_menu_entry_for(entries, provider, model);
    let effort = others.iter().position(|entry| entry.id == "effort");
    let fast = others.iter().position(|entry| entry.id == "fastMode");
    let mut traits = Vec::new();
    traits.push(
        effort
            .and_then(|index| option(&others[index], catalog))
            .unwrap_or_else(|| unavailable("effort", "Reasoning", "Default")),
    );
    traits.push(match entry {
        Some(entry) if !entry.variants.is_empty() => ModelMenuTrait {
            id: "context".to_string(),
            label: "Context Window".to_string(),
            value_label: entry
                .variants
                .iter()
                .find(|variant| Some(variant.value.as_str()) == model)
                .map(|variant| variant.label.clone()),
            disabled: None,
            choices: entry
                .variants
                .iter()
                .map(|variant| ModelMenuTraitChoice {
                    value: variant.value.clone(),
                    label: variant.label.clone(),
                    selected: Some(variant.value.as_str()) == model,
                    is_default: is_long_context_value(&variant.value),
                    exit_plan: None,
                })
                .collect(),
            icon: None,
            toggle: None,
            hint: None,
        },
        _ => unavailable("context", "Context Window", "Default"),
    });
    traits.push(
        fast.and_then(|index| option(&others[index], catalog))
            .unwrap_or_else(|| {
                // `detectedFastLabel`: Cursor has no Fast toggle in chat, but gxserver reads
                // "Fast" off its footer, so the disabled button reports that state.
                let label = if detected_fast == Some("on") {
                    "On"
                } else {
                    "Off"
                };
                unavailable("fastMode", "Fast mode", label)
            })
            .with_hint(fast_mode_hint(provider, catalog)),
    );
    for (index, descriptor) in others.iter().enumerate() {
        if Some(index) == effort || Some(index) == fast {
            continue;
        }
        if let Some(trait_row) = option(descriptor, catalog) {
            traits.push(trait_row);
        }
    }
    traits.into_iter().map(as_button).collect()
}

/// `modelMenuPillLabels`: the model's name, then the footer values that are set, as "High · 200K".
///
/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: "don't show 1M in the chat composer when we have that one selected, only show 200k when
/// that one is selected". 1M is the normal window, so the pill names the context window only
/// when it is 200K; the picker's Context Window button still reads 1M or 200K.
pub fn model_menu_pill_labels(
    entries: &ModelMenuEntries,
    provider: Option<&str>,
    model: Option<&str>,
    model_label: Option<&str>,
    traits: &[ModelMenuTrait],
) -> Value {
    let entry = model_menu_entry_for(entries, provider, model);
    let parts: Vec<String> = traits
        .iter()
        .filter(|trait_row| {
            (trait_row.id == "effort" || trait_row.id == "context") && !trait_row.choices.is_empty()
        })
        .filter(|trait_row| {
            trait_row.id != "context"
                || !trait_row
                    .choices
                    .iter()
                    .any(|choice| choice.selected && is_long_context_value(&choice.value))
        })
        .filter_map(|trait_row| trait_row.value_label.clone())
        .filter(|value| !value.is_empty())
        .collect();
    let label = entry
        .map(|entry| entry.label.clone())
        .or_else(|| model_label.map(str::to_string));
    let suffix = (!parts.is_empty()).then(|| parts.join(" · "));
    // The pill cuts a long name to its width, so its tooltip carries the whole of it.
    let tooltip = label.as_ref().map(|label| match &suffix {
        Some(suffix) => format!("{label} \u{b7} {suffix}"),
        None => label.clone(),
    });
    json!({
        "label": label,
        "suffix": suffix,
        "tooltip": tooltip,
    })
}
