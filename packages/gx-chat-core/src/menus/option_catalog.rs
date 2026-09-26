//! Per-agent session-option catalogs for the composer footer pills.
//!
//! Port of the catalog half of `packages/core-ui/chat/session-chat-session-options.ts`.
//!
//! The agent is a TUI: there is no API to set a model or a reasoning effort, only keystrokes. So
//! every option here is DELIVERED as a slash command (or a raw key) typed into the running agent.
//! A displayed value is either pending local intent (`dispatched`) or agent-owned evidence
//! (`detected`) read from the transcript or statusline by gxserver. No catalog value is presented
//! as the current session truth without evidence.
//!
//! CDXC:AgentProviders 2026-09-02 WHY:
//! The model lineup, the effort levels and the fast-mode support of every agent come from the
//! published agent model catalog (`catalog.rs`), bundled as a snapshot and refreshed from the
//! repo's main branch at runtime. This module only decides HOW each option is delivered to the
//! TUI; it never names a model itself.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer};

use crate::menus::catalog::{AgentModelCatalog, CatalogAgent, CatalogGroup, CatalogModel};

/// CDXC:AgentScreenDetection 2026-09-04 DECISION:
/// User: fast mode and plan mode sit together under one "Modes" section of the options dropdown,
/// fast mode first, with no separate "Fast mode" section. The pills merge consecutive descriptors
/// that share a label into one section, so both toggles carry this label and the category order
/// puts fast above plan.
pub const MODES_SECTION_LABEL: &str = "Modes";

/// Options-pill ordering; the model category has its own pill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptionCategory {
    Model,
    ThoughtLevel,
    ModelConfig,
    Mode,
}

impl OptionCategory {
    /// `CATEGORY_ORDER`.
    fn order(self) -> i32 {
        match self {
            Self::Model => -1,
            Self::ThoughtLevel => 0,
            Self::ModelConfig => 1,
            Self::Mode => 2,
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::ThoughtLevel => "thought_level",
            Self::ModelConfig => "model_config",
            Self::Mode => "mode",
        }
    }
}

impl Serialize for OptionCategory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.wire())
    }
}

/// One row of a choice list.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionChoice {
    pub value: String,
    pub label: String,
    /// The CLI's own row text when it differs from `label`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picker_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Id of the `choiceGroups` submenu this row is nested under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// A submenu of choices.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionChoiceGroup {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<&CatalogGroup> for OptionChoiceGroup {
    fn from(group: &CatalogGroup) -> Self {
        Self {
            id: group.id.clone(),
            label: group.label.clone(),
            description: group.description.clone(),
        }
    }
}

/// How a `command` dispatch turns a value into the text typed into the TUI.
///
/// The TypeScript carries a closure here, which `JSON.stringify` drops; the Rust keeps the table
/// the closure would have consulted, so the command for a value is still exact and the descriptor
/// still serializes to `{"kind": "command"}` alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandBuild {
    /// The commands for the values the descriptor offers.
    by_value: BTreeMap<String, String>,
    /// `${prefix}${value}` for anything else, which is what every builder falls back to for a
    /// value outside its own choice list.
    fallback_prefix: String,
}

impl CommandBuild {
    /// A builder that is `${prefix}${value}` for every value.
    pub fn prefix(prefix: impl Into<String>) -> Self {
        Self {
            by_value: BTreeMap::new(),
            fallback_prefix: prefix.into(),
        }
    }

    /// A builder with its own answer for some values.
    pub fn table(
        prefix: impl Into<String>,
        by_value: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self {
            by_value: by_value.into_iter().collect(),
            fallback_prefix: prefix.into(),
        }
    }

    /// `build(value)`.
    pub fn build(&self, value: &str) -> String {
        match self.by_value.get(value) {
            Some(command) => command.clone(),
            None => format!("{}{value}", self.fallback_prefix),
        }
    }
}

/// How an option reaches the agent.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionDispatch {
    /// Types `build(value)` into the TUI; the chosen value becomes the local truth.
    Command(CommandBuild),
    /// Types a filtered picker command, then confirms its sole matching row.
    CommandConfirmPicker(CommandBuild),
    /// Types a fixed toggle command while tracking its optimistic target.
    ToggleCommand { command: String },
    /// Types a command that opens the agent's own picker, then shows the terminal.
    AgentPicker { command: String },
    /// CDXC:AgentScreenDetection 2026-09-03 WHY:
    /// Codex model changes drive its own `/model` picker on the daemon
    /// (`/api/selectSessionChatModel`), because `/model <name>` is not a command there. Normal
    /// effort-only changes use shifted arrows in the same serialized job; Max and Ultra go
    /// through the picker's More reasoning section. The model rows and the effort rows share this
    /// kind: a model row keeps the current effort when the new model offers it, an effort row
    /// keeps the current model. Hosts without the endpoint fall back to the `agent-picker`
    /// behaviour (type `/model`, show the terminal).
    ModelPicker,
    /// Nothing is typed: the pill shows the value gxserver read from the agent's statusline and
    /// hands the user to the terminal to change it.
    TerminalHandoff,
    /// Steps through a bounded TUI setting using shifted arrow keys.
    BoundedKeySteps {
        decrease_key: String,
        increase_key: String,
    },
    /// Cycles forward through a fixed ordered setting using one repeated key.
    CyclicKeySteps { key: String },
    /// Writes a raw keystroke sequence (no text, no Enter).
    Key { key: String, marker: String },
}

impl OptionDispatch {
    /// The `kind` string the document carries.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Command(_) => "command",
            Self::CommandConfirmPicker(_) => "command-confirm-picker",
            Self::ToggleCommand { .. } => "toggle-command",
            Self::AgentPicker { .. } => "agent-picker",
            Self::ModelPicker => "model-picker",
            Self::TerminalHandoff => "terminal-handoff",
            Self::BoundedKeySteps { .. } => "bounded-key-steps",
            Self::CyclicKeySteps { .. } => "cyclic-key-steps",
            Self::Key { .. } => "key",
        }
    }
}

impl Serialize for OptionDispatch {
    /// The shape `JSON.stringify` writes: `kind` plus the data fields, with the closures dropped.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.kind())?;
        match self {
            Self::ToggleCommand { command } | Self::AgentPicker { command } => {
                map.serialize_entry("command", command)?;
            }
            Self::BoundedKeySteps {
                decrease_key,
                increase_key,
            } => {
                map.serialize_entry("decreaseKey", decrease_key)?;
                map.serialize_entry("increaseKey", increase_key)?;
            }
            Self::CyclicKeySteps { key } => map.serialize_entry("key", key)?,
            Self::Key { key, marker } => {
                map.serialize_entry("key", key)?;
                map.serialize_entry("marker", marker)?;
            }
            Self::Command(_)
            | Self::CommandConfirmPicker(_)
            | Self::ModelPicker
            | Self::TerminalHandoff => {}
        }
        map.end()
    }
}

/// One option the pills can show and the menu can change.
///
/// The field order below is the order every builder writes the object literal in, so the
/// serialized descriptor lines up with the TypeScript key for key.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionDescriptor {
    /// Stable per agent; also the persistence key.
    pub id: String,
    /// Category name, for example "Effort": shown in the tooltip, not in the pill.
    pub label: String,
    pub category: OptionCategory,
    /// Present for value-carrying (select) options only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<OptionChoice>>,
    /// Submenus the choices may name; order comes from `choices`, not from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice_groups: Option<Vec<OptionChoiceGroup>>,
    /// Row label for toggle, agent-picker and key rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_label: Option<String>,
    /// Tooltip for a `terminal-handoff` pill, replacing the generic one, when the CLI has a named
    /// command for the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_hint: Option<String>,
    /// Muted line under the menu heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    pub dispatch: OptionDispatch,
}

impl OptionDescriptor {
    fn new(id: &str, label: &str, category: OptionCategory, dispatch: OptionDispatch) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            category,
            choices: None,
            choice_groups: None,
            action_label: None,
            handoff_hint: None,
            description: None,
            default_value: None,
            dispatch,
        }
    }

    /// `descriptor.choices ?? []`.
    pub fn choice_list(&self) -> &[OptionChoice] {
        self.choices.as_deref().unwrap_or(&[])
    }
}

/// The option catalog for one agent.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionOptionCatalog {
    /// The model pill's descriptor.
    pub model: OptionDescriptor,
    /// Provider artwork shown beside the current model name.
    pub model_icon: String,
    options: CatalogOptions,
}

/// How `optionsForModel` answers, per agent family.
#[derive(Clone, Debug, PartialEq)]
enum CatalogOptions {
    Claude {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    Codex {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    Cursor {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    Grok {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    Antigravity {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    OpenCode { agent: Box<CatalogAgent>, catalog: Box<AgentModelCatalog> },
    /// Read-only mirrors: the same list whatever the model is.
    Fixed(Vec<OptionDescriptor>),
}

impl SessionOptionCatalog {
    /// `catalog.optionsForModel(modelValue)`: everything but the model, in category order, for the
    /// model currently believed to be running.
    pub fn options_for_model(&self, model_value: &str) -> Vec<OptionDescriptor> {
        match &self.options {
            CatalogOptions::Claude { agent, catalog } => {
                // Until gxserver confirms the model, do not offer effort controls that may not
                // exist for the actual model (the catalog gives Haiku none).
                let current = agent.model(model_value);
                let efforts = current
                    .map(|model| model.efforts.clone())
                    .unwrap_or_default();
                let mut descriptors = Vec::new();
                if !efforts.is_empty() {
                    descriptors.push(claude_effort(catalog, &efforts));
                }
                if agent.fast_mode.available && current.map(|model| model.fast_mode) == Some(true) {
                    descriptors.push(fast_mode_descriptor(agent));
                }
                descriptors.push(claude_mode());
                sort_descriptors(descriptors)
            }
            CatalogOptions::Codex { agent, catalog } => {
                let current = agent.model(model_value);
                let mut descriptors =
                    vec![codex_effort(catalog, &agent.efforts_for_model(model_value))];
                if agent.fast_mode.available && current.is_none_or(|model| model.fast_mode) {
                    descriptors.push(fast_mode_descriptor(agent));
                }
                descriptors.push(codex_mode());
                sort_descriptors(descriptors)
            }
            CatalogOptions::Cursor { agent, catalog } => {
                let efforts = agent.efforts_for_model(model_value);
                if efforts.is_empty() {
                    Vec::new()
                } else {
                    vec![reasoning_effort_picker(catalog, &efforts)]
                }
            }
            CatalogOptions::Grok { agent, catalog } => vec![reasoning_effort_picker(
                catalog,
                &agent.efforts_for_model(model_value),
            )],
            CatalogOptions::Antigravity { agent, catalog } => {
                // The effort is typed together with the model, so it is only offered once
                // gxserver has confirmed which catalog model is running.
                match agent.model(model_value) {
                    Some(model) if !model.efforts.is_empty() => {
                        vec![antigravity_effort(agent, catalog, model)]
                    }
                    _ => Vec::new(),
                }
            }
            CatalogOptions::OpenCode {agent, catalog} => {
                let mut options = Vec::new();
                let efforts = agent.efforts_for_model(model_value);
                if !efforts.is_empty() { options.push(reasoning_effort_picker(catalog, &efforts)); }
                let mut mode = OptionDescriptor::new("mode", MODES_SECTION_LABEL, OptionCategory::Mode, OptionDispatch::ModelPicker);
                mode.choices = Some(vec![OptionChoice {value:"build".into(), label:"Build".into(), ..Default::default()}, OptionChoice {value:"plan".into(), label:"Plan".into(), ..Default::default()}]);
                options.push(mode);
                options
            }
            CatalogOptions::Fixed(descriptors) => descriptors.clone(),
        }
    }

    /// `catalog.pickerEffortFor`: for `model-picker` catalogs, the effort a model change should
    /// keep or fall to. `None` when the catalog has no such rule.
    pub fn picker_effort_for(
        &self,
        model_value: &str,
        current_effort: Option<&str>,
    ) -> Option<String> {
        let CatalogOptions::Codex { agent, .. } = &self.options else {
            return None;
        };
        let efforts = agent.efforts_for_model(model_value);
        if let Some(current) = current_effort {
            if efforts.iter().any(|effort| effort == current) {
                return Some(current.to_string());
            }
        }
        agent
            .model(model_value)
            .and_then(|model| model.default_effort.clone())
            .or_else(|| efforts.first().cloned())
    }

    /// Whether this catalog has a `pickerEffortFor` at all, which `dispatchSessionChatOption`
    /// reads as `catalog.pickerEffortFor?.(...)`.
    pub fn has_picker_effort(&self) -> bool {
        matches!(self.options, CatalogOptions::Codex { .. })
    }

    /// Whether a model pick may leave the effort out and let the agent keep its own level.
    ///
    /// CDXC:SessionChat 2026-09-25 WHY:
    /// Claude Code keeps one saved effort level across model switches, including through a model
    /// with no levels (Haiku), and gxserver types no `/effort` for an empty effort. When the chat
    /// does not know a level the new model offers (none is detected on Haiku), a model pick sends
    /// no effort rather than the model's first level: falling back to the first level sent
    /// `/effort low` on the way back from Haiku to Sonnet and overwrote the Medium the user had
    /// picked. Codex's picker needs an effort with every model, so it keeps the fallback.
    pub fn agent_keeps_effort(&self) -> bool {
        matches!(self.options, CatalogOptions::Claude { .. })
    }
}

/// `sortDescriptors`: category order, stable within a category.
fn sort_descriptors(descriptors: Vec<OptionDescriptor>) -> Vec<OptionDescriptor> {
    let mut sorted = descriptors;
    sorted.sort_by_key(|descriptor| descriptor.category.order());
    sorted
}

/// `effortChoices`.
fn effort_choices(catalog: &AgentModelCatalog, efforts: &[String]) -> Vec<OptionChoice> {
    efforts
        .iter()
        .map(|value| OptionChoice {
            value: value.clone(),
            label: catalog.effort_label(value),
            ..OptionChoice::default()
        })
        .collect()
}

/// `modelChoices`.
fn model_choices(agent: &CatalogAgent) -> Vec<OptionChoice> {
    agent
        .models
        .iter()
        .map(|model| OptionChoice {
            value: model.value.clone(),
            label: model.label.clone(),
            picker_label: model.picker_label.clone(),
            description: model.description.clone(),
            group: model.group.clone(),
        })
        .collect()
}

fn model_choice_groups(agent: &CatalogAgent) -> Option<Vec<OptionChoiceGroup>> {
    if agent.groups.is_empty() {
        None
    } else {
        Some(agent.groups.iter().map(OptionChoiceGroup::from).collect())
    }
}

/// The shared `fastMode` descriptor: same shape for Claude and Codex.
fn fast_mode_descriptor(agent: &CatalogAgent) -> OptionDescriptor {
    let command = agent
        .fast_mode
        .command
        .clone()
        .unwrap_or_else(|| "/fast".to_string());
    let mut descriptor = OptionDescriptor::new(
        "fastMode",
        MODES_SECTION_LABEL,
        OptionCategory::ModelConfig,
        OptionDispatch::ToggleCommand { command },
    );
    descriptor.action_label = Some("Fast mode".to_string());
    descriptor
}

// ---------------------------------------------------------------------------
// Claude / OpenClaude
// ---------------------------------------------------------------------------

/// `CLAUDE_MODES`.
fn claude_modes() -> Vec<OptionChoice> {
    [
        ("bypass", "Bypass permissions"),
        ("auto", "Auto"),
        ("manual", "Manual"),
        ("accept-edits", "Accept edits"),
        ("plan", "Plan"),
    ]
    .into_iter()
    .map(|(value, label)| OptionChoice {
        value: value.to_string(),
        label: label.to_string(),
        ..OptionChoice::default()
    })
    .collect()
}

/// `CLAUDE_MODE`.
///
/// Permission mode is Shift+Tab in Claude Code's TUI. The terminal footer supplies the current
/// value, so selecting a target sends the exact forward distance in Claude's cyclic mode order.
fn claude_mode() -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "mode",
        "Mode",
        OptionCategory::Mode,
        OptionDispatch::CyclicKeySteps {
            key: "shift-tab".to_string(),
        },
    );
    descriptor.choices = Some(claude_modes());
    descriptor.description = Some("Select Claude Code's permission mode.".to_string());
    descriptor
}

fn claude_effort(catalog: &AgentModelCatalog, efforts: &[String]) -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "effort",
        "Effort",
        OptionCategory::ThoughtLevel,
        OptionDispatch::Command(CommandBuild::prefix("/effort ")),
    );
    descriptor.choices = Some(effort_choices(catalog, efforts));
    descriptor
}

fn build_claude_catalog(catalog: &AgentModelCatalog, agent: &CatalogAgent) -> SessionOptionCatalog {
    let mut model = OptionDescriptor::new(
        "model",
        "Model",
        OptionCategory::Model,
        OptionDispatch::Command(CommandBuild::prefix("/model ")),
    );
    model.choices = Some(model_choices(agent));
    model.choice_groups = model_choice_groups(agent);
    SessionOptionCatalog {
        model,
        model_icon: "claude".to_string(),
        options: CatalogOptions::Claude {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.clone()),
        },
    }
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// `CODEX_MODE`.
///
/// CDXC:AgentScreenDetection 2026-09-05 DECISION:
/// User: the Codex "Plan mode" checkbox updates optimistically; this supersedes waiting for the
/// footer before showing the selection. Codex's `/plan` enters Plan mode and Shift+Tab leaves it,
/// with the footer confirming the result afterward.
fn codex_mode() -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "mode",
        MODES_SECTION_LABEL,
        OptionCategory::Mode,
        OptionDispatch::ToggleCommand {
            command: "/plan".to_string(),
        },
    );
    descriptor.action_label = Some("Plan mode".to_string());
    descriptor
}

/// The `Reasoning effort` descriptor that goes through the daemon's model picker.
fn reasoning_effort_picker(catalog: &AgentModelCatalog, efforts: &[String]) -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "effort",
        "Reasoning effort",
        OptionCategory::ThoughtLevel,
        OptionDispatch::ModelPicker,
    );
    descriptor.choices = Some(effort_choices(catalog, efforts));
    descriptor
}

fn codex_effort(catalog: &AgentModelCatalog, efforts: &[String]) -> OptionDescriptor {
    reasoning_effort_picker(catalog, efforts)
}

/// CDXC:AgentScreenDetection 2026-09-05 DECISION:
/// User: choosing normal Codex effort in the dropdown sends Shift+Up/Down internally, without
/// adding composer shortcuts; Max and Ultra use `/model` and More reasoning instead. This
/// supersedes sending shifted arrows to every effort level. Both paths use the same daemon request
/// and read the live footer to confirm the selection.
fn build_codex_catalog(catalog: &AgentModelCatalog, agent: &CatalogAgent) -> SessionOptionCatalog {
    let mut model = OptionDescriptor::new(
        "model",
        "Model",
        OptionCategory::Model,
        OptionDispatch::ModelPicker,
    );
    model.choices = Some(model_choices(agent));
    model.choice_groups = model_choice_groups(agent);
    model.action_label = Some("Open the CLI's model picker".to_string());
    SessionOptionCatalog {
        model,
        model_icon: "codex".to_string(),
        options: CatalogOptions::Codex {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.clone()),
        },
    }
}

// ---------------------------------------------------------------------------
// Cursor Agent
// ---------------------------------------------------------------------------

fn build_opencode_catalog(catalog: &AgentModelCatalog, agent: &CatalogAgent) -> SessionOptionCatalog {
    let mut result = build_cursor_catalog(catalog, agent);
    result.model_icon = "opencode".into();
    result.model.dispatch = OptionDispatch::ModelPicker;
    result.options = CatalogOptions::OpenCode {agent: Box::new(agent.clone()), catalog: Box::new(catalog.clone())};
    result
}

fn build_cursor_catalog(catalog: &AgentModelCatalog, agent: &CatalogAgent) -> SessionOptionCatalog {
    let choices = model_choices(agent);
    // The picker filter needs the row's literal text ("Claude Opus 5").
    let table = choices.iter().map(|choice| {
        let filter = choice
            .picker_label
            .clone()
            .unwrap_or_else(|| choice.label.clone());
        (choice.value.clone(), format!("/model {filter}"))
    });
    let mut model = OptionDescriptor::new(
        "model",
        "Model",
        OptionCategory::Model,
        OptionDispatch::CommandConfirmPicker(CommandBuild::table("/model ", table)),
    );
    model.choices = Some(choices);
    model.choice_groups = model_choice_groups(agent);
    SessionOptionCatalog {
        model,
        model_icon: "cursor-cli".to_string(),
        options: CatalogOptions::Cursor {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.clone()),
        },
    }
}

// ---------------------------------------------------------------------------
// Grok
// ---------------------------------------------------------------------------

/// CDXC:SessionChat 2026-09-09 DECISION:
/// User: Grok Build lists and changes models and efforts directly in chat without switching to the
/// CLI. Its supported /model command replaces the read-only handoff.
fn build_grok_catalog(catalog: &AgentModelCatalog, agent: &CatalogAgent) -> SessionOptionCatalog {
    let mut model = OptionDescriptor::new(
        "model",
        "Model",
        OptionCategory::Model,
        OptionDispatch::ModelPicker,
    );
    model.choices = Some(model_choices(agent));
    model.choice_groups = model_choice_groups(agent);
    SessionOptionCatalog {
        model,
        model_icon: "grok-build".to_string(),
        options: CatalogOptions::Grok {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.clone()),
        },
    }
}

// ---------------------------------------------------------------------------
// Antigravity CLI
// ---------------------------------------------------------------------------

/// `antigravityModelCommand`.
///
/// Antigravity's `/model` takes one flattened id per model and effort (`gemini-3.8-flash-high`),
/// and rejects the bare model id when the model has efforts. The catalog keys rows by the model
/// part, so the model pill appends the model's default effort and the effort pill re-sends the
/// current model with the chosen effort. Models without efforts are typed as-is.
fn antigravity_model_command(
    agent: &CatalogAgent,
    model_value: &str,
    effort: Option<&str>,
) -> String {
    let model = agent.model(model_value);
    let suffix = effort
        .map(str::to_string)
        .or_else(|| model.and_then(|model| model.default_effort.clone()))
        .or_else(|| model.and_then(|model| model.efforts.first().cloned()));
    match (model, suffix) {
        (Some(model), Some(suffix)) if !model.efforts.is_empty() => {
            format!("/model {model_value}-{suffix}")
        }
        _ => format!("/model {model_value}"),
    }
}

fn antigravity_effort(
    agent: &CatalogAgent,
    catalog: &AgentModelCatalog,
    model: &CatalogModel,
) -> OptionDescriptor {
    let table = model.efforts.iter().map(|effort| {
        (
            effort.clone(),
            antigravity_model_command(agent, &model.value, Some(effort)),
        )
    });
    let mut descriptor = OptionDescriptor::new(
        "effort",
        "Effort",
        OptionCategory::ThoughtLevel,
        OptionDispatch::Command(CommandBuild::table(
            format!("/model {}-", model.value),
            table,
        )),
    );
    descriptor.choices = Some(effort_choices(catalog, &model.efforts));
    descriptor
}

fn build_antigravity_catalog(
    catalog: &AgentModelCatalog,
    agent: &CatalogAgent,
) -> SessionOptionCatalog {
    let table = agent.models.iter().map(|model| {
        (
            model.value.clone(),
            antigravity_model_command(agent, &model.value, None),
        )
    });
    let mut model = OptionDescriptor::new(
        "model",
        "Model",
        OptionCategory::Model,
        OptionDispatch::Command(CommandBuild::table("/model ", table)),
    );
    model.choices = Some(model_choices(agent));
    model.choice_groups = model_choice_groups(agent);
    SessionOptionCatalog {
        model,
        model_icon: "antigravity-cli".to_string(),
        options: CatalogOptions::Antigravity {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.clone()),
        },
    }
}

// ---------------------------------------------------------------------------
// Read-only mirrors
// ---------------------------------------------------------------------------

/// A `terminal-handoff` descriptor: the pill mirrors the statusline and the CLI owns the change.
fn handoff(
    id: &str,
    label: &str,
    category: OptionCategory,
    hint: Option<&str>,
) -> OptionDescriptor {
    let mut descriptor =
        OptionDescriptor::new(id, label, category, OptionDispatch::TerminalHandoff);
    descriptor.action_label = Some("Change it in the CLI".to_string());
    descriptor.handoff_hint = hint.map(str::to_string);
    descriptor
}

/// Hermes names its model in the leading segment of its terminal statusline and owns model
/// selection in its interactive /model picker, so the pill is the same read-only terminal mirror
/// used for Grok and Pi. Its statusline never names a reasoning effort, so there is no effort pill.
fn hermes_catalog() -> SessionOptionCatalog {
    SessionOptionCatalog {
        model: handoff(
            "model",
            "Model",
            OptionCategory::Model,
            Some("Change the model in the CLI by using /model"),
        ),
        model_icon: "hermes-agent".to_string(),
        options: CatalogOptions::Fixed(Vec::new()),
    }
}

/// Pi reports both values in its terminal statusline. Its model list is provider-dependent, and
/// model and effort changes belong to the CLI, so these are read-only mirrors.
fn pi_catalog() -> SessionOptionCatalog {
    SessionOptionCatalog {
        model: handoff("model", "Model", OptionCategory::Model, None),
        model_icon: "pi".to_string(),
        options: CatalogOptions::Fixed(vec![handoff(
            "effort",
            "Reasoning effort",
            OptionCategory::ThoughtLevel,
            None,
        )]),
    }
}

fn omp_catalog() -> SessionOptionCatalog {
    SessionOptionCatalog {
        model: handoff("model", "Model", OptionCategory::Model, None),
        model_icon: "omp".to_string(),
        options: CatalogOptions::Fixed(vec![handoff(
            "effort",
            "Reasoning effort",
            OptionCategory::ThoughtLevel,
            None,
        )]),
    }
}

// ---------------------------------------------------------------------------
// Catalog resolution
// ---------------------------------------------------------------------------

/// `sessionChatSessionOptionCatalog(agent)`: the option catalog for an agent, built from the agent
/// model catalog in effect right now.
pub fn session_option_catalog(
    catalog: &AgentModelCatalog,
    agent_id: Option<&str>,
) -> Option<SessionOptionCatalog> {
    let agent_id = agent_id?;
    // Both the transcript family id (read state) and the sidebar agent id reach this lookup, so
    // the catalog answers to either spelling.
    let (builder_id, build): (
        &str,
        fn(&AgentModelCatalog, &CatalogAgent) -> SessionOptionCatalog,
    ) = match agent_id {
        "claude" | "openclaude" => ("claude", build_claude_catalog),
        "opencode" => ("opencode", build_opencode_catalog),
        "codex" => ("codex", build_codex_catalog),
        "cursor" | "cursor-cli" => ("cursor", build_cursor_catalog),
        "grok" | "grok-build" => ("grok", build_grok_catalog),
        "antigravity" | "antigravity-cli" => ("antigravity", build_antigravity_catalog),
        "hermes" | "hermes-agent" => return Some(hermes_catalog()),
        "omp" => return Some(omp_catalog()),
        "pi" => return Some(pi_catalog()),
        _ => return None,
    };
    let agent = catalog.agents.get(builder_id)?;
    Some(build(catalog, agent))
}

/// `sessionChatOptionCommandNames`: command names the option pills can type, so a dispatched pill
/// command renders as the same muted "Ran /model sonnet" row a typed command gets. Names only (no
/// slash), matching the slash-command catalog.
pub fn session_option_command_names(
    catalog: &AgentModelCatalog,
    agent_id: Option<&str>,
) -> Vec<String> {
    let Some(catalog) = session_option_catalog(catalog, agent_id) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    let mut collect = |descriptor: &OptionDescriptor| {
        let command = match &descriptor.dispatch {
            OptionDispatch::Command(build) | OptionDispatch::CommandConfirmPicker(build) => {
                let first = descriptor
                    .choice_list()
                    .first()
                    .map_or("", |choice| choice.value.as_str());
                Some(build.build(first))
            }
            OptionDispatch::ToggleCommand { command } | OptionDispatch::AgentPicker { command } => {
                Some(command.clone())
            }
            OptionDispatch::ModelPicker => Some("/model".to_string()),
            _ => None,
        };
        let Some(command) = command else { return };
        // `split(/\s+/, 1)[0]` after a trim, which `split_whitespace` already does.
        let head = command.split_whitespace().next().unwrap_or("");
        // `replace(/^\//, '')` strips exactly one leading slash.
        let name = head.strip_prefix('/').unwrap_or(head).to_string();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    };
    collect(&catalog.model);
    // Union over every model, so a name only reachable under one model (Claude's /fast) still
    // classifies as a command.
    // `catalog.model.choices ?? [{ value: '', label: '' }]`: an empty but present list iterates
    // nothing, only an absent one falls back to the blank model.
    let values: Vec<String> = match &catalog.model.choices {
        Some(choices) => choices.iter().map(|choice| choice.value.clone()).collect(),
        None => vec![String::new()],
    };
    for value in values {
        for descriptor in catalog.options_for_model(&value) {
            collect(&descriptor);
        }
    }
    names
}
