//! The expanded panel of an Agents row (`settings-list-panel`), in three groups: Start (Name,
//! Works like for custom agents, Command, Permission mode, Default interface for chat-capable
//! agents, CDXC:AgentProviders 2026-08-27), CLI (the agent CLI controls) and Session resume hook;
//! then Duplicate as custom agent, Reset to defaults (built-in agents) and Delete agent (custom
//! agents only: a built-in agent is turned off with its switch). Its rows sit inside the list
//! inset, so they drop their own side padding and divide with hairlines.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, ROW_PADDING_X, RowSpec, SELECT_WIDTH, setting_row,
    settings_button_sized, settings_list_item, settings_select,
};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::icons;
use super::model::{
    AgentButton, HookStatusItem, accept_all_mode_options, default_agents, hook_removable,
    inherit_value, preferred_interface_override_options, supports_accept_all, supports_chat_view,
};
use super::roster::hook_detail_icon;
use super::select::DropdownOption;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};

/// A setting row or list item inside the panel: the row's own 20px sides are given back.
fn panel_child(element: AnyElement) -> AnyElement {
    div()
        .mx(px(-ROW_PADDING_X))
        .child(element)
        .into_any_element()
}

/// The small heading of a panel group.
fn group_title(p: &SettingsPalette, title: &'static str) -> AnyElement {
    div()
        .pt(px(14.0))
        .pb(px(2.0))
        .text_size(px(11.5))
        .line_height(px(16.0))
        .text_color(hsla(css_fade(p.muted, 0.85)))
        .child(title.to_uppercase())
        .into_any_element()
}

/// Width of the Name and Command inputs.
const INPUT_WIDTH: f32 = 300.0;

impl AgentsTab {
    /// `saveAgent` for a changed permission mode: the agent as it is with the new mode.
    fn save_agent_mode(&mut self, agent: &AgentButton, mode: String, cx: &mut Context<Self>) {
        let mut message = json!({
            "acceptAllMode": mode,
            "agentId": agent.agent_id,
            "command": agent.command.clone().unwrap_or_default(),
            "name": agent.name,
            "type": "saveSidebarAgent",
        });
        if let Some(icon) = &agent.icon {
            message["icon"] = json!(icon);
        }
        self.post(message, cx);
        cx.notify();
    }

    /// Saves a changed name, command or "Works like" type, keeping the permission mode.
    fn save_agent_fields(
        &mut self,
        agent: &AgentButton,
        name: Option<String>,
        command: Option<String>,
        icon: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut message = json!({
            "agentId": agent.agent_id,
            "command": command.or_else(|| agent.command.clone()).unwrap_or_default(),
            "name": name.unwrap_or_else(|| agent.name.clone()),
            "type": "saveSidebarAgent",
        });
        if let Some(icon) = icon.or_else(|| agent.icon.clone()) {
            message["icon"] = json!(icon);
        }
        self.post(message, cx);
        cx.notify();
    }

    /// Reset to defaults: a built-in agent's own name and command, and the app's approvals.
    fn reset_agent(&mut self, agent: &AgentButton, cx: &mut Context<Self>) {
        let (Some(name), Some(command)) = (&agent.default_name, &agent.default_command) else {
            return;
        };
        let mut message = json!({
            "acceptAllMode": "inherit",
            "agentId": agent.agent_id,
            "command": command,
            "name": name,
            "type": "saveSidebarAgent",
        });
        if let Some(icon) = &agent.icon {
            message["icon"] = json!(icon);
        }
        self.post(message, cx);
        cx.notify();
    }

    /// CDXC:AgentProviders 2026-08-27 (agents.tsx): Inherit is an absent key, never a stored
    /// third value, so an untouched agent keeps following the global Default Agent View.
    fn set_interface_override(&mut self, agent_id: &str, value: String, cx: &mut Context<Self>) {
        let mut overrides: Map<String, Value> = self
            .store
            .read(cx)
            .value("preferredAgentInterfaceOverrides")
            .as_object()
            .cloned()
            .unwrap_or_default();
        if value == inherit_value() {
            overrides.remove(agent_id);
        } else {
            overrides.insert(agent_id.to_string(), json!(value));
        }
        self.save(
            "preferredAgentInterfaceOverrides",
            Value::Object(overrides),
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_agent_panel(
        &mut self,
        p: &SettingsPalette,
        agent: &AgentButton,
        hook_agent: Option<&str>,
        hook_status: Option<&HookStatusItem>,
        pending: bool,
        cli_agent: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agent_id = agent.agent_id.clone();
        let loading = self.hook_status_loading;
        let mut children: Vec<AnyElement> = vec![group_title(p, "Start")];
        let name_agent = agent.clone();
        let name_input = self.inline_input(
            p,
            SharedString::from(format!("agent-name-{agent_id}")),
            &agent.name,
            "Name",
            INPUT_WIDTH,
            false,
            move |page: &mut Self, name, cx| {
                page.save_agent_fields(&name_agent, Some(name), None, None, cx)
            },
            window,
            cx,
        );
        children.push(panel_child(setting_row(
            p,
            SharedString::from(format!("agent-name-row-{agent_id}")),
            RowSpec::new("Name"),
            None,
            name_input,
            cx,
        )));
        if !agent.is_default {
            let current = agent.icon.clone().unwrap_or_else(|| "custom".to_string());
            let mut options: Vec<DropdownOption> = Vec::new();
            if current == "custom" {
                options.push(DropdownOption {
                    value: "custom".to_string(),
                    label: "Nothing (plain command)".to_string(),
                    icon: Some("custom".to_string()),
                });
            }
            options.extend(default_agents().iter().map(|default| DropdownOption {
                value: default.icon.clone(),
                label: default.name.clone(),
                icon: Some(default.icon.clone()),
            }));
            let works_agent = agent.clone();
            let works_like = self.dropdown(
                p,
                SharedString::from(format!("agent-works-like-{agent_id}")),
                &options,
                Some(&current),
                "",
                true,
                Some(SELECT_WIDTH),
                false,
                None,
                move |page, value, _window, cx| {
                    if value != "custom" {
                        page.save_agent_fields(&works_agent, None, None, Some(value), cx);
                    }
                },
                window,
                cx,
            );
            children.push(panel_child(setting_row(
                p,
                SharedString::from(format!("agent-works-like-row-{agent_id}")),
                RowSpec::new("Works like").description(
                    "Gives it that agent's logo, chat view, resume hook and permission handling.",
                ),
                None,
                works_like,
                cx,
            )));
        }
        let command_agent = agent.clone();
        let command_input = self.inline_input(
            p,
            SharedString::from(format!("agent-command-{agent_id}")),
            agent.command.as_deref().unwrap_or_default(),
            "Command",
            INPUT_WIDTH,
            true,
            move |page: &mut Self, command, cx| {
                page.save_agent_fields(&command_agent, None, Some(command), None, cx)
            },
            window,
            cx,
        );
        children.push(panel_child(setting_row(
            p,
            SharedString::from(format!("agent-command-row-{agent_id}")),
            RowSpec::new("Command").description("What Ghostex runs for a new session."),
            None,
            command_input,
            cx,
        )));
        let accept_supported = supports_accept_all(&agent.agent_id, agent.icon.as_deref());
        let mode = agent
            .accept_all_mode
            .clone()
            .unwrap_or_else(|| "inherit".to_string());
        let mode_agent = agent.clone();
        let mode_select = settings_select(
            self,
            p,
            SharedString::from(format!("agent-permission-{agent_id}")),
            &accept_all_mode_options(),
            &mode,
            Some(SELECT_WIDTH),
            !accept_supported,
            Some("This agent doesn’t support approval policy changes.".into()),
            move |page: &mut Self, value, _window, cx| page.save_agent_mode(&mode_agent, value, cx),
            window,
            cx,
        );
        children.push(panel_child(setting_row(
            p,
            SharedString::from(format!("agent-permission-row-{agent_id}")),
            RowSpec::new("Permission mode")
                .description("How the agent handles approvals when Ghostex starts it."),
            None,
            mode_select,
            cx,
        )));
        if supports_chat_view(&agent.agent_id, agent.icon.as_deref()) {
            let (global, overrides) = {
                let store = self.store.read(cx);
                (
                    store.string("preferredAgentInterface"),
                    store.value("preferredAgentInterfaceOverrides"),
                )
            };
            let value = overrides
                .get(&agent_id)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(inherit_value);
            let interface_agent = agent_id.clone();
            let select = settings_select(
                self,
                p,
                SharedString::from(format!("agent-interface-{agent_id}")),
                &preferred_interface_override_options(&global),
                &value,
                Some(SELECT_WIDTH),
                false,
                None,
                move |page: &mut Self, value, _window, cx| {
                    page.set_interface_override(&interface_agent, value, cx)
                },
                window,
                cx,
            );
            children.push(panel_child(setting_row(
                p,
                SharedString::from(format!("agent-interface-row-{agent_id}")),
                RowSpec::new("Default interface").description(
                    "Open this agent in Chat or Terminal, or follow the app-wide default.",
                ),
                None,
                select,
                cx,
            )));
        }
        if let Some(controls) = self.render_cli_controls(p, cli_agent, window, cx) {
            children.push(group_title(p, "CLI"));
            children.push(controls);
        }
        if let Some(hook) = hook_agent {
            children.push(group_title(p, "Session resume hook"));
            let hook_installed = hook_status.is_some_and(|status| status.status == "installed");
            let label = if hook_installed {
                "Reinstall"
            } else if hook_status.is_some_and(|status| status.status == "updateRequired") {
                "Update hook"
            } else {
                "Install hook"
            };
            let loading_reason: SharedString = "Hook status is being checked.".into();
            let install_hook = hook.to_string();
            // Ghostex follows this agent without its hook (Use agent hooks off): nothing to
            // install, the detail says so.
            let not_required = hook_status.is_some_and(|status| status.status == "notRequired");
            let mut controls =
                h_flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.0))
                    .children((!not_required).then(|| {
                        settings_button_sized(
                            p,
                            SharedString::from(format!("agent-hook-install-{agent_id}")),
                            label,
                            Some(if hook_installed {
                                icons::REFRESH
                            } else {
                                icons::DOWNLOAD
                            }),
                            ButtonVariant::Outline,
                            ButtonSize::Sm,
                            loading,
                            Some(loading_reason.clone()),
                            move |page: &mut Self, _window, cx| {
                                page.install_hooks(Some(vec![install_hook.clone()]), cx);
                                cx.notify();
                            },
                            cx,
                        )
                    }));
            if hook_removable(hook_status) {
                let uninstall_hook = hook.to_string();
                controls = controls.child(settings_button_sized(
                    p,
                    SharedString::from(format!("agent-hook-uninstall-{agent_id}")),
                    "Uninstall hook",
                    Some(icons::TRASH),
                    ButtonVariant::Destructive,
                    ButtonSize::Sm,
                    loading,
                    Some(loading_reason),
                    move |page: &mut Self, _window, cx| {
                        page.uninstall_hooks(Some(vec![uninstall_hook.clone()]), cx);
                        cx.notify();
                    },
                    cx,
                ));
            }
            let detail = h_flex()
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .child(hook_detail_icon(
                    p,
                    hook_status,
                    pending,
                    &format!("agent-hook-detail-{agent_id}"),
                ))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(
                            hook_status
                                .map(|status| status.detail.clone())
                                .unwrap_or_else(|| "Waiting for hook check".to_string()),
                        ),
                )
                .into_any_element();
            children.push(panel_child(settings_list_item(
                p,
                None,
                None,
                "Session resume hook",
                Some(detail),
                Some(controls.into_any_element()),
            )));
        }
        let duplicate_agent = agent.clone();
        let mut actions =
            h_flex()
                .flex_wrap()
                .justify_end()
                .gap(px(8.0))
                .child(settings_button_sized(
                    p,
                    SharedString::from(format!("agent-duplicate-{agent_id}")),
                    "Duplicate as custom agent",
                    Some(icons::COPY),
                    ButtonVariant::Outline,
                    ButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, window, cx| {
                        page.open_editor_from(&duplicate_agent, window, cx);
                        cx.notify();
                    },
                    cx,
                ));
        let customized = agent
            .default_name
            .as_ref()
            .is_some_and(|name| name != &agent.name)
            || agent.default_command.as_ref().is_some_and(|command| {
                Some(command.as_str()) != agent.command.as_deref().map(str::trim)
            })
            || agent.accept_all_mode.is_some();
        if agent.is_default && customized {
            let reset_agent = agent.clone();
            actions = actions.child(settings_button_sized(
                p,
                SharedString::from(format!("agent-reset-{agent_id}")),
                "Reset to defaults",
                Some(icons::ARROW_BACK_UP),
                ButtonVariant::Ghost,
                ButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| page.reset_agent(&reset_agent, cx),
                cx,
            ));
        }
        if !agent.is_default {
            let delete_id = agent_id.clone();
            actions = actions.child(settings_button_sized(
                p,
                SharedString::from(format!("agent-delete-{agent_id}")),
                "Delete agent",
                Some(icons::TRASH),
                ButtonVariant::Destructive,
                ButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.post(
                        json!({ "agentId": delete_id, "type": "deleteSidebarAgent" }),
                        cx,
                    );
                },
                cx,
            ));
        }
        children.push(panel_child(settings_list_item(
            p,
            None,
            None,
            "Agent",
            None,
            Some(actions.into_any_element()),
        )));
        let hairline = hsla(p.hairline);
        v_flex()
            .w_full()
            .border_t_1()
            .border_color(hsla(css_fade(p.hairline, 0.7)))
            .children(children.into_iter().enumerate().map(|(index, child)| {
                div()
                    .w_full()
                    .when(index > 0, |this| this.border_t_1().border_color(hairline))
                    .child(child)
            }))
            .into_any_element()
    }
}
