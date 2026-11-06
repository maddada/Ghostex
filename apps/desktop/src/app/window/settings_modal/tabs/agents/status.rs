//! What the Agents card says about CLIs and session resume hooks: a row speaks only when
//! something is wrong (its CLI is missing, its hook is off), an available update is a muted link,
//! and one summary line above the list counts only agents that are on and were used before, with
//! Fix all.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, settings_button_sized, settings_icon,
};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::cli::{CliSlot, cli_definition};
use super::icons;
use super::model::{AgentButton, HookStatus, hook_agent_id};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px, rgb,
};
use gpui_component::h_flex;

/// The amber of a problem: (text, icon).
pub(super) fn amber(p: &SettingsPalette) -> (gpui::Rgba, gpui::Rgba) {
    if p.light {
        (rgb(0x92400e), rgb(0x92400e))
    } else {
        (rgb(0xfcd34d), rgb(0xfbbf24))
    }
}

/// The emerald of a healthy state: (text, icon).
pub(super) fn emerald(p: &SettingsPalette) -> (gpui::Rgba, gpui::Rgba) {
    if p.light {
        (rgb(0x047857), rgb(0x047857))
    } else {
        (rgb(0x6ee7b7), rgb(0x34d399))
    }
}

/// What is wrong with an agent that is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowProblem {
    /// Its CLI is not on this computer.
    CliMissing,
    /// Its CLI is here but its session resume hook is not (or is out of date).
    HookOff { update: bool },
}

impl RowProblem {
    fn label(self) -> &'static str {
        match self {
            Self::CliMissing => "CLI not installed",
            Self::HookOff { update: false } => "Resume hook off",
            Self::HookOff { update: true } => "Hook needs update",
        }
    }

    fn sentence(self, name: &str) -> String {
        match self {
            Self::CliMissing => format!("{name} isn't installed"),
            Self::HookOff { update: false } => format!("{name}'s resume hook is off"),
            Self::HookOff { update: true } => format!("{name}'s resume hook needs an update"),
        }
    }
}

/// An amber "⚠ text" problem label.
fn problem_label(p: &SettingsPalette, text: &'static str) -> AnyElement {
    let (color, icon) = amber(p);
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.5))
        .line_height(px(16.0))
        .text_color(hsla(color))
        .child(settings_icon(icons::ALERT_TRIANGLE, 14.0, icon).flex_shrink_0())
        .child(text)
        .into_any_element()
}

impl AgentsTab {
    /// The problem a row that is on shows, if any.
    pub(super) fn row_problem(
        &self,
        agent: &AgentButton,
        status: Option<&HookStatus>,
        loading: bool,
    ) -> Option<RowProblem> {
        if !agent.enabled {
            return None;
        }
        let cli_agent = self.cli_agent_id(agent);
        let hook = hook_agent_id(agent);
        let hook_status = hook
            .as_deref()
            .and_then(|hook| status.and_then(|status| status.item(hook)));
        if cli_definition(&cli_agent).is_some()
            && self.cli_missing(&cli_agent, hook_status.map(|status| status.status.as_str()))
        {
            return Some(RowProblem::CliMissing);
        }
        if loading && status.is_none() {
            return None;
        }
        let hook_status = hook_status?;
        match hook_status.status.as_str() {
            "installed" | "notRequired" | "cliMissing" => None,
            "updateRequired" => Some(RowProblem::HookOff { update: true }),
            _ => Some(RowProblem::HookOff { update: false }),
        }
    }

    /// The status at the right of a row: nothing when all is well.
    pub(super) fn render_row_status(
        &mut self,
        p: &SettingsPalette,
        agent: &AgentButton,
        status: Option<&HookStatus>,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !agent.enabled || self.turn_on.contains_key(&agent.agent_id) {
            return Vec::new();
        }
        let cli_agent = self.cli_agent_id(agent);
        let problem = self.row_problem(agent, status, loading);
        let show_cli_action =
            self.cli_row_action_shown(&cli_agent, problem == Some(RowProblem::CliMissing), cx);
        if !show_cli_action {
            self.cli_unmount(CliSlot::Row, &cli_agent);
        }
        let mut items = Vec::new();
        match problem {
            Some(RowProblem::CliMissing) => {
                items.push(problem_label(
                    p,
                    if agent.synced_from.is_some() {
                        "Not installed here"
                    } else {
                        RowProblem::CliMissing.label()
                    },
                ));
                if let Some(action) = self.render_cli_row_action(p, &cli_agent, true, cx) {
                    items.push(action);
                }
            }
            Some(problem @ RowProblem::HookOff { update }) => {
                items.push(problem_label(p, problem.label()));
                if let Some(hook) = hook_agent_id(agent) {
                    items.push(settings_button_sized(
                        p,
                        SharedString::from(format!("agent-inline-install-{}", agent.agent_id)),
                        if update {
                            "Update hook"
                        } else {
                            "Install hook"
                        },
                        Some(icons::DOWNLOAD),
                        ButtonVariant::Outline,
                        ButtonSize::Sm,
                        loading,
                        Some("Hook status is being checked.".into()),
                        move |page: &mut Self, _window, cx| {
                            page.install_hooks(Some(vec![hook.clone()]), cx);
                            cx.notify();
                        },
                        cx,
                    ));
                }
            }
            None if show_cli_action => {
                if let Some(action) = self.render_cli_row_action(p, &cli_agent, false, cx) {
                    items.push(action);
                }
            }
            None => {}
        }
        items
    }

    /// The agents that are on, were used before and have a problem, one per CLI (a custom agent
    /// that works like a built-in shares its CLI and hook).
    ///
    /// CDXC:AgentHooks 2026-10-06 DECISION:
    /// User: the summary line and Fix all only count agents that are on AND were used before; unused agents' rows still show their own Install CLI. Built-in agents start on, so counting every agent that is on made Fix all install CLIs nobody uses.
    fn problems(
        &self,
        agents: &[AgentButton],
        status: Option<&HookStatus>,
        loading: bool,
    ) -> Vec<(AgentButton, RowProblem)> {
        let mut seen: Vec<String> = Vec::new();
        let mut problems = Vec::new();
        for agent in agents.iter().filter(|agent| agent.used_before()) {
            let Some(problem) = self.row_problem(agent, status, loading) else {
                continue;
            };
            let key = self.cli_agent_id(agent);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            problems.push((agent.clone(), problem));
        }
        problems
    }

    /// Fix all: install every missing CLI (its hook follows) and every missing hook.
    fn fix_all(&mut self, cx: &mut Context<Self>) {
        let agents = self.ordered_agents(cx);
        let status = self
            .store
            .read(cx)
            .host_payload("agentHookStatus")
            .map(HookStatus::parse);
        let problems = self.problems(&agents, status.as_ref(), self.hook_status_loading);
        let mut hooks = Vec::new();
        for (agent, problem) in problems {
            match problem {
                RowProblem::CliMissing => {
                    let cli_agent = self.cli_agent_id(&agent);
                    if hook_agent_id(&agent).is_some() {
                        self.hook_after_cli.insert(cli_agent.clone());
                    }
                    self.cli_install(CliSlot::Row, &cli_agent, cx);
                }
                RowProblem::HookOff { .. } => {
                    if let Some(hook) = hook_agent_id(&agent)
                        && !hooks.contains(&hook)
                    {
                        hooks.push(hook);
                    }
                }
            }
        }
        if !hooks.is_empty() {
            self.install_hooks(Some(hooks), cx);
        }
        cx.notify();
    }

    /// The agents turned on from another computer whose CLI isn't on this one, one per CLI.
    fn missing_from_sync(
        &self,
        agents: &[AgentButton],
        status: Option<&HookStatus>,
        loading: bool,
    ) -> Vec<AgentButton> {
        let mut seen: Vec<String> = Vec::new();
        let mut missing = Vec::new();
        // One used here already is in the summary line.
        for agent in agents
            .iter()
            .filter(|agent| agent.synced_from.is_some() && !agent.used_before())
        {
            if self.row_problem(agent, status, loading) != Some(RowProblem::CliMissing) {
                continue;
            }
            let key = self.cli_agent_id(agent);
            if !seen.contains(&key) {
                seen.push(key);
                missing.push(agent.clone());
            }
        }
        missing
    }

    /// Install all missing: each agent's own installer, the one its Install CLI button runs.
    /// Nothing installs until this is clicked.
    fn install_all_missing(&mut self, cx: &mut Context<Self>) {
        let agents = self.ordered_agents(cx);
        let status = self
            .store
            .read(cx)
            .host_payload("agentHookStatus")
            .map(HookStatus::parse);
        for agent in self.missing_from_sync(&agents, status.as_ref(), self.hook_status_loading) {
            let cli_agent = self.cli_agent_id(&agent);
            if hook_agent_id(&agent).is_some() {
                self.hook_after_cli.insert(cli_agent.clone());
            }
            self.cli_install(CliSlot::Row, &cli_agent, cx);
        }
        cx.notify();
    }

    /// The line above the list when agents came from another computer without their CLI here:
    /// how many, and Install all missing.
    ///
    /// CDXC:SettingsSync 2026-11-07 WHY:
    /// Synced agents were never used here, so the summary line (only agents used before, CDXC:AgentHooks 2026-10-06) leaves them out; this line names them on their own and never installs anything until clicked.
    pub(super) fn render_missing_from_sync_line(
        &mut self,
        p: &SettingsPalette,
        agents: &[AgentButton],
        status: Option<&HookStatus>,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.shown_machine.is_some() || !self.cli_connected(cx) {
            return None;
        }
        let missing = self.missing_from_sync(agents, status, loading);
        let first = missing.first()?;
        let from = first.synced_from.clone().unwrap_or_default();
        let (text, icon) = amber(p);
        let names: Vec<String> = missing
            .iter()
            .take(3)
            .map(|agent| agent.name.clone())
            .collect();
        let mut details = names.join(", ");
        if missing.len() > 3 {
            details.push_str(&format!(" and {} more", missing.len() - 3));
        }
        let install = settings_button_sized(
            p,
            "agents-install-all-missing",
            "Install all missing",
            Some(icons::DOWNLOAD),
            ButtonVariant::Outline,
            ButtonSize::Sm,
            false,
            None,
            |page: &mut Self, _window, cx| page.install_all_missing(cx),
            cx,
        );
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap(px(10.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(settings_icon(icons::DOWNLOAD, 15.0, icon).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(hsla(p.muted))
                        .child(div().text_color(hsla(text)).child(if missing.len() == 1 {
                            format!("1 agent from {from} isn't installed here")
                        } else {
                            format!(
                                "{} agents from your other computers aren't installed here",
                                missing.len()
                            )
                        }))
                        .child(details),
                )
                .child(install)
                .into_any_element(),
        )
    }

    /// The one line above the list: the problems with Fix all, or a quiet "ready" line.
    pub(super) fn render_summary_line(
        &mut self,
        p: &SettingsPalette,
        agents: &[AgentButton],
        status: Option<&HookStatus>,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let status_ref = status?;
        if status_ref.error_message.is_some() {
            return None;
        }
        let problems = self.problems(agents, status, loading);
        let line = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(10.0))
            .text_size(px(13.0))
            .line_height(px(18.0));
        if problems.is_empty() {
            let mut hooked: Vec<String> = agents
                .iter()
                .filter(|agent| agent.enabled && agent.used_before())
                .filter_map(hook_agent_id)
                .collect();
            hooked.sort();
            hooked.dedup();
            let ready = hooked
                .iter()
                .filter(|hook| {
                    status_ref
                        .item(hook)
                        .is_some_and(|item| item.status == "installed")
                })
                .count();
            if ready == 0 {
                return None;
            }
            let (_, icon) = emerald(p);
            return Some(
                line.text_color(hsla(p.muted))
                    .child(settings_icon(icons::CIRCLE_CHECK_FILLED, 15.0, icon).flex_shrink_0())
                    .child(div().min_w_0().child(if ready == 1 {
                        "Session resume is ready for the agent you use".to_string()
                    } else {
                        format!("Session resume is ready for all {ready} agents you use")
                    }))
                    .into_any_element(),
            );
        }
        let (text, icon) = amber(p);
        let count = problems.len();
        let mut details: Vec<String> = problems
            .iter()
            .take(3)
            .map(|(agent, problem)| problem.sentence(&agent.name))
            .collect();
        if count > 3 {
            details.push(format!("{} more", count - 3));
        }
        let fix = settings_button_sized(
            p,
            "agents-fix-all",
            "Fix all",
            None,
            ButtonVariant::Outline,
            ButtonSize::Sm,
            loading,
            Some("Hook status is being checked.".into()),
            |page: &mut Self, _window, cx| page.fix_all(cx),
            cx,
        );
        Some(
            line.child(settings_icon(icons::ALERT_TRIANGLE, 15.0, icon).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(hsla(p.muted))
                        .child(div().text_color(hsla(text)).child(if count == 1 {
                            "1 agent needs attention".to_string()
                        } else {
                            format!("{count} agents need attention")
                        }))
                        .child(details.join(" · ")),
                )
                .child(fix)
                .into_any_element(),
        )
    }
}
