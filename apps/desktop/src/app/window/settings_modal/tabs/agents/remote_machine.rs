//! The Agents page with a remote machine picked (tabs/machine_picker.rs): one row per agent CLI
//! Ghostex can install, as that machine's gxserver reports it (installed or not, version, an
//! update), with the row's Install / Update action and, opened, the same CLI controls as this
//! computer's rows (cli.rs, which sends every request to the picked machine). Turning agents on,
//! their order, defaults and resume hooks stay with This computer.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{settings_icon, settings_section};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::cli::{CliState, cli_catalog, ghost_icon_button, spinning_icon};
use super::icons;
use super::logos::agent_icon_tile;
use super::model::default_agent_by_id;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Transformation, Window, div, px,
    radians,
};
use gpui_component::{h_flex, v_flex};

/// One agent CLI on the machine.
struct RemoteAgent {
    agent_id: String,
    name: String,
    icon: Option<String>,
    state: CliState,
}

/// What a row says under the agent's name.
fn remote_subtitle(state: &CliState) -> String {
    if state.executable_path.is_none() {
        return "Not installed".to_string();
    }
    let mut text = match &state.version {
        Some(version) => format!("Installed · {version}"),
        None => "Installed".to_string(),
    };
    if state.update_available
        && let Some(latest) = &state.latest_version
    {
        text.push_str(&format!(" · {latest} available"));
    } else if state.update_checking {
        text.push_str(" · checking for updates…");
    }
    text
}

impl AgentsTab {
    /// The machine's agent CLIs: installed ones first, each group in the catalog's order.
    fn remote_agents(&self) -> Vec<RemoteAgent> {
        let mut agents: Vec<RemoteAgent> = cli_catalog()
            .iter()
            .filter_map(|entry| {
                let state = self.cli.list.get(&entry.agent_id)?.clone();
                let default = default_agent_by_id(&entry.agent_id);
                Some(RemoteAgent {
                    agent_id: entry.agent_id.clone(),
                    name: default
                        .map(|agent| agent.name.clone())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| entry.binary.clone()),
                    icon: default
                        .map(|agent| agent.icon.clone())
                        .filter(|icon| !icon.is_empty()),
                    state,
                })
            })
            .collect();
        agents.sort_by_key(|agent| agent.state.executable_path.is_none());
        agents
    }

    /// The card for a remote machine: its CLI rows, or why there are none yet.
    pub(super) fn render_remote_agents(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let machine = self.cli_machine_label();
        let agents = self.remote_agents();
        let mut rows: Vec<AnyElement> = Vec::new();
        if agents.is_empty() {
            let (text, loading) = match &self.cli.list_error {
                Some(error) => (error.clone(), false),
                None => (format!("Reading the agents on {machine}…"), true),
            };
            rows.push(
                h_flex()
                    .w_full()
                    .px(px(20.0))
                    .py(px(16.0))
                    .gap(px(10.0))
                    .items_center()
                    .when(loading, |this| {
                        this.child(spinning_icon(
                            icons::REFRESH,
                            14.0,
                            p.muted,
                            "remote-agents-loading",
                        ))
                    })
                    .when(!loading, |this| {
                        this.child(
                            settings_icon(icons::ALERT_TRIANGLE, 14.0, p.muted).flex_shrink_0(),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(hsla(p.muted))
                            .child(text),
                    )
                    .into_any_element(),
            );
        } else {
            let mut list = v_flex().w_full();
            for (index, agent) in agents.iter().enumerate() {
                let row = self.render_remote_agent_row(p, agent, window, cx);
                list = list.child(
                    div()
                        .w_full()
                        .px(px(16.0))
                        .when(index > 0, |this| {
                            this.border_t_1().border_color(hsla(p.hairline))
                        })
                        .child(row),
                );
            }
            rows.push(list.into_any_element());
        }
        settings_section(
            p,
            format!("Agents on {machine}"),
            Some(
                "Install or update each agent's CLI there. Turning agents on, their order, defaults and resume hooks are set with This computer picked."
                    .into(),
            ),
            None,
            rows,
        )
    }

    fn render_remote_agent_row(
        &mut self,
        p: &SettingsPalette,
        agent: &RemoteAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agent_id = agent.agent_id.clone();
        let expanded = self.expanded.contains(&agent_id);
        let installed = agent.state.executable_path.is_some();
        let action = self.render_cli_row_action(p, &agent_id, !installed, cx);
        let toggle = agent_id.clone();
        let main = h_flex()
            .id(SharedString::from(format!(
                "remote-agent-row-open-{agent_id}"
            )))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("{} CLI", agent.name)))
            .aria_expanded(expanded)
            .flex_1()
            .min_w_0()
            .p(px(8.0))
            .gap(px(12.0))
            .items_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .cursor_pointer()
            .when(!installed, |this| this.opacity(0.7))
            .on_click(cx.listener(move |page, _, _window, cx| {
                page.toggle_expanded(&toggle, cx);
            }))
            .child(agent_icon_tile(agent.icon.as_deref(), p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(hsla(p.foreground))
                            .child(agent.name.clone()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .text_color(hsla(p.muted))
                            .child(remote_subtitle(&agent.state)),
                    ),
            );
        let chevron_agent = agent_id.clone();
        let chevron_icon = settings_icon(icons::CHEVRON_DOWN, 16.0, p.foreground)
            .when(expanded, |icon| {
                icon.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
            })
            .into_any_element();
        let chevron = ghost_icon_button(
            p,
            SharedString::from(format!("remote-agent-row-chevron-{agent_id}")),
            format!("{} CLI", agent.name),
            chevron_icon,
            false,
            expanded,
            move |page: &mut Self, _window, cx| page.toggle_expanded(&chevron_agent, cx),
            cx,
        );
        let hover = p.raised_hover;
        let header = h_flex()
            .id(SharedString::from(format!("remote-agent-row-{agent_id}")))
            .mx(px(-16.0))
            .px(px(16.0))
            .py(px(6.0))
            .min_h(px(56.0))
            .gap(px(10.0))
            .items_center()
            .hover(move |this| this.bg(hsla(hover)))
            .child(main)
            .children(action)
            .child(chevron);
        let mut row = v_flex().w_full().child(header);
        if expanded && let Some(controls) = self.render_cli_controls(p, &agent_id, window, cx) {
            row = row.child(div().w_full().pb(px(12.0)).child(controls));
        }
        row.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_row_says_whether_the_cli_is_there_and_what_update_is_waiting() {
        let missing = CliState::parse(&json!({ "platform": "linux" })).expect("state");
        assert_eq!(remote_subtitle(&missing), "Not installed");
        let current = CliState::parse(&json!({
            "platform": "linux",
            "executablePath": "/home/u/.local/bin/codex",
            "version": "0.40.0",
        }))
        .expect("state");
        assert_eq!(remote_subtitle(&current), "Installed · 0.40.0");
        let behind = CliState::parse(&json!({
            "platform": "linux",
            "executablePath": "/home/u/.local/bin/codex",
            "version": "0.40.0",
            "latestVersion": "0.41.0",
            "updateAvailable": true,
        }))
        .expect("state");
        assert_eq!(
            remote_subtitle(&behind),
            "Installed · 0.40.0 · 0.41.0 available"
        );
    }
}
