//! The Agents card: the summary line, one drag-to-reorder row per agent in the list (grip, icon,
//! name with the Chat View and Custom badges, command or "Last used …", the row's problem if it
//! has one, the on/off switch, disclosure), the step a row shows right after it was turned on,
//! the expanded panel, "More agents", the one-time tidy-up offer and "Add custom agent" or its
//! form.
//!
//! CDXC:AgentLauncher 2026-10-06 DECISION:
//! User: "ok implement the plan", choosing A1, B2, C1 and D3 of the Agents page mockup (docs/2026-10-06/agents-settings/). A1: agents that are off but were used before stay dimmed in place in the list. B2: agents that are off and never used are a compact chip grid under "More agents"; a click turns one on. C1: custom agents sit in the same list with a Custom tag and are the only rows with Delete; "Add custom agent" is the last row. D3: a row speaks only when something is wrong, with one summary line and Fix all above the list. Every agent has a switch; built-in agents can only be turned off, never removed.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    card_inset, reorder_handle, reorder_order, reorder_row, reorder_scroll_container,
    settings_icon, settings_section, switch_control, tooltip_text, wrapped_tooltip_text,
};
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::cli::{ghost_icon_button, spinning_icon};
use super::icons;
use super::logos::{agent_icon_tile, muted_fill};
use super::model::{
    AgentButton, HookStatus, HookStatusItem, hook_agent_id, last_used_label, supports_chat_view,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Transformation, Window, div, px,
    radians, rgb,
};
use gpui_component::{h_flex, v_flex};

const LIST: &str = "settings-agents";

/// The pill colours of a hook status (`getAgentHookStatusClassName` and the icon tints).
fn pill_colors(
    p: &SettingsPalette,
    status: Option<&HookStatusItem>,
    loading: bool,
) -> (gpui::Rgba, gpui::Rgba, gpui::Rgba) {
    let muted_pill = (muted_fill(p), p.muted, p.muted);
    if loading {
        return muted_pill;
    }
    let Some(status) = status else {
        return muted_pill;
    };
    let emerald = (
        css_fade(rgb(0x10b981), 0.1),
        if p.light {
            rgb(0x047857)
        } else {
            rgb(0x6ee7b7)
        },
        if p.light {
            rgb(0x047857)
        } else {
            rgb(0x34d399)
        },
    );
    let amber = (
        css_fade(rgb(0xf59e0b), 0.1),
        if p.light {
            rgb(0x92400e)
        } else {
            rgb(0xfcd34d)
        },
        if p.light {
            rgb(0x92400e)
        } else {
            rgb(0xfbbf24)
        },
    );
    match status.status.as_str() {
        "installed" => emerald,
        "updateRequired" | "cliMissing" => amber,
        "notRequired" => muted_pill,
        _ => (css_fade(p.destructive, 0.1), p.destructive, p.destructive),
    }
}

/// The icon of a hook's detail line (`AgentHookStatusIcon` in the row panel).
pub(super) fn hook_detail_icon(
    p: &SettingsPalette,
    status: Option<&HookStatusItem>,
    loading: bool,
    id: &str,
) -> AnyElement {
    let (_, _, color) = pill_colors(p, status, loading);
    if loading {
        return spinning_icon(icons::REFRESH, 14.0, color, id);
    }
    let path = match status.map(|status| status.status.as_str()) {
        None | Some("notRequired") => icons::INFO_CIRCLE,
        Some("installed") => icons::CIRCLE_CHECK_FILLED,
        Some("updateRequired") | Some("cliMissing") => icons::ALERT_TRIANGLE,
        Some(_) => icons::CIRCLE_X,
    };
    settings_icon(path, 14.0, color)
        .flex_shrink_0()
        .into_any_element()
}

/// The violet of the Custom tag.
fn custom_tag_color(p: &SettingsPalette) -> gpui::Rgba {
    if p.light {
        rgb(0x6d28d9)
    } else {
        rgb(0xc4b5fd)
    }
}

impl AgentsTab {
    fn toggle_expanded(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        if let Some(position) = self.expanded.iter().position(|id| id == agent_id) {
            self.expanded.remove(position);
            self.cli_unmount(super::cli::CliSlot::Panel, agent_id);
        } else {
            self.expanded.push(agent_id.to_string());
        }
        cx.notify();
    }

    pub(super) fn render_roster(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        self.roster_follow_hud(cx);
        let status = self
            .store
            .read(cx)
            .host_payload("agentHookStatus")
            .map(HookStatus::parse);
        let loading = self.hook_status_loading;
        let all = self.all_agents(cx);
        let agents = self.ordered_agents(cx);
        let unlisted = self.unlisted_agents(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(tidy) = self.render_tidy_up(p, &all, cx) {
            rows.push(tidy);
        }
        if let Some(line) = self.render_summary_line(p, &agents, status.as_ref(), loading, cx) {
            rows.push(card_inset(line));
        }
        if agents.is_empty() {
            rows.push(card_inset(
                v_flex()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .text_center()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(24.9))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("No agents are on"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(21.1))
                            .text_color(hsla(p.muted))
                            .child("Turn one on under More agents, or add a custom agent."),
                    ),
            ));
        } else {
            let handle = self
                .store
                .update(cx, |store, _| store.scroll_handle(SettingsTabId::Agents));
            reorder_scroll_container(self, LIST, handle);
            let order = reorder_order(self, LIST, agents.len(), cx);
            let mut list = v_flex().w_full();
            for (slot, index) in order.iter().copied().enumerate() {
                let agent = agents[index].clone();
                let row =
                    self.render_agent_row(p, &agent, index, status.as_ref(), loading, window, cx);
                let wrapped = reorder_row(
                    self,
                    LIST,
                    index,
                    slot,
                    row,
                    |page: &mut Self, from, to, _window, cx| page.move_agent(from, to, cx),
                    cx,
                );
                list = list.child(
                    div()
                        .w_full()
                        .px(px(16.0))
                        .when(slot > 0, |this| {
                            this.border_t_1().border_color(hsla(p.hairline))
                        })
                        .child(wrapped),
                );
            }
            rows.push(list.into_any_element());
        }
        if let Some(more) = self.render_more_agents(p, &unlisted, cx) {
            rows.push(more);
        }
        if self.editor.is_some() {
            let form = self.render_editor(p, window, cx);
            rows.push(
                v_flex()
                    .w_full()
                    .child(
                        div()
                            .px(px(20.0))
                            .pt(px(14.0))
                            .text_size(px(14.0))
                            .text_color(hsla(p.foreground))
                            .child("New custom agent"),
                    )
                    .children(form)
                    .into_any_element(),
            );
        } else {
            rows.push(self.render_add_custom_row(p, cx));
        }
        settings_section(p, "Agents", None, None, rows)
    }

    fn render_add_custom_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let hover = p.raised_hover;
        h_flex()
            .id("agents-add-custom")
            .role(gpui::Role::Button)
            .aria_label("Add custom agent")
            .w_full()
            .px(px(20.0))
            .py(px(12.0))
            .gap(px(10.0))
            .items_center()
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_press(cx, |page, window, cx| {
                page.open_editor(None, window, cx);
                cx.notify();
            })
            .child(settings_icon(icons::PLUS, 15.0, p.muted).flex_shrink_0())
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(hsla(p.foreground))
                    .child("Add custom agent"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(p.muted))
                    .child("Your own command, or a variant of a built-in agent"),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_agent_row(
        &mut self,
        p: &SettingsPalette,
        agent: &AgentButton,
        index: usize,
        status: Option<&HookStatus>,
        loading: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agent_id = agent.agent_id.clone();
        let hook_agent = hook_agent_id(agent);
        let hook_status = hook_agent
            .as_deref()
            .and_then(|hook| status.and_then(|status| status.item(hook)));
        let pending = loading && status.is_none();
        let expanded = self.expanded.contains(&agent_id);
        let cli_agent = self.cli_agent_id(agent);
        let name = agent.name.clone();
        let grip = reorder_handle(
            p,
            LIST,
            index,
            name.clone(),
            div()
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .hover(|this| {
                    this.bg(hsla(if p.light {
                        rgb(0xf1f1f1)
                    } else {
                        css_fade(rgb(0x262626), 0.5)
                    }))
                })
                .child(settings_icon(icons::GRIP_VERTICAL, 16.0, p.foreground))
                .into_any_element(),
        );
        let subtitle = if agent.enabled {
            agent
                .command
                .as_deref()
                .map(str::trim)
                .filter(|command| !command.is_empty())
                .unwrap_or("Not configured")
                .to_string()
        } else {
            match &agent.last_used_at {
                Some(at) => format!("Off · {}", last_used_label(at).to_lowercase()),
                None => "Off".to_string(),
            }
        };
        let chat_badge = supports_chat_view(&agent.agent_id, agent.icon.as_deref()).then(|| {
            div()
                .id(SharedString::from(format!("agent-chat-badge-{agent_id}")))
                .flex_shrink_0()
                .size(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .tooltip(tooltip_text("Supports Chat View"))
                .child(settings_icon(
                    icons::MESSAGE_CIRCLE,
                    16.0,
                    css_fade(p.muted, 0.7),
                ))
        });
        let custom_tag = (!agent.is_default).then(|| {
            let color = custom_tag_color(p);
            div()
                .flex_shrink_0()
                .px(px(6.0))
                .py(px(1.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(hsla(css_fade(color, 0.35)))
                .text_size(px(11.0))
                .line_height(px(15.0))
                .text_color(hsla(color))
                .child("Custom")
        });
        let toggle_agent = agent_id.clone();
        let main = h_flex()
            .id(SharedString::from(format!("agent-row-open-{agent_id}")))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("{name} options")))
            .aria_expanded(expanded)
            .flex_1()
            .min_w_0()
            .p(px(8.0))
            .gap(px(12.0))
            .items_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .cursor_pointer()
            .when(!agent.enabled, |this| this.opacity(0.55))
            .on_press(cx, move |page, _window, cx| {
                page.toggle_expanded(&toggle_agent, cx);
            })
            .child(agent_icon_tile(agent.icon.as_deref(), p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(hsla(p.foreground))
                                    .child(name.clone()),
                            )
                            .children(chat_badge)
                            .children(custom_tag),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("agent-row-subtitle-{agent_id}")))
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .text_color(hsla(p.muted))
                            .when(agent.enabled, |this| this.font_family(MODAL_MONO_FONT))
                            .tooltip(wrapped_tooltip_text(subtitle.clone()))
                            .child(subtitle),
                    ),
            );
        let row_status = self.render_row_status(p, agent, status, loading, cx);
        let switch_agent = agent.clone();
        let switch = switch_control(
            p,
            SharedString::from(format!("agent-enabled-{agent_id}")),
            SharedString::from(name.clone()),
            agent.enabled,
            false,
            None,
            move |page: &mut Self, on, _window, cx| {
                if on {
                    page.turn_on_agent(&switch_agent, cx);
                } else {
                    page.set_agents_enabled(vec![switch_agent.agent_id.clone()], false, cx);
                }
            },
            cx,
        );
        let chevron_agent = agent_id.clone();
        let chevron_icon = settings_icon(icons::CHEVRON_DOWN, 16.0, p.foreground)
            .when(expanded, |icon| {
                icon.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
            })
            .into_any_element();
        let chevron = ghost_icon_button(
            p,
            SharedString::from(format!("agent-row-chevron-{agent_id}")),
            format!("{name} options"),
            chevron_icon,
            false,
            expanded,
            move |page: &mut Self, _window, cx| page.toggle_expanded(&chevron_agent, cx),
            cx,
        );
        let hover = p.raised_hover;
        let header = h_flex()
            .id(SharedString::from(format!("agent-row-{agent_id}")))
            .mx(px(-16.0))
            .px(px(16.0))
            .py(px(6.0))
            .min_h(px(56.0))
            .gap(px(10.0))
            .items_center()
            .hover(move |this| this.bg(hsla(hover)))
            .child(grip)
            .child(main)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .children(row_status),
            )
            .child(switch)
            .child(chevron);
        let mut row = v_flex().w_full().child(header);
        if let Some(step) = self.render_turn_on_step(p, agent, expanded, window, cx) {
            row = row.child(step);
        }
        if expanded {
            row = row.child(self.render_agent_panel(
                p,
                agent,
                hook_agent.as_deref(),
                hook_status,
                pending,
                &cli_agent,
                window,
                cx,
            ));
        }
        row.into_any_element()
    }
}
