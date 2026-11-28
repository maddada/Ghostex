//! The expanded panel of an Agents row (`settings-list-panel`). While the agent is off it holds
//! only the CLI group and the Agent actions (`off_note`). While it is on, in these groups: Start (Name,
//! Works like for custom agents, Command, Permission mode, Default interface for chat-capable
//! agents, CDXC:AgentProviders 2026-08-27), Context (Claude Code only, claude_context.rs), CLI (the
//! agent CLI controls) and Session resume hook;
//! then Duplicate as custom agent, Reset to defaults (built-in agents) and Delete agent (custom
//! agents only: a built-in agent is turned off with its switch). Its rows sit inside the list
//! inset, so they drop their own side padding and divide with hairlines.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, ROW_PADDING_X, RowSpec, SELECT_WIDTH, setting_row,
    settings_button_sized, settings_list_item, settings_select, settings_select_described,
    wrapped_tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::icons;
use super::model::{
    AgentButton, HookStatusItem, auto_permission_family, default_agents, hook_removable,
    inherit_value, non_cli_mode_family, permission_mode_options,
    preferred_interface_override_options, session_mode_options, supports_accept_all,
    supports_chat_view,
};
use super::roster::hook_detail_icon;
use super::select::DropdownOption;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::time::Duration;

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

/// What a turned-off agent's panel says in place of the rows that only matter while it is on.
///
/// CDXC:Settings 2026-11-14 DECISION:
/// User (W440, Q3107 "Also fix existing ones now"): a setting that only matters when its parent switch is on is hidden while it is off. A turned-off agent's panel showed every row as usable and only dimmed its header. Now, while an agent is off, its panel hides Start (Name, Works like, Command, Permission mode, Default interface), Context (Claude Code's idle compaction) and Session resume hook, and Reset to defaults with them, and says to turn the agent on to change them. It keeps what manages the agent while it is off: the CLI group (install, update, uninstall), Duplicate as custom agent and, for a custom agent, Delete agent. The switch in the row's header turns it back on.
fn off_note(p: &SettingsPalette, name: &str) -> AnyElement {
    div()
        .py(px(12.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(hsla(p.muted))
        .child(format!(
            "{name} is turned off. Turn it on with the switch above to change its command, permission mode and other options."
        ))
        .into_any_element()
}

/// Width of the Name and Command inputs.
const INPUT_WIDTH: f32 = 300.0;

/// The warning dot's tooltip on an Antigravity command that runs a custom agent.
const ANTIGRAVITY_AGENT_FLAG_WARNING: &str = "Antigravity bills custom agents (--agent) at about 4x the usual quota. Put your instructions in AGENTS.md, GEMINI.md or a skill instead.";

/// Whether `command` passes Antigravity's `--agent` flag (`--agent x` or `--agent=x`) as a word of
/// its own, not inside another word (`--agents`, `x--agent`).
fn has_agent_flag(command: &str) -> bool {
    command
        .split_whitespace()
        .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
        .any(|word| word == "--agent" || word.starts_with("--agent="))
}

/// The small yellow dot inside the Command field of an Antigravity command that uses `--agent`,
/// pulsing softly (opacity 100% to 40%, 1.2 s a cycle), with the warning as its tooltip.
///
/// CDXC:AgentProviders 2026-11-28 DECISION:
/// Antigravity bills a conversation run by a custom agent (`--agent <name>`, `.agents/agents/*`,
/// plugin agents and their subagents) at about 4 times the usual quota
/// (google-antigravity/antigravity-cli#1890, closed as intended). Ghostex never adds `--agent`, but
/// a user can type it into Antigravity's Command or a custom agent that works like Antigravity. The
/// user's design: "make it a yellow flashing dot on the field, when u hover it then it shows that
/// warning". The dot is a bright yellow (SettingsPalette::agent_flag_warning_yellow), at the field's right end, and only shows while
/// the command (as typed, before it is saved) has the flag. SEE-ALSO: the "Custom flags ignored"
/// card in server/src/session_chat_options/detection.rs.
fn agent_flag_dot(p: &SettingsPalette, agent_id: &str) -> AnyElement {
    let colour = hsla(p.agent_flag_warning_yellow());
    div()
        .id(SharedString::from(format!(
            "agent-command-agent-flag-{agent_id}"
        )))
        .size(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_default()
        .child(
            div()
                .size(px(8.0))
                .rounded_full()
                .bg(colour)
                .with_animation(
                    SharedString::from(format!("agent-command-agent-flag-pulse-{agent_id}")),
                    Animation::new(Duration::from_millis(1200))
                        .repeat()
                        .with_max_fps(crate::app::helpers::INDICATOR_MAX_FPS),
                    |dot, delta| {
                        // Bright, fading to 40% and back, eased like a breath.
                        let wave = 0.5 + 0.5 * (delta * std::f32::consts::TAU).cos();
                        dot.opacity(0.4 + 0.6 * wave)
                    },
                ),
        )
        .tooltip(wrapped_tooltip_text(ANTIGRAVITY_AGENT_FLAG_WARNING))
        .into_any_element()
}

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
        let family = ghostex_gx_chat_core::menus::catalog::canonical_model_family(
            agent.icon.as_deref().unwrap_or(&agent.agent_id),
        )
        .to_string();
        let mut defaults = self
            .store
            .read(cx)
            .value("agentDefaultModels")
            .as_object()
            .cloned()
            .unwrap_or_default();
        if defaults.remove(&family).is_some() {
            self.save("agentDefaultModels", Value::Object(defaults), cx);
        }
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

    fn set_session_mode(&mut self, family: &str, mode: String, cx: &mut Context<Self>) {
        let (mut map, legacy_map) = {
            let store = self.store.read(cx);
            let modes = store
                .value("agentSessionMode")
                .as_object()
                .cloned()
                .unwrap_or_default();
            let legacy = store.value("agentAcpMode").as_object().cloned();
            (modes, legacy)
        };
        let normalized = if mode == "non-cli" {
            "non-cli"
        } else {
            "agent-cli"
        };
        map.insert(family.to_string(), json!(normalized));
        let mut patch = Map::new();
        patch.insert("agentSessionMode".to_string(), Value::Object(map));
        if let Some(mut legacy) = legacy_map {
            if legacy.remove(family).is_some() {
                patch.insert("agentAcpMode".to_string(), Value::Object(legacy));
            }
        }
        self.store.update(cx, |store, cx| {
            store.apply_patch(patch, "settings:control", cx)
        });
    }

    fn set_default_model(&mut self, family: &str, model: String, cx: &mut Context<Self>) {
        let mut map = self
            .store
            .read(cx)
            .value("agentDefaultModels")
            .as_object()
            .cloned()
            .unwrap_or_default();
        if ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(&model) {
            map.remove(family);
        } else {
            map.insert(family.to_string(), json!(model));
        }
        self.save("agentDefaultModels", Value::Object(map), cx);
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
        let mut children: Vec<AnyElement> = Vec::new();
        if agent.enabled {
            children.push(group_title(p, "Start"));
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
            let antigravity = auto_permission_family(&agent.agent_id, agent.icon.as_deref())
                == Some("antigravity");
            let dot_palette = *p;
            let dot_agent_id = agent_id.clone();
            let command_input = self.inline_input_with_trailing(
                p,
                SharedString::from(format!("agent-command-{agent_id}")),
                agent.command.as_deref().unwrap_or_default(),
                "Command",
                INPUT_WIDTH,
                true,
                move |page: &mut Self, command, cx| {
                    page.save_agent_fields(&command_agent, None, Some(command), None, cx)
                },
                antigravity.then_some(move |command: &str| {
                    has_agent_flag(command).then(|| agent_flag_dot(&dot_palette, &dot_agent_id))
                }),
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
            let mode_select = settings_select_described(
                self,
                p,
                SharedString::from(format!("agent-permission-{agent_id}")),
                &permission_mode_options(&agent.agent_id, agent.icon.as_deref()),
                &mode,
                Some(SELECT_WIDTH),
                !accept_supported,
                Some("This agent doesn’t support approval policy changes.".into()),
                move |page: &mut Self, value, _window, cx| {
                    page.save_agent_mode(&mode_agent, value, cx)
                },
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
            if let Some(family) = non_cli_mode_family(&agent.agent_id, agent.icon.as_deref()) {
                let current_mode = {
                    let store = self.store.read(cx);
                    match store
                        .value("agentSessionMode")
                        .get(family)
                        .and_then(Value::as_str)
                    {
                        Some("non-cli") => "non-cli",
                        Some("agent-cli") => "agent-cli",
                        _ if store
                            .value("agentAcpMode")
                            .get(family)
                            .and_then(Value::as_bool)
                            == Some(true) =>
                        {
                            "non-cli"
                        }
                        _ => "agent-cli",
                    }
                };
                let control = settings_select(
                    self,
                    p,
                    SharedString::from(format!("agent-session-mode-{agent_id}")),
                    &session_mode_options(),
                    current_mode,
                    Some(SELECT_WIDTH),
                    false,
                    None,
                    move |page: &mut Self, value, _window, cx| {
                        page.set_session_mode(family, value, cx);
                    },
                    window,
                    cx,
                );
                let session_mode_row = setting_row(
                    p,
                    SharedString::from(format!("agent-session-mode-row-{agent_id}")),
                    RowSpec::new("Session mode").description(if family == "antigravity" {
                        // CDXC:AgentProviders 2026-11-27: Native Antigravity refuses agy older
                        // than 1.5.1 (server/src/acp/startup_error.rs), so its row says so.
                        "Choose Native (faster, with live approvals; the default; needs Antigravity CLI 1.5.1 or newer) or Terminal-based (the agent's own app in a terminal, with instant switching between Chat and Terminal). Applies to new or resumed sessions."
                    } else {
                        "Choose Native (faster, with live approvals; the default) or Terminal-based (the agent's own app in a terminal, with instant switching between Chat and Terminal). Applies to new or resumed sessions."
                    }),
                    None,
                    control,
                    cx,
                );
                children.push(panel_child(session_mode_row));
            }
            if let Some(catalog) = crate::app::gx_chat::current_model_catalog() {
                let model_family = ghostex_gx_chat_core::menus::catalog::canonical_model_family(
                    agent.icon.as_deref().unwrap_or(&agent.agent_id),
                )
                .to_string();
                let catalog_agent = catalog
                    .agents
                    .get(&model_family)
                    .or_else(|| catalog.agents.get(&agent.agent_id));
                if let Some(cat_agent) = catalog_agent {
                    let options: Vec<super::super::super::catalog::SettingOption> = cat_agent
                        .models
                        .iter()
                        .filter(|m| {
                            !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(
                                &m.value,
                            ) && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_label(
                                &m.label,
                            )
                        })
                        .map(|m| super::super::super::catalog::SettingOption {
                            label: m.label.clone(),
                            value: m.value.clone(),
                        })
                        .collect();
                    if !options.is_empty() {
                        let explicit = self
                            .store
                            .read(cx)
                            .value("agentDefaultModels")
                            .get(&model_family)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|v| options.iter().any(|opt| opt.value == *v))
                            .map(str::to_string);
                        let remembered = self
                            .agent_model_pins
                            .get(&model_family)
                            .filter(|v| options.iter().any(|opt| opt.value == **v))
                            .cloned();
                        let fallback =
                            ghostex_gx_chat_core::menus::catalog::preferred_default_model(
                                &model_family,
                                cat_agent,
                            )
                            .map(|m| m.value.clone())
                            .unwrap_or_else(|| options[0].value.clone());
                        let current_model = explicit.or(remembered).unwrap_or(fallback);
                        let save_family = model_family.clone();
                        let select = settings_select(
                            self,
                            p,
                            SharedString::from(format!("agent-default-model-{agent_id}")),
                            &options,
                            &current_model,
                            Some(SELECT_WIDTH),
                            false,
                            None,
                            move |page: &mut Self, value, _window, cx| {
                                page.set_default_model(&save_family, value, cx);
                            },
                            window,
                            cx,
                        );
                        children.push(panel_child(setting_row(
                            p,
                            SharedString::from(format!("agent-default-model-row-{agent_id}")),
                            RowSpec::new("Default model").description(
                                "Model new chats and threads of this agent start on. Overrides the last model picked in chat.",
                            ),
                            None,
                            select,
                            cx,
                        )));
                    }
                }
            }
            // Claude Code's idle compaction and prompt cache length (claude_context.rs).
            if agent.is_default && agent_id == super::claude_context::CLAUDE_AGENT_ID {
                children.push(group_title(p, "Context"));
                let rows = self.claude_context_rows(p, window, cx);
                children.extend(rows.into_iter().map(panel_child));
            }
            // What a message sent to a busy OMP or Pi does (pi_messages.rs). A Power User Flag
            // that stays in the panel and shows only with the page's flags (CDXC:Settings
            // 2026-11-27 in packages/settings-catalog/src/pages/agents.rs).
            if super::pi_messages::is_pi_like(agent)
                && !self
                    .store
                    .read(cx)
                    .general_search()
                    .hidden_keys
                    .contains(&"piMessagesWhileWorking")
            {
                children.push(group_title(p, "Messages"));
                let row = self.pi_messages_row(p, &agent_id, window, cx);
                children.push(panel_child(row));
            }
        } else {
            children.push(off_note(p, &agent.name));
        }
        if let Some(controls) = self.render_cli_controls(p, cli_agent, window, cx) {
            children.push(group_title(p, "CLI"));
            children.push(controls);
        }
        if let Some(hook) = hook_agent.filter(|_| agent.enabled) {
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
        if agent.enabled && agent.is_default && customized {
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
