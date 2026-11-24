//! The "Add custom agent" form at the end of the Agents list: Works like (the searchable select
//! with each agent's logo; "Nothing (plain command)" is a plain custom command), Name, Command,
//! Permission mode, Cancel and Add agent. Picking a built-in fills the name and command while
//! they are empty or still the previous choice's. Existing agents are edited in their own row.
use super::super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, RowSpec, SELECT_WIDTH, card_inset, setting_row,
    settings_button, settings_select_described, settings_text_input, settings_textarea,
};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::model::{
    AgentButton, auto_permission_family, default_agent_by_icon, default_agents,
    permission_mode_options, supports_accept_all,
};
use super::select::DropdownOption;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use gpui_component::input::InputState;
use serde_json::json;

const NAME_ID: &str = "agent-editor-name";
const COMMAND_ID: &str = "agent-editor-command";

/// The draft the editor edits (`AgentConfigDraft`) and its inputs.
pub(super) struct AgentEditor {
    agent_id: Option<String>,
    /// A default agent's icon, or `custom`.
    icon: String,
    accept_all_mode: String,
    name: Entity<InputState>,
}

impl AgentsTab {
    /// Opens the form prefilled from `agent` (Duplicate as custom agent).
    pub(super) fn open_editor_from(
        &mut self,
        agent: &AgentButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let copy = AgentButton {
            agent_id: String::new(),
            name: format!("{} (copy)", agent.name),
            ..agent.clone()
        };
        self.open_editor(Some(copy), window, cx);
        if let Some(editor) = self.editor.as_mut() {
            editor.agent_id = None;
        }
    }

    /// Opens the form blank, or prefilled from `agent`.
    pub(super) fn open_editor(
        &mut self,
        agent: Option<AgentButton>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (agent_id, name, command, icon, mode) = match &agent {
            Some(agent) => (
                Some(agent.agent_id.clone()),
                agent.name.clone(),
                agent.command.clone().unwrap_or_default(),
                agent.icon.clone().unwrap_or_else(|| "custom".to_string()),
                agent
                    .accept_all_mode
                    .clone()
                    .unwrap_or_else(|| "inherit".to_string()),
            ),
            None => (
                None,
                String::new(),
                String::new(),
                "custom".to_string(),
                "inherit".to_string(),
            ),
        };
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Codex")
                .default_value(name)
        });
        // A fresh command buffer for this draft.
        self.fields.textareas.remove(COMMAND_ID);
        let command_id = SharedString::from(COMMAND_ID);
        FieldStates::textarea_state(
            self,
            &command_id,
            &command,
            Some("codex"),
            (1, 12),
            |_page: &mut Self, _text, _window, cx| cx.notify(),
            window,
            cx,
        );
        let name_subscription = cx.subscribe(
            &name_input,
            |_page: &mut Self, _, _: &gpui_component::input::InputEvent, cx| cx.notify(),
        );
        self.fields.subscriptions.push(name_subscription);
        name_input.update(cx, |input, cx| input.focus(window, cx));
        self.editor = Some(AgentEditor {
            agent_id,
            icon,
            accept_all_mode: mode,
            name: name_input,
        });
        self.close_dropdowns();
    }

    /// The Add custom agent form as Settings remembers it between visits (CDXC:Settings
    /// 2026-10-16 in settings_modal/restore.rs).
    pub(super) fn open_editor_form(&self, cx: &gpui::App) -> Option<serde_json::Value> {
        let editor = self.editor.as_ref()?;
        Some(json!({
            "kind": "customAgent",
            "icon": editor.icon,
            "acceptAllMode": editor.accept_all_mode,
            "name": editor.name.read(cx).value().to_string(),
            "command": self.editor_command(cx),
        }))
    }

    /// Reopens the form `open_editor_form` reported.
    pub(super) fn restore_editor(
        &mut self,
        form: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = |key: &str| {
            form.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        self.open_editor(None, window, cx);
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let icon = text("icon");
        if !icon.is_empty() {
            editor.icon = icon;
        }
        let mode = text("acceptAllMode");
        if !mode.is_empty() {
            editor.accept_all_mode = mode;
        }
        let name_input = editor.name.clone();
        let name = text("name");
        name_input.update(cx, |input, cx| input.set_value(name, window, cx));
        if let Some(state) = self.fields.textareas.get(COMMAND_ID) {
            let input = state.input.clone();
            let command = text("command");
            input.update(cx, |input, cx| input.set_value(command, window, cx));
        }
    }

    fn editor_command(&self, cx: &gpui::App) -> String {
        self.fields
            .textareas
            .get(COMMAND_ID)
            .map(|state| state.input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// `updateAgentType`.
    fn set_editor_type(&mut self, next: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let previous =
            default_agent_by_icon(Some(editor.icon.as_str()).filter(|icon| *icon != "custom"));
        let next_default =
            default_agent_by_icon(Some(next.as_str()).filter(|icon| *icon != "custom"));
        editor.icon = next;
        if editor.accept_all_mode == "auto" {
            let icon_for_check = (editor.icon != "custom").then_some(editor.icon.as_str());
            let resolved_id = editor
                .agent_id
                .as_deref()
                .or_else(|| next_default.map(|agent| agent.agent_id.as_str()))
                .unwrap_or_default();
            if auto_permission_family(resolved_id, icon_for_check).is_none() {
                editor.accept_all_mode = "inherit".to_string();
            }
        }
        let Some(next_default) = next_default else {
            cx.notify();
            return;
        };
        let name_input = editor.name.clone();
        let name = name_input.read(cx).value().to_string();
        if name.trim().is_empty() || previous.is_some_and(|previous| previous.name == name) {
            let value = next_default.name.clone();
            name_input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        let command = self.editor_command(cx);
        if (command.trim().is_empty()
            || previous.is_some_and(|previous| previous.command == command))
            && let Some(state) = self.fields.textareas.get(COMMAND_ID)
        {
            let input = state.input.clone();
            let value = next_default.command.clone();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let command = self.editor_command(cx);
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let name = editor.name.read(cx).value().trim().to_string();
        let command = command.trim().to_string();
        if name.is_empty() || command.is_empty() {
            return;
        }
        let mut message = json!({
            "acceptAllMode": editor.accept_all_mode,
            "command": command,
            "name": name,
            "type": "saveSidebarAgent",
        });
        if let Some(agent_id) = &editor.agent_id {
            message["agentId"] = json!(agent_id);
        }
        if editor.icon != "custom" {
            message["icon"] = json!(editor.icon);
        }
        self.post(message, cx);
        self.editor = None;
        cx.notify();
    }

    pub(super) fn render_editor(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some((agent_id, icon, mode, name_input)) = self.editor.as_ref().map(|editor| {
            (
                editor.agent_id.clone(),
                editor.icon.clone(),
                editor.accept_all_mode.clone(),
                editor.name.clone(),
            )
        }) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        let mut type_options = vec![DropdownOption {
            value: "custom".to_string(),
            label: "Nothing (plain command)".to_string(),
            icon: Some("custom".to_string()),
        }];
        type_options.extend(default_agents().iter().map(|agent| DropdownOption {
            value: agent.icon.clone(),
            label: agent.name.clone(),
            icon: Some(agent.icon.clone()),
        }));
        let type_select = self.dropdown(
            p,
            "agent-editor-type",
            &type_options,
            Some(&icon),
            "",
            true,
            Some(SELECT_WIDTH),
            false,
            None,
            |page, value, window, cx| page.set_editor_type(value, window, cx),
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            "agent-editor-type-row",
            RowSpec::new("Works like").description(
                "Its logo, chat view, resume hook and permission handling. Picking a built-in agent that is off turns that agent back on instead.",
            ),
            None,
            type_select,
            cx,
        ));
        rows.push(setting_row(
            p,
            "agent-editor-name-row",
            RowSpec::new("Name"),
            None,
            div()
                .w(px(CONTROL_LANE_WIDTH))
                .max_w_full()
                .flex()
                .child(settings_text_input(p, &name_input, None, false, window, cx))
                .into_any_element(),
            cx,
        ));
        let command_state = self
            .fields
            .textareas
            .get(COMMAND_ID)
            .map(|state| state.input.clone());
        if let Some(command_state) = &command_state {
            rows.push(setting_row(
                p,
                "agent-editor-command-row",
                RowSpec::new("Command").wide(),
                None,
                settings_textarea(p, command_state, 64.0, false, false, window, cx),
                cx,
            ));
        }
        let icon_for_check = (icon != "custom").then_some(icon.as_str());
        let resolved_agent_id = agent_id
            .clone()
            .or_else(|| default_agent_by_icon(icon_for_check).map(|agent| agent.agent_id.clone()))
            .unwrap_or_default();
        let accept_supported = supports_accept_all(&resolved_agent_id, icon_for_check);
        let mode_select = settings_select_described(
            self,
            p,
            "agent-editor-approvals",
            &permission_mode_options(&resolved_agent_id, icon_for_check),
            &mode,
            Some(SELECT_WIDTH),
            !accept_supported,
            Some("This agent doesn’t support approval policy changes.".into()),
            |page: &mut Self, value, _window, cx| {
                if let Some(editor) = page.editor.as_mut() {
                    editor.accept_all_mode = value;
                }
                cx.notify();
            },
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            "agent-editor-approvals-row",
            RowSpec::new("Permission mode").description(if accept_supported {
                "Use app default follows the global Agents setting. Full permissions: the agent won't ask for permission before edits or commands. At your own risk."
            } else {
                "This agent does not expose a supported approval policy in Ghostex."
            }),
            None,
            mode_select,
            cx,
        ));
        let name_empty = name_input.read(cx).value().trim().is_empty();
        let command_empty = self.editor_command(cx).trim().is_empty();
        let save_reason = if name_empty && command_empty {
            "Enter a name and command first."
        } else if name_empty {
            "Enter an agent name first."
        } else {
            "Enter an agent command first."
        };
        rows.push(card_inset(
            h_flex()
                .w_full()
                .justify_end()
                .gap(px(12.0))
                .child(settings_button(
                    p,
                    "agent-editor-cancel",
                    "Cancel",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        page.editor = None;
                        cx.notify();
                    },
                    cx,
                ))
                .child(settings_button(
                    p,
                    "agent-editor-save",
                    "Add agent",
                    None,
                    ButtonVariant::Default,
                    name_empty || command_empty,
                    Some(save_reason.into()),
                    |page: &mut Self, _window, cx| page.save_editor(cx),
                    cx,
                )),
        ));
        rows
    }
}
