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
///
/// Each family's `catalog` is read only for its effort labels, so it holds
/// [`AgentModelCatalog::effort_labels_only`]: a copy of every agent's lineup per built catalog was
/// most of what a document cost, because a menu builds one catalog per provider.
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
    OpenCode {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    Hermes {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
    /// Pi, OMP and Empryo: the lineup their own CLI lists, which gxserver sends with the session.
    PiFamily {
        agent: Box<CatalogAgent>,
        catalog: Box<AgentModelCatalog>,
    },
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
                descriptors.push(codex_permissions());
                sort_descriptors(descriptors)
            }
            CatalogOptions::Cursor { agent, catalog } => {
                let efforts = agent.efforts_for_model(model_value);
                let mut descriptors = Vec::new();
                if !efforts.is_empty() {
                    descriptors.push(reasoning_effort_picker(catalog, &efforts));
                }
                descriptors.push(cursor_mode());
                sort_descriptors(descriptors)
            }
            CatalogOptions::Grok { agent, catalog } => sort_descriptors(vec![
                reasoning_effort_picker(catalog, &agent.efforts_for_model(model_value)),
                grok_mode(),
            ]),
            CatalogOptions::Antigravity { agent, catalog } => {
                // The effort is typed together with the model, so it is only offered once
                // gxserver has confirmed which catalog model is running.
                let mut descriptors = match agent.model(model_value) {
                    Some(model) if !model.efforts.is_empty() => {
                        vec![antigravity_effort(agent, catalog, model)]
                    }
                    _ => Vec::new(),
                };
                descriptors.push(antigravity_mode());
                sort_descriptors(descriptors)
            }
            CatalogOptions::OpenCode { agent, catalog } => {
                let mut options = Vec::new();
                let efforts = agent.efforts_for_model(model_value);
                if !efforts.is_empty() {
                    options.push(reasoning_effort_picker(catalog, &efforts));
                }
                let mut mode = OptionDescriptor::new(
                    "mode",
                    MODES_SECTION_LABEL,
                    OptionCategory::Mode,
                    OptionDispatch::ModelPicker,
                );
                mode.choices = Some(vec![
                    OptionChoice {
                        value: "build".into(),
                        label: "Build".into(),
                        ..Default::default()
                    },
                    OptionChoice {
                        value: "plan".into(),
                        label: "Plan".into(),
                        ..Default::default()
                    },
                ]);
                options.push(mode);
                options
            }
            CatalogOptions::Hermes { agent, catalog } => {
                let mut effort =
                    reasoning_effort_picker(catalog, &agent.efforts_for_model(model_value));
                effort.default_value = agent.default_effort.clone();
                vec![effort]
            }
            CatalogOptions::PiFamily { agent, catalog } => {
                // A model without reasoning has no level to pick.
                let efforts = agent.efforts_for_model(model_value);
                if efforts.is_empty() {
                    Vec::new()
                } else {
                    vec![reasoning_effort_picker(catalog, &efforts)]
                }
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
    ///
    /// Pi, OMP and Empryo clamp their current level to the nearest one the new model supports, so a
    /// model pick whose level the new model lacks sends none and lets the CLI clamp, instead of
    /// falling to the model's lowest level (`off`).
    pub fn agent_keeps_effort(&self) -> bool {
        matches!(
            self.options,
            CatalogOptions::Claude { .. } | CatalogOptions::PiFamily { .. }
        )
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
            catalog: Box::new(catalog.effort_labels_only()),
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

/// The id of Codex's permission-mode descriptor, which the composer's Mode button owns.
pub const CODEX_PERMISSIONS_ID: &str = "permissions";

/// CDXC:SessionChat 2026-10-27 DECISION:
/// User: Codex sessions get the same Mode button Claude sessions have ("we already have this for claude, we need to have same thing for codex"), asked for by a Codex tester who wants to pick an approval mode instead of accepting every command. The rows are the modes Codex 0.164's own `/permissions` menu lists, in its order and with its names; its descriptions are shortened to one menu line (Approve for me keeps Codex's own); gxserver types `/permissions <profile>` into the session and reads the current mode back from what Codex prints.
/// SEE-ALSO: `CODEX_PERMISSION_MODES` and `codex_permissions_on_screen` in server/src/session_chat_options/agent_matchers.rs, `drive_permissions_option` in server/src/session_chat_codex_permissions.rs.
fn codex_permissions() -> OptionDescriptor {
    let choices = [
        (
            "ask-for-approval",
            "Ask for approval",
            ":ask-for-approval",
            "Asks before internet access or edits outside the workspace",
        ),
        (
            "approve-for-me",
            "Approve for me",
            ":approve-for-me",
            "Only ask for actions detected as potentially unsafe",
        ),
        (
            "full-access",
            "Full Access",
            ":danger-full-access",
            "Edits anywhere and uses the internet without asking",
        ),
        (
            "read-only",
            "Read Only",
            ":read-only",
            "Reads workspace files; asks before edits or internet access",
        ),
    ];
    let table = choices
        .iter()
        .map(|(value, _, profile, _)| (value.to_string(), format!("/permissions {profile}")));
    let mut descriptor = OptionDescriptor::new(
        CODEX_PERMISSIONS_ID,
        "Mode",
        OptionCategory::Mode,
        OptionDispatch::Command(CommandBuild::table("/permissions ", table)),
    );
    descriptor.choices = Some(
        choices
            .into_iter()
            .map(|(value, label, _, description)| OptionChoice {
                value: value.to_string(),
                label: label.to_string(),
                description: Some(description.to_string()),
                ..OptionChoice::default()
            })
            .collect(),
    );
    descriptor.description = Some("Choose what Codex is allowed to do.".to_string());
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
            catalog: Box::new(catalog.effort_labels_only()),
        },
    }
}

// ---------------------------------------------------------------------------
// Cursor Agent
// ---------------------------------------------------------------------------

fn build_opencode_catalog(
    catalog: &AgentModelCatalog,
    agent: &CatalogAgent,
) -> SessionOptionCatalog {
    let mut result = build_cursor_catalog(catalog, agent);
    result.model_icon = "opencode".into();
    result.model.dispatch = OptionDispatch::ModelPicker;
    result.options = CatalogOptions::OpenCode {
        agent: Box::new(agent.clone()),
        catalog: Box::new(catalog.effort_labels_only()),
    };
    result
}

/// CDXC:AgentProviders 2026-11-02 DECISION:
/// User (issue #390, "fix this too"): OpenCode shows as a tab in the model pill of every thread, a new draft and a running thread of another agent alike, with its real models, and a pick hands the thread to OpenCode as every other agent's tab does. gxserver serves the lineup OpenCode itself lists (`server/src/agent_model_lineups.rs`), and an OpenCode thread's own lineup for its folder replaces it (`menus::lifecycle::adopt_session_lineup`). Until either has arrived (a gxserver that predates it, or one whose OpenCode never answered) the tab lists OpenCode's own free model, which every OpenCode can run without a provider set up, so the tab is never missing.
/// SEE-ALSO: packages/gx-chat-core/src/menus/picker/settle.rs (`deliver_draft_handoff_model`).
fn opencode_fallback_agent() -> CatalogAgent {
    CatalogAgent {
        name: "OpenCode".to_string(),
        models: vec![CatalogModel {
            value: "opencode/big-pickle".to_string(),
            label: "Big Pickle".to_string(),
            ..CatalogModel::default()
        }],
        ..CatalogAgent::default()
    }
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// Cursor CLI has no command that returns to Agent mode (`/plan` only enters Plan, `/ask` and
/// `/debug` toggle), so the Mode pill presses Shift+Tab, which cycles Agent → Plan → Debug → Ask in
/// that order (2026.10.16). gxserver reads the current mode from the row Cursor prints above its
/// footer (`match_cursor_mode_line`) and refuses the key while a command approval is open, where
/// Shift+Tab means "Run Everything".
fn cursor_mode() -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "mode",
        "Mode",
        OptionCategory::Mode,
        OptionDispatch::CyclicKeySteps {
            key: "shift-tab".to_string(),
        },
    );
    descriptor.choices = Some(
        [
            ("default", "Agent"),
            ("plan", "Plan"),
            ("debug", "Debug"),
            ("search", "Ask"),
        ]
        .into_iter()
        .map(|(value, label)| OptionChoice {
            value: value.to_string(),
            label: label.to_string(),
            ..OptionChoice::default()
        })
        .collect(),
    );
    descriptor.description = Some("Select Cursor's mode.".to_string());
    descriptor
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
            catalog: Box::new(catalog.effort_labels_only()),
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
            catalog: Box::new(catalog.effort_labels_only()),
        },
    }
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// Grok Build's mode is its Shift+Tab cycle, in this order (1.0.62), and gxserver reads the current one off the composer border (`grok_mode_choice` in `server/src/session_chat_options/agent_matchers.rs`), so a pick presses Shift+Tab the forward distance to it. The chat offered no mode for Grok, which left a session started with always-approve unable to ask again without the terminal.
fn grok_mode() -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "mode",
        "Mode",
        OptionCategory::Mode,
        OptionDispatch::CyclicKeySteps {
            key: "shift-tab".to_string(),
        },
    );
    descriptor.choices = Some(
        [
            ("default", "Default"),
            ("plan", "Plan"),
            ("auto-review", "Auto review"),
            ("always-approve", "Always approve"),
        ]
        .into_iter()
        .map(|(value, label)| OptionChoice {
            value: value.to_string(),
            label: label.to_string(),
            ..OptionChoice::default()
        })
        .collect(),
    );
    descriptor.description = Some("Select Grok Build's mode.".to_string());
    descriptor
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

/// Shift+Tab cycles agy's mode in this order (Antigravity CLI 1.4.0) and the footer names the
/// current one, which gxserver reports with these values (`match_antigravity_statusline` in
/// `server/src/session_chat_options/agent_matchers.rs`). A pick is queued to gxserver, which presses
/// Shift+Tab one step at a time and reads the footer back until agy is in the chosen mode, so the
/// order here only sets the menu's order.
fn antigravity_mode() -> OptionDescriptor {
    let mut descriptor = OptionDescriptor::new(
        "mode",
        "Mode",
        OptionCategory::Mode,
        OptionDispatch::CyclicKeySteps {
            key: "shift-tab".to_string(),
        },
    );
    descriptor.choices = Some(
        [
            ("default", "Default"),
            ("accept-edits", "Accept edits"),
            ("auto", "Auto"),
            ("plan", "Plan"),
        ]
        .into_iter()
        .map(|(value, label)| OptionChoice {
            value: value.to_string(),
            label: label.to_string(),
            ..OptionChoice::default()
        })
        .collect(),
    );
    descriptor.description = Some("Select Antigravity CLI's mode.".to_string());
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
            catalog: Box::new(catalog.effort_labels_only()),
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

/// A Hermes profile's own lineup, which gxserver reads from the profile and sends with the
/// session (`modelCatalog`, `session_chat_hermes_status.rs`): the default model, then the models
/// its sessions used. A pick goes through the daemon's queue like every other model provider's, and
/// gxserver types `/model <name> --reasoning <level>`. The effort starts on the level gxserver
/// sends as the default (the session's starting level, else the profile's), because Hermes never
/// shows the one it runs.
fn build_hermes_catalog(catalog: &AgentModelCatalog, agent: &CatalogAgent) -> SessionOptionCatalog {
    let mut model = OptionDescriptor::new(
        "model",
        "Model",
        OptionCategory::Model,
        OptionDispatch::ModelPicker,
    );
    model.choices = Some(model_choices(agent));
    SessionOptionCatalog {
        model,
        model_icon: "hermes-agent".to_string(),
        options: CatalogOptions::Hermes {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.effort_labels_only()),
        },
    }
}

/// Until gxserver has read the profile's lineup (the hooks have not recorded the session yet), the
/// pill mirrors the model Hermes names in its statusline and hands the change to the terminal.
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

/// CDXC:AgentProviders 2026-10-17 WHY:
/// Copilot CLI's model lineup depends on the account's plan (a Copilot Free account is offered only
/// Auto, and `/model <id>` refuses the rest), and nothing outside the CLI lists it, so the model pill
/// shows the model Copilot's footer reports and hands the change to the CLI's `/model`. The mode is
/// Copilot's Shift+Tab cycle, Interactive → Plan → Autopilot, in that order (1.0.98), whose current
/// value the footer also names (`session_chat_copilot_status.rs` in gxserver).
fn copilot_catalog() -> SessionOptionCatalog {
    let mut mode = OptionDescriptor::new(
        "mode",
        "Mode",
        OptionCategory::Mode,
        OptionDispatch::CyclicKeySteps {
            key: "shift-tab".to_string(),
        },
    );
    mode.choices = Some(
        [
            ("interactive", "Interactive"),
            ("plan", "Plan"),
            ("autopilot", "Autopilot"),
        ]
        .into_iter()
        .map(|(value, label)| OptionChoice {
            value: value.to_string(),
            label: label.to_string(),
            ..OptionChoice::default()
        })
        .collect(),
    );
    mode.description = Some("Select Copilot's mode.".to_string());
    SessionOptionCatalog {
        model: handoff(
            "model",
            "Model",
            OptionCategory::Model,
            Some("Change the model in the CLI by using /model"),
        ),
        model_icon: "copilot".to_string(),
        options: CatalogOptions::Fixed(vec![mode]),
    }
}

/// CDXC:AgentProviders 2026-09-30 DECISION:
/// User: "set up for me the model selector for these 3 in chat view" (OpenCode, Pi and OMP, each
/// logged in with their Codex subscription). Pi and OMP list every model their own CLI can reach
/// (`session_chat_pi_models.rs` asks the CLI), grouped by provider, with the reasoning levels each
/// model supports. A pick types Pi's `/model <provider>/<id>` and `/thinking <level>`, or OMP's
/// `/switch <provider>/<id>:<level>`, which both apply to the session alone.
/// SEE-ALSO: server/src/session_chat_pi_models.rs builds the lineup,
/// server/src/session_chat_provider_model_picker.rs types the pick.
fn build_pi_family_catalog(
    catalog: &AgentModelCatalog,
    agent: &CatalogAgent,
    icon: &str,
) -> SessionOptionCatalog {
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
        model_icon: icon.to_string(),
        options: CatalogOptions::PiFamily {
            agent: Box::new(agent.clone()),
            catalog: Box::new(catalog.effort_labels_only()),
        },
    }
}

/// Until gxserver has read the CLI's lineup, Pi's, OMP's and Empryo's pills mirror the model and
/// level their statusline reports and hand the change to the terminal.
fn cli_lineup_handoff_catalog(icon: &str) -> SessionOptionCatalog {
    SessionOptionCatalog {
        model: handoff("model", "Model", OptionCategory::Model, None),
        model_icon: icon.to_string(),
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
        "opencode" => {
            return Some(match catalog.agents.get("opencode") {
                Some(agent) => build_opencode_catalog(catalog, agent),
                None => build_opencode_catalog(catalog, &opencode_fallback_agent()),
            });
        }
        "codex" => ("codex", build_codex_catalog),
        "cursor" | "cursor-cli" => ("cursor", build_cursor_catalog),
        "grok" | "grok-build" => ("grok", build_grok_catalog),
        "antigravity" | "antigravity-cli" => ("antigravity", build_antigravity_catalog),
        "copilot" => return Some(copilot_catalog()),
        "hermes" | "hermes-agent" => {
            return Some(match catalog.agents.get("hermes") {
                Some(agent) => build_hermes_catalog(catalog, agent),
                None => hermes_catalog(),
            });
        }
        // CDXC:AgentProviders 2026-10-06 DECISION:
        // Sven (Empryo harness spec): "As a Ghostex user, I want to pick Empryo's model and effort in the chat, so that I switch lanes without typing slash commands", from "the models my Empryo actually has". Empryo's lineup is `empryo --list-models`, grouped by provider; a pick types Empryo's own `/models` and `/effort`.
        // SEE-ALSO: server/src/session_chat_pi_models.rs reads the lineups, server/src/session_chat_empryo_picker.rs types an Empryo pick.
        id @ ("pi" | "omp" | "empryo") => {
            return Some(match catalog.agents.get(id) {
                Some(agent) => build_pi_family_catalog(catalog, agent, id),
                None => cli_lineup_handoff_catalog(id),
            });
        }
        // CDXC:AgentProviders 2026-10-28 DECISION:
        // User: DeepSeek Harness's chat should "expose the models there". Its rows are the catalog's `provider/model` routes (the API-key route, then the account route) with DeepSeek's off, low, high and max levels; a pick types dsh-tui's `/model <provider>/<id>` and `/effort <level>` (server/src/session_chat_dsh_picker.rs). dsh-tui keeps its level across a model switch, like Pi, so the menu has Pi's shape.
        "dsh" | "dsh-tui" => {
            return Some(match catalog.agents.get("dsh") {
                Some(agent) => build_pi_family_catalog(catalog, agent, "dsh"),
                None => cli_lineup_handoff_catalog("dsh"),
            });
        }
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
