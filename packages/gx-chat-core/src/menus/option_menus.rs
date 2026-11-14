//! The three menus behind the composer pills: the model pill's, the options pill's, and the mode
//! pill's.
//!
//! Port of `packages/shared/session-chat-controller/native-option-menus.ts`.

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::menus::option_catalog::{OptionDescriptor, SessionOptionCatalog, CODEX_PERMISSIONS_ID};
use crate::menus::option_menu::{
    is_mode_button, option_menu_sections, option_rows, visible_options, OptionCaps, OptionRows,
};
use crate::menus::option_values::{ChoiceSection, OptionState};

/// One row of a native chat menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeChatMenuItem {
    pub id: String,
    /// Run the command and leave the menu open, for a row that toggles rather than chooses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_open: Option<bool>,
    /// Draw `checked` as the app's switch instead of a check mark.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkey_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<NativeChatMenuItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Map<String, Value>>,
}

impl NativeChatMenuItem {
    fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

/// The three lists the pills open.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOptionMenus {
    pub model: Vec<NativeChatMenuItem>,
    pub options: Vec<NativeChatMenuItem>,
    pub mode: Vec<NativeChatMenuItem>,
}

/// One draft agent the model pill can switch to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftAgent {
    pub agent_id: String,
    pub name: String,
    pub icon: Option<String>,
    /// The family a custom agent is built on: a project agent built on Claude still manages
    /// Claude accounts.
    pub base_agent_id: Option<String>,
}

impl DraftAgent {
    /// Reads one row of the folded `availableAgents` list.
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        Some(Self {
            agent_id: object.get("agentId")?.as_str()?.to_string(),
            name: object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            icon: object
                .get("icon")
                .and_then(Value::as_str)
                .map(str::to_string),
            base_agent_id: object
                .get("baseAgentId")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    /// Reads the whole folded list, or `None` when the session is not a draft.
    pub fn list(value: Option<&Value>) -> Option<Vec<Self>> {
        let entries = value?.as_array()?;
        Some(entries.iter().filter_map(Self::from_value).collect())
    }
}

/// What the menus need to know about the session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionMenuParams {
    pub can_pick_model: bool,
    pub working: bool,
    pub can_send_key: bool,
    /// A draft session's agent choices, or `None` once the draft is promoted.
    pub draft_agents: Option<Vec<DraftAgent>>,
    pub draft_agent_id: Option<String>,
    /// Whether the agent has a model provider, so model and effort picks go through the durable
    /// model selection and stay available while the agent works.
    pub has_provider: bool,
    /// Why the last selection was abandoned, shown above the rows that offered it.
    pub selection_error: Option<String>,
    /// The modes this session's agent cannot use.
    pub unavailable_modes: Vec<ghostex_gx_protocol::UnavailableMode>,
}

fn command(descriptor_id: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("type".to_string(), json!("selectOption"));
    map.insert("descriptorId".to_string(), json!(descriptor_id));
    map
}

/// `nativeOptionMenus`.
pub fn native_option_menus(
    catalog: Option<&SessionOptionCatalog>,
    state: &OptionState,
    option_descriptors: &[OptionDescriptor],
    params: &OptionMenuParams,
) -> NativeOptionMenus {
    let queued_controls = matches!(
        catalog.map(|catalog| catalog.model_icon.as_str()),
        Some("codex") | Some("claude") | Some("antigravity-cli")
    );
    let caps = OptionCaps {
        can_pick_model: params.can_pick_model,
        can_send_key: params.can_send_key,
        queued_controls,
    };
    let rows = |descriptor: &OptionDescriptor| -> Vec<NativeChatMenuItem> {
        let presentation = option_rows(descriptor, state, caps);
        let base = command(&descriptor.id);
        let disabled = params.working
            && !((params.has_provider && (descriptor.id == "model" || descriptor.id == "effort"))
                || (queued_controls
                    && matches!(
                        descriptor.id.as_str(),
                        "mode" | "fastMode" | CODEX_PERMISSIONS_ID
                    )));
        match presentation {
            OptionRows::Action { label } => {
                let mut item = NativeChatMenuItem::new(descriptor.id.clone());
                item.label = Some(label);
                item.disabled = Some(disabled);
                item.command = Some(base);
                vec![item]
            }
            OptionRows::Toggle {
                label,
                checked,
                value,
                disabled: row_disabled,
                exit_plan,
            } => {
                let mut map = base;
                map.insert("value".to_string(), json!(value));
                map.insert("exitPlan".to_string(), json!(exit_plan));
                let mut item = NativeChatMenuItem::new(descriptor.id.clone());
                item.label = Some(label);
                item.checked = Some(checked);
                item.disabled = Some(disabled || row_disabled);
                item.command = Some(map);
                vec![item]
            }
            OptionRows::Choices { sections, current } => sections
                .iter()
                .flat_map(|section| {
                    let choices: Vec<NativeChatMenuItem> = section
                        .choices()
                        .iter()
                        .map(|choice| {
                            let mut map = base.clone();
                            map.insert("value".to_string(), json!(choice.value));
                            let mut item = NativeChatMenuItem::new(format!(
                                "{}:{}",
                                descriptor.id, choice.value
                            ));
                            item.label = Some(choice.label.clone());
                            item.description = choice.description.clone();
                            let checked = current.as_deref() == Some(choice.value.as_str());
                            item.checked = Some(checked);
                            item.disabled = Some(disabled);
                            // CDXC:SessionChat 2026-10-31 DECISION:
                            // GitHub #374, user: a mode the session can't use is greyed out, with how to turn it on in the menu's own words. The mode it is in right now is never greyed: the agent has just shown it can be used.
                            // SEE-ALSO: server/src/session_chat_mode_availability.rs.
                            if descriptor.id == "mode" && !checked {
                                if let Some(mode) = params
                                    .unavailable_modes
                                    .iter()
                                    .find(|mode| mode.value == choice.value)
                                {
                                    item.disabled = Some(true);
                                    item.description = Some(mode.why.clone());
                                }
                            }
                            item.command = Some(map);
                            item
                        })
                        .collect();
                    match section {
                        ChoiceSection::Choices { .. } => choices,
                        ChoiceSection::Group { group, .. } => {
                            let mut item = NativeChatMenuItem::new(section.key().to_string());
                            item.label = Some(group.label.clone());
                            item.description = group.description.clone();
                            item.detail = section
                                .choices()
                                .iter()
                                .find(|choice| current.as_deref() == Some(choice.value.as_str()))
                                .map(|choice| choice.label.clone());
                            item.children = Some(choices);
                            vec![item]
                        }
                    }
                })
                .collect(),
        }
    };
    let mut model: Vec<NativeChatMenuItem> = Vec::new();
    if let Some(agents) = params
        .draft_agents
        .as_ref()
        .filter(|agents| !agents.is_empty())
    {
        let mut switcher = NativeChatMenuItem::new("agents");
        switcher.label = Some("Switch Agent CLI".to_string());
        switcher.children = Some(
            agents
                .iter()
                .map(|agent| {
                    let mut map = Map::new();
                    map.insert("type".to_string(), json!("switchDraftAgent"));
                    map.insert("agentId".to_string(), json!(agent.agent_id));
                    let mut item = NativeChatMenuItem::new(agent.agent_id.clone());
                    item.label = Some(agent.name.clone());
                    item.icon = agent.icon.clone();
                    item.checked =
                        Some(Some(agent.agent_id.as_str()) == params.draft_agent_id.as_deref());
                    item.command = Some(map);
                    item
                })
                .collect(),
        );
        model.push(switcher);
        let mut separator = NativeChatMenuItem::new("agents-separator");
        separator.separator = Some(true);
        model.push(separator);
    }
    let Some(catalog) = catalog else {
        /*
        CDXC:SessionChat 2026-10-04 DECISION:
        User: Freebuff's model dropdown listed no models; "least we can do is to say switch model in cli button for freebuff in the models dropdown, and when we click on that then we just switch to terminal view from chat view". Every agent whose models chat cannot list gets the row.
        */
        let mut terminal = NativeChatMenuItem::new("switchModelInCli");
        terminal.label = Some("Switch model in CLI".to_string());
        terminal.command = Some(Map::from_iter([(
            "type".to_string(),
            json!("switchToTerminal"),
        )]));
        model.push(terminal);
        return NativeOptionMenus {
            model,
            options: Vec::new(),
            mode: Vec::new(),
        };
    };
    // CDXC:SessionChat 2026-09-18 DECISION:
    // User: when a model choice cannot be applied, say so where it was chosen. A queued selection
    // retries quietly, so only an abandoned one reaches this row; the next choice replaces it.
    if let Some(error) = params
        .selection_error
        .as_deref()
        .filter(|error| !error.is_empty())
    {
        let mut heading = NativeChatMenuItem::new("model-error");
        heading.heading = Some(true);
        heading.label = Some("Not applied".to_string());
        heading.description = Some(error.to_string());
        model.push(heading);
        let mut separator = NativeChatMenuItem::new("model-error-separator");
        separator.separator = Some(true);
        model.push(separator);
    }
    let mut heading = NativeChatMenuItem::new("model-heading");
    heading.heading = Some(true);
    heading.label = Some(catalog.model.label.clone());
    heading.description = catalog.model.description.clone();
    model.push(heading);
    model.extend(rows(&catalog.model));
    let visible = visible_options(option_descriptors, caps);
    let mode = visible.iter().find(|descriptor| is_mode_button(descriptor));
    let others: Vec<OptionDescriptor> = visible
        .iter()
        .filter(|descriptor| !is_mode_button(descriptor))
        .cloned()
        .collect();
    let mut options: Vec<NativeChatMenuItem> = Vec::new();
    for (index, section) in option_menu_sections(&others).iter().enumerate() {
        if index > 0 {
            let mut separator = NativeChatMenuItem::new(format!("{}:separator", section.label));
            separator.separator = Some(true);
            options.push(separator);
        }
        let mut heading = NativeChatMenuItem::new(format!("{}:heading", section.label));
        heading.label = Some(section.label.clone());
        heading.description = section.description.clone();
        heading.heading = Some(true);
        options.push(heading);
        for descriptor in &section.descriptors {
            options.extend(rows(descriptor));
        }
    }
    let mode = match mode {
        None => Vec::new(),
        Some(mode) => {
            let mut heading = NativeChatMenuItem::new("mode-heading");
            heading.heading = Some(true);
            heading.label = Some(mode.label.clone());
            heading.description = mode.description.clone();
            let mut items = vec![heading];
            for item in rows(mode) {
                // Under a greyed mode a restart can bring, its switch (menus/mode_restart.rs).
                let restart = item.disabled.unwrap_or(false).then(|| {
                    params
                        .unavailable_modes
                        .iter()
                        .find(|unavailable| item.id == format!("mode:{}", unavailable.value))
                        .and_then(crate::menus::mode_restart::restart_row)
                });
                items.push(item);
                items.extend(restart.flatten());
            }
            items
        }
    };
    NativeOptionMenus {
        model,
        options,
        mode,
    }
}
