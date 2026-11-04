//! Panel 2, Agents: the detected and missing agent rows, the integration and Computer Use switches,
//! the scan console / extension previews on the right and the phase tracker (panels/agents.tsx (deleted 2026-10-01),
//! styles/agents.css).
use super::agent_cli::InstallEvent;
use super::interact;
use super::model::{
    ComputerUseState, PRIMARY_AGENTS, all_installed_have_hooks, catalog_agent_name,
    default_agent_id, installed_agents,
};
use super::primitives::*;
use super::scan_log::{ExtraTone, ScanExtra, ScanLog};
use super::stage::*;
use super::welcome::foot_actions;
use super::{GpuiOnboardingWindow, OnboardingCommand};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, linear_color_stop,
    linear_gradient,
};
use gpui_component::tooltip::Tooltip;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How long "Connected" stays lit before the panel advances on its own.
const CONNECTED_HOLD: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RightTab {
    Scan,
    Integration,
    ComputerUse,
}

pub(crate) struct AgentsPanelState {
    pub(crate) right_tab: RightTab,
    pub(crate) right_tab_since: Instant,
    pub(crate) guide_open: bool,
    pub(crate) guide_opened_at: Instant,
    connecting: bool,
    computer_use_requested: bool,
    pub(crate) extras: Vec<ScanExtra>,
    hooks_before: HashMap<String, bool>,
    installed_before: Option<HashSet<String>>,
    connect_errors: Vec<String>,
    was_loading: bool,
    connected_since: Option<Instant>,
    last_computer_use: ComputerUseState,
    pub(crate) scan_log: ScanLog,
    pub(crate) scan_log_mounted: Instant,
}

impl AgentsPanelState {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            right_tab: RightTab::Scan,
            right_tab_since: now,
            guide_open: false,
            guide_opened_at: now,
            connecting: false,
            computer_use_requested: false,
            extras: Vec::new(),
            hooks_before: HashMap::new(),
            installed_before: None,
            connect_errors: Vec::new(),
            was_loading: false,
            connected_since: None,
            last_computer_use: ComputerUseState::Off,
            scan_log: ScanLog::new(),
            scan_log_mounted: now,
        }
    }
}

type Phase = &'static str;

impl GpuiOnboardingWindow {
    fn agents_mounted(&self) -> bool {
        self.panel == 2 && !self.flow.finished
    }

    fn add_extra(&mut self, id: String, text: String, tone: ExtraTone) {
        let now = Instant::now();
        self.agents_panel.extras.retain(|extra| extra.id != id);
        self.agents_panel.extras.push(ScanExtra {
            id,
            text,
            tone,
            at: chrono::Local::now(),
            appeared: now,
        });
    }

    fn remove_extra(&mut self, id: &str) {
        self.agents_panel.extras.retain(|extra| extra.id != id);
    }

    fn set_right_tab(&mut self, tab: RightTab) {
        if self.agents_panel.right_tab == tab {
            return;
        }
        self.agents_panel.right_tab = tab;
        self.agents_panel.right_tab_since = Instant::now();
    }

    pub(super) fn mount_agents_panel(&mut self, now: Instant, _cx: &mut Context<Self>) {
        self.agents_panel = AgentsPanelState::new(now);
        self.agents_panel.hooks_before = self
            .agents
            .iter()
            .map(|agent| (agent.agent_id.clone(), agent.hooks_installed))
            .collect();
        self.agents_panel.was_loading = self.agents_loading;
        self.agents_panel.last_computer_use = self.computer_use_state();
        self.installed_before_effect(self.agents_loading, _cx);
        self.computer_use_effect();
        self.scan_log_mount(now);
    }

    /// `props.onRescanAgents`.
    pub(super) fn rescan_agents(&mut self, cx: &mut Context<Self>) {
        let was_loading = self.agents_loading;
        self.agents_loading = true;
        self.send(OnboardingCommand::RescanAgents, cx);
        if self.agents_mounted() && !was_loading {
            let now = Instant::now();
            self.scan_log_on_loading(false, now);
            self.installed_before_effect(true, cx);
            self.agents_panel.was_loading = true;
        }
        cx.notify();
    }

    /// The panel's effects on a new detection payload, in the order React ran them.
    pub(super) fn after_agents_changed(&mut self, previous_loading: bool, cx: &mut Context<Self>) {
        if !self.agents_mounted() {
            return;
        }
        let now = Instant::now();
        // The payload lands while the host still reports loading ...
        let hooks_now: HashMap<String, bool> = self
            .agents
            .iter()
            .map(|agent| (agent.agent_id.clone(), agent.hooks_installed))
            .collect();
        let newly_connected: Vec<(String, String)> = installed_agents(&self.agents)
            .into_iter()
            .filter(|agent| {
                agent.hooks_installed
                    && self.agents_panel.hooks_before.get(&agent.agent_id) == Some(&false)
            })
            .map(|agent| (agent.agent_id.clone(), agent.name.clone()))
            .collect();
        for (agent_id, name) in newly_connected {
            self.add_extra(
                format!("connected-{agent_id}"),
                format!("Connected {name}"),
                ExtraTone::Ok,
            );
        }
        self.agents_panel.hooks_before = hooks_now;
        self.scan_log_on_agents(previous_loading, now);
        self.installed_before_effect(previous_loading, cx);
        self.connect_error_effect(previous_loading);
        // ... and then the loading marker clears.
        if previous_loading != self.agents_loading {
            self.scan_log_on_loading(previous_loading, now);
            self.installed_before_effect(self.agents_loading, cx);
            self.connect_error_effect(self.agents_loading);
        }
    }

    /// An agent installed from this panel becomes the default when the configured default is one the
    /// host knows is missing (or none is set). Only rescans after a real payload count, so the first
    /// payload of a reopen never rewrites an existing user's choice.
    fn installed_before_effect(&mut self, loading: bool, cx: &mut Context<Self>) {
        let now: Vec<String> = installed_agents(&self.agents)
            .into_iter()
            .map(|agent| agent.agent_id.clone())
            .collect();
        let before = self.agents_panel.installed_before.clone();
        if !self.agents.is_empty() && !loading {
            self.agents_panel.installed_before = Some(now.iter().cloned().collect());
        }
        let Some(before) = before else {
            return;
        };
        if loading {
            return;
        }
        if !now.iter().any(|agent_id| !before.contains(agent_id)) {
            return;
        }
        let configured = self.settings.default_prompt_agent_id.clone();
        if now.contains(&configured) {
            return;
        }
        let configured_known_missing =
            configured.is_empty() || self.agents.iter().any(|agent| agent.agent_id == configured);
        if !configured_known_missing {
            return;
        }
        let mut patch = serde_json::Map::new();
        patch.insert("defaultPromptAgentId".into(), Value::String(now[0].clone()));
        self.update_settings(patch, cx);
    }

    /// The helper install answers through one more status round trip, so its failure shows only as
    /// "loading ended and not every installed agent reports hooks": release the button and say which
    /// agent did not connect, otherwise the panel would stay on "Connecting…" forever.
    fn connect_error_effect(&mut self, loading: bool) {
        let loading_ended = self.agents_panel.was_loading && !loading;
        self.agents_panel.was_loading = loading;
        if !loading_ended || !self.agents_panel.connecting || all_installed_have_hooks(&self.agents)
        {
            return;
        }
        self.agents_panel.connecting = false;
        let mut errors = Vec::new();
        if let Some(host_error) = self
            .hook_status
            .as_ref()
            .and_then(|status| status.get("errorMessage"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|error| !error.is_empty())
        {
            errors.push(host_error.to_string());
        }
        for agent in installed_agents(&self.agents) {
            if agent.hooks_installed {
                continue;
            }
            let reason = if agent.update_required {
                "needs an update".to_string()
            } else {
                agent
                    .detail
                    .clone()
                    .filter(|detail| !detail.is_empty())
                    .unwrap_or_else(|| "could not be connected".to_string())
            };
            errors.push(format!("{}: {reason}", agent.name));
        }
        self.agents_panel.connect_errors = errors.clone();
        for (index, text) in errors.into_iter().enumerate() {
            self.add_extra(format!("connect-error-{index}"), text, ExtraTone::Wait);
        }
    }

    pub(super) fn after_computer_use_changed(
        &mut self,
        before: ComputerUseState,
        cx: &mut Context<Self>,
    ) {
        let _ = cx;
        if !self.agents_mounted() || self.computer_use_state() == before {
            return;
        }
        self.computer_use_effect();
    }

    fn computer_use_effect(&mut self) {
        let state = self.computer_use_state();
        self.agents_panel.last_computer_use = state;
        if state != ComputerUseState::Off {
            self.agents_panel.computer_use_requested = false;
        }
        match state {
            ComputerUseState::Installing => self.add_extra(
                "cu".into(),
                "Computer Use: installing…".into(),
                ExtraTone::Acc,
            ),
            ComputerUseState::Permissions => self.add_extra(
                "cu".into(),
                "Computer Use: waiting for your permission".into(),
                ExtraTone::Wait,
            ),
            ComputerUseState::On => {
                self.add_extra("cu".into(), "Computer Use: on".into(), ExtraTone::Ok)
            }
            ComputerUseState::Off => self.remove_extra("cu"),
        }
    }

    /// Every CLI install (panel rows and the Install guide) narrates itself here, then asks for a rescan.
    pub(super) fn on_install_event(&mut self, event: InstallEvent, cx: &mut Context<Self>) {
        if !self.agents_mounted() {
            return;
        }
        match event {
            InstallEvent::Started {
                agent_id,
                name,
                method_label,
            } => {
                self.set_right_tab(RightTab::Scan);
                let suffix = format!("-{agent_id}");
                self.agents_panel.extras.retain(|extra| {
                    !extra.id.starts_with("install-") || !extra.id.ends_with(&suffix)
                });
                self.add_extra(
                    format!("install-{agent_id}"),
                    format!("Installing {name} with {method_label}…"),
                    ExtraTone::Acc,
                );
            }
            InstallEvent::Output { agent_id, line } => {
                self.add_extra(format!("install-log-{agent_id}"), line, ExtraTone::Acc);
            }
            InstallEvent::Succeeded { agent_id, name } => {
                self.remove_extra(&format!("install-log-{agent_id}"));
                self.add_extra(
                    format!("install-done-{agent_id}"),
                    format!("Installed {name}"),
                    ExtraTone::Ok,
                );
                self.rescan_agents(cx);
            }
            InstallEvent::Failed {
                agent_id,
                name,
                error,
            } => {
                self.remove_extra(&format!("install-log-{agent_id}"));
                self.add_extra(
                    format!("install-error-{agent_id}"),
                    format!("{name}: {error}"),
                    ExtraTone::Wait,
                );
            }
        }
    }

    fn agents_connected(&self) -> bool {
        self.flow.integration_on && all_installed_have_hooks(&self.agents)
    }

    pub(super) fn tick_agents(&mut self, now: Instant, cx: &mut Context<Self>) {
        self.scan_log_tick(now);
        let holding = self.agents_panel.connecting && self.agents_connected();
        match (holding, self.agents_panel.connected_since) {
            (true, None) => self.agents_panel.connected_since = Some(now),
            (true, Some(since)) if now.saturating_duration_since(since) >= CONNECTED_HOLD => {
                self.agents_panel.connected_since = None;
                self.go(3, cx);
            }
            (false, Some(_)) => self.agents_panel.connected_since = None,
            _ => {}
        }
    }

    fn pick_default(&mut self, agent_id: String, cx: &mut Context<Self>) {
        let current = default_agent_id(&self.settings, &self.agents);
        if current.as_deref() != Some(agent_id.as_str()) {
            let name = self
                .agents
                .iter()
                .find(|agent| agent.agent_id == agent_id)
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| agent_id.clone());
            self.add_extra(
                "default".into(),
                format!("{name} set as your default"),
                ExtraTone::Acc,
            );
        }
        let mut patch = serde_json::Map::new();
        patch.insert("defaultPromptAgentId".into(), Value::String(agent_id));
        self.update_settings(patch, cx);
    }

    fn connect_and_continue(&mut self, cx: &mut Context<Self>) {
        let installed: Vec<String> = installed_agents(&self.agents)
            .into_iter()
            .map(|agent| agent.agent_id.clone())
            .collect();
        if !self.flow.integration_on || self.agents_connected() || installed.is_empty() {
            self.go(3, cx);
            return;
        }
        self.set_right_tab(RightTab::Scan);
        self.agents_panel.connecting = true;
        self.agents_panel.connect_errors.clear();
        self.agents_panel
            .extras
            .retain(|extra| !extra.id.starts_with("connect-error-"));
        self.flow.hooks_requested = true;
        let was_loading = self.agents_loading;
        // The modal host marks detection as loading while the helper installs.
        self.agents_loading = true;
        if !was_loading {
            let now = Instant::now();
            self.scan_log_on_loading(false, now);
            self.agents_panel.was_loading = true;
        }
        self.send(OnboardingCommand::InstallAgentHooks(installed), cx);
        cx.notify();
    }

    fn toggle_computer_use(&mut self, cx: &mut Context<Self>) {
        self.set_right_tab(RightTab::ComputerUse);
        if self.computer_use_state() != ComputerUseState::Off
            || self.agents_panel.computer_use_requested
        {
            cx.notify();
            return;
        }
        self.agents_panel.computer_use_requested = true;
        let before = self.computer_use_state();
        self.computer_use_install_requested = true;
        self.computer_use_install_deadline = Some(Instant::now() + Duration::from_secs(150));
        let install_driver = self
            .cli_status
            .as_ref()
            .and_then(|status| status.get("cuaDriverInstalled"))
            .and_then(Value::as_bool)
            != Some(true);
        self.send(OnboardingCommand::InstallComputerUse { install_driver }, cx);
        self.after_computer_use_changed(before, cx);
        self.show_toast(
            super::model::computer_use_copy(super::model::ComputerUseDesktop::current()).turned_on,
            cx,
        );
    }

    pub(super) fn render_agents(
        &mut self,
        s: S,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let _ = window;
        let installed = installed_agents(&self.agents)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let missing_primary: Vec<(String, String)> = PRIMARY_AGENTS
            .iter()
            .filter(|(agent_id, _)| {
                !self
                    .agents
                    .iter()
                    .any(|agent| agent.agent_id == *agent_id && agent.installed)
            })
            .map(|(agent_id, _)| {
                (
                    agent_id.to_string(),
                    catalog_agent_name(&self.agents, &self.catalog, agent_id),
                )
            })
            .collect();
        for (agent_id, name) in &missing_primary {
            self.ensure_cli_row(&format!("row:{agent_id}"), agent_id, name, false, cx);
        }
        let default_agent = default_agent_id(&self.settings, &self.agents);
        let connected = self.agents_connected();
        let integration_on = self.flow.integration_on;
        let scanning = self.agents_loading;
        let connecting = self.agents_panel.connecting;
        let computer_use_state = self.computer_use_state();
        let other_missing: Vec<_> = self
            .agents
            .iter()
            .filter(|agent| {
                !agent.installed && !PRIMARY_AGENTS.iter().any(|(id, _)| *id == agent.agent_id)
            })
            .cloned()
            .collect();
        let list_rows = installed.len() + missing_primary.len();
        let compact = list_rows > 3;
        let row_height = if compact {
            ((228.0 - 8.0 * (list_rows as f32 - 1.0)) / list_rows as f32)
                .floor()
                .max(50.0)
        } else {
            68.0
        };

        let mut out = Vec::new();
        out.push(eyebrow(s, 48.0, 120.0, None, "Agents").into_any_element());
        out.push(
            heading(
                s,
                48.0,
                150.0,
                720.0,
                46.0,
                "Use the agents you already have.",
                None,
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 48.0, 212.0, 720.0, 16.0, false)
                .child("Ghostex finds the agents already installed, installs the ones you are missing, and asks which one should be your default.")
                .into_any_element(),
        );
        let mut list = abs(s, 48.0, 264.0, Some(706.0), Some(228.0))
            .id("agent-list")
            .flex()
            .flex_col()
            .gap(s.px(8.0))
            .overflow_y_scroll();
        for agent in &installed {
            let selected = default_agent.as_deref() == Some(agent.agent_id.as_str());
            let (pill_tone, pill_text) = if scanning {
                (DetTone::Wait, "Scanning…")
            } else if integration_on && agent.hooks_installed {
                (DetTone::On, "Connected")
            } else {
                (DetTone::Detected, "Detected")
            };
            let agent_id = agent.agent_id.clone();
            let detail = agent.detail.clone();
            let subtitle = match &agent.account_label {
                Some(label) => format!("Installed • {label}"),
                None => "Installed".to_string(),
            };
            let key = format!("agent-row-{}", agent.agent_id);
            let row = agent_row(s, &key, selected, scanning, compact, row_height)
                .id(SharedString::from(key.clone()))
                .cursor_pointer()
                .when_some(detail, |this, detail| {
                    this.tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
                })
                .child(radio(s, &key, selected))
                .child(agent_box(s, compact).child(agent_logo(
                    s,
                    &self.catalog,
                    &agent.agent_id,
                    28.0,
                )))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 17.0, 500.0).child(agent.name.clone()))
                        .child(ss(s, 12.5).child(subtitle)),
                )
                .child(detpill(s, pill_text, pill_tone, DETPILL_DEFAULT));
            list = list.child(self.control(
                s,
                row,
                key,
                interact::Ring::new(14.0, 1.0),
                interact::Keys::Space,
                cx,
                move |this, _, cx| this.pick_default(agent_id.clone(), cx),
            ));
        }
        for (agent_id, name) in &missing_primary {
            list = list.child(
                self.missing_agent_row(s, now, agent_id, name, scanning, compact, row_height, cx),
            );
        }
        out.push(list.into_any_element());

        let other_names: Vec<String> = other_missing
            .iter()
            .take(3)
            .map(|agent| agent.name.clone())
            .collect();
        // `.arow3.other`: `transition: border-color 0.25s`, `:hover` border.
        let other_border = interact::hover_color(
            "agents-other",
            "border",
            rgba(150, 165, 205, 0.12),
            rgba(180, 195, 235, 0.24),
            250,
        );
        let other_row = glass(s)
            .absolute()
            .left(s.px(48.0))
            .top(s.px(492.0))
            .w(s.px(706.0))
            .h(s.px(68.0))
            .flex()
            .items_center()
            .pl(s.px(22.0))
            .pr(s.px(20.0))
            .id("agents-other")
            .border_color(other_border)
            .cursor_pointer()
            .child(
                div()
                    .size(s.px(18.0))
                    .flex_none()
                    .mr(s.px(18.0))
                    .rounded_full()
                    .border(s.px(1.5))
                    .border_dashed()
                    .border_color(white(0.18))
                    .opacity(0.6),
            )
            .child(
                div()
                    .h(s.px(48.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(s.px(10.0))
                    .mr(s.px(16.0))
                    .rounded(s.px(12.0))
                    .border_1()
                    .border_color(white(0.08))
                    .bg(white(0.03))
                    .child(other_agents_strip(
                        s,
                        &self.catalog,
                        19.0,
                        other_missing.len(),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(nm(s, 17.0, 500.0).child(if other_missing.is_empty() {
                        "Other agents".to_string()
                    } else {
                        format!("Other agents (+{})", other_missing.len())
                    }))
                    .child(ss(s, 12.5).child(if other_names.is_empty() {
                        "Pi Agent, OpenCode, Gemini, and more.".to_string()
                    } else {
                        format!("{}, and more.", other_names.join(", "))
                    })),
            )
            // `tabIndex={-1}`: the row is the control; the button only looks like one.
            .child(interact::watch_hover(
                guide_button(s, "agents-other-guide"),
                "agents-other-guide",
            ));
        out.push(
            self.control(
                s,
                other_row,
                "agents-other",
                interact::Ring::new(14.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.open_install_guide(cx),
            )
            .into_any_element(),
        );
        let integration_toggle = self.control(
            s,
            toggle(
                s,
                "agents-integration-toggle",
                integration_on,
                self.thumb("integration", integration_on, now),
                ToggleSize::Lg,
                false,
            ),
            "agents-integration-toggle",
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.flow.integration_on = !this.flow.integration_on;
                this.set_right_tab(RightTab::Integration);
                cx.notify();
            },
        );
        let integration_row = switch_row(s, "agents-integration", 576.0, 72.0, (20.0, 30.0), 26.0, None)
                .child(icon(s, "pulse", 22.0, 1.6, hex(0xd6dbe5)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 17.0, 500.0).child("Ghostex integration"))
                        .child(ss(s, 12.5).child(
                            "Adds a small Ghostex helper to each agent's own settings, so you see its live status and can resume it.",
                        )),
                )
                .child(integration_toggle);
        out.push(
            self.control(
                s,
                integration_row,
                "agents-integration",
                interact::Ring::new(14.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| {
                    this.set_right_tab(RightTab::Integration);
                    cx.notify();
                },
            )
            .into_any_element(),
        );
        let computer_use_on =
            computer_use_state != ComputerUseState::Off || self.agents_panel.computer_use_requested;
        let perm_pill = match computer_use_state {
            ComputerUseState::Installing => Some(
                perm_pill(s, false)
                    .child(spinner(s, 12.0, 1.5, self.opened_at, now))
                    .child(div().ml(s.px(6.0)).child("Installing…")),
            ),
            ComputerUseState::Permissions => Some(perm_pill(s, false).child("Needs OS permission")),
            ComputerUseState::On => Some(perm_pill(s, true).child("On")),
            ComputerUseState::Off if self.agents_panel.computer_use_requested => {
                Some(perm_pill(s, false).child("Needs OS permission"))
            }
            ComputerUseState::Off => None,
        };
        let computer_use_toggle = self.control(
            s,
            toggle(
                s,
                "agents-computer-use-toggle",
                computer_use_on,
                self.thumb("computer-use", computer_use_on, now),
                ToggleSize::Md,
                false,
            ),
            "agents-computer-use-toggle",
            interact::Ring::new(16.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.toggle_computer_use(cx);
            },
        );
        let computer_use_row = switch_row(
            s,
            "agents-computer-use",
            660.0,
            72.0,
            (18.0, 30.0),
            18.0,
            computer_use_on.then(|| rgba(110, 150, 255, 0.42)),
        )
        .child(icon(s, "monitor", 22.0, 1.6, hex(0xd6dbe5)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(nm(s, 17.0, 500.0).child("Computer Use"))
                .child(
                    ss(s, 12.5)
                        .child("Let agents use apps outside Ghostex, off until you allow it."),
                ),
        )
        .children(perm_pill)
        .child(computer_use_toggle);
        out.push(
            self.control(
                s,
                computer_use_row,
                "agents-computer-use",
                interact::Ring::new(14.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| {
                    this.set_right_tab(RightTab::ComputerUse);
                    cx.notify();
                },
            )
            .into_any_element(),
        );
        let next_label = if !integration_on || connected || installed.is_empty() {
            "Next"
        } else if connecting {
            "Connecting…"
        } else {
            "Connect & continue"
        };
        out.push(
            foot_actions(s, 2)
                .when(
                    integration_on && !connected && !installed.is_empty(),
                    |this| {
                        this.child(self.control(
                            s,
                            ghost(s, "agents-skip", "Skip for now", 16.0, false),
                            "agents-skip",
                            interact::Ring::new(0.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| this.go(3, cx),
                        ))
                    },
                )
                .child({
                    let next = cta(
                        s,
                        "agents-next",
                        next_label,
                        true,
                        true,
                        scanning || connecting,
                        CtaSize::Foot,
                    );
                    // A disabled button is out of the tab order.
                    if scanning || connecting {
                        next
                    } else {
                        self.control(
                            s,
                            next,
                            "agents-next",
                            interact::Ring::new(10.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| {
                                if !(this.agents_loading || this.agents_panel.connecting) {
                                    this.connect_and_continue(cx);
                                }
                            },
                        )
                    }
                })
                .into_any_element(),
        );
        if !self.agents_panel.connect_errors.is_empty() && !connected {
            let joined = self.agents_panel.connect_errors.join("\n");
            out.push(
                abs(s, 48.0, 812.0, Some(706.0), None)
                    .id("agents-connect-errors")
                    .font_family(super::fonts::dm_sans())
                    .text_size(s.px(13.0))
                    .line_height(s.px(18.0))
                    .text_color(hex(0xff6b62))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip(move |window, cx| Tooltip::new(joined.clone()).build(window, cx))
                    .child(self.agents_panel.connect_errors.join(" · "))
                    .into_any_element(),
            );
        }
        let right_tab = self.agents_panel.right_tab;
        out.push(
            abs(s, 885.0, 122.0, Some(700.0), Some(40.0))
                .flex()
                .items_center()
                .gap(s.px(4.0))
                .children(
                    [
                        (RightTab::Scan, "terminal", "Scan"),
                        (RightTab::Integration, "pulse", "Ghostex integration"),
                        (RightTab::ComputerUse, "monitor", "Computer Use"),
                    ]
                    .into_iter()
                    .map(|(tab, icon_name, label)| {
                        let key = SharedString::from(format!("agents-tab-{label}"));
                        self.control(
                            s,
                            window_tab(
                                s,
                                key.clone(),
                                icon_name,
                                label,
                                tab == right_tab,
                                34.0,
                                13.0,
                            ),
                            key,
                            interact::Ring::new(8.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                this.set_right_tab(tab);
                                cx.notify();
                            },
                        )
                    })
                    .collect::<Vec<_>>(),
                )
                .into_any_element(),
        );
        match right_tab {
            RightTab::Scan => {
                out.push(self.render_scan_log(s, now, default_agent.as_deref(), connecting, cx));
            }
            RightTab::Integration => out.push(self.render_integration_panel(s, now)),
            RightTab::ComputerUse => out.push(self.render_computer_use_panel(s, now, cx)),
        }
        // The tracker follows the scan console's own "Scanning…" / "Complete", so the two never
        // disagree while the console is still revealing lines.
        let scanning = self.scan_log_scanning(now);
        let phase_search: Phase = if scanning { "active" } else { "done" };
        let phase_found: Phase = if scanning {
            "idle"
        } else if connected || connecting {
            "done"
        } else {
            "here"
        };
        let phase_connected: Phase = if connected && !scanning {
            "here"
        } else if connecting {
            "active"
        } else {
            "idle"
        };
        let phases = [
            ("Searching", phase_search),
            ("Found", phase_found),
            ("Connected", phase_connected),
        ];
        let mut track = abs(s, 900.0, 772.0, Some(670.0), Some(60.0))
            .flex()
            .items_center();
        for (index, (label, phase)) in phases.into_iter().enumerate() {
            if index > 0 {
                track = track.child(div().flex_1().h(s.px(2.0)).mx(s.px(6.0)).map(|this| {
                    if phase != "idle" {
                        this.bg(linear_gradient(
                            90.0,
                            linear_color_stop(rgba(59, 227, 162, 0.6), 0.0),
                            linear_color_stop(rgba(110, 150, 255, 0.7), 1.0),
                        ))
                    } else {
                        this.bg(white(0.08))
                    }
                }));
            }
            track = track.child(track_chip(s, label, phase, self.opened_at, now));
        }
        out.push(track.into_any_element());
        if self.agents_panel.guide_open {
            out.push(self.render_install_guide(s, now, true, cx));
        }
        out
    }

    /// A thumb position that eases like `.tg::after` (`left 0.22s cubic-bezier(0.3, 0.8, 0.3, 1)`).
    pub(super) fn thumb(&self, id: &str, on: bool, now: Instant) -> f32 {
        self.transitions.value(
            &format!("thumb-{id}"),
            if on { 1.0 } else { 0.0 },
            (Duration::from_millis(220), Ease::Bezier(0.3, 0.8, 0.3, 1.0)),
            now,
        )
    }

    fn open_install_guide(&mut self, cx: &mut Context<Self>) {
        self.agents_panel.guide_open = true;
        self.agents_panel.guide_opened_at = Instant::now();
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    fn missing_agent_row(
        &self,
        s: S,
        now: Instant,
        agent_id: &str,
        name: &str,
        scanning: bool,
        compact: bool,
        row_height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("row:{agent_id}");
        let row = self.cli_rows.get(&key);
        let error = row.and_then(|row| row.error());
        let running = row.is_some_and(|row| row.running());
        let queued = row.is_some_and(|row| row.queued());
        let installed = row.is_some_and(|row| row.installed());
        let path_directory = row.and_then(|row| row.path_directory()).map(str::to_string);
        let method = row.and_then(|row| row.method()).cloned();
        let checking = row.is_some_and(|row| row.checking());
        let subtitle = if let Some(error) = &error {
            error.clone()
        } else if queued {
            "Waiting for the other installs…".to_string()
        } else if running {
            match &method {
                Some(method) => format!("Installing with {}…", method.label),
                None => "Installing…".to_string(),
            }
        } else if installed && path_directory.is_some() {
            "Installed, but new terminals cannot find it".to_string()
        } else if installed {
            "Installed, press Rescan".to_string()
        } else if self
            .agents
            .iter()
            .any(|agent| agent.agent_id == agent_id && agent.installs_on_demand)
        {
            // CDXC:Onboarding 2026-10-28 WHY: an install-on-first-run launcher (Omarchy's mise wrappers) is not an install; gxserver reports it apart so setup neither calls it Installed nor hides why a command already answers to the name.
            "Not installed yet · installs itself the first time you start it".to_string()
        } else if let Some(method) = &method {
            format!("Not installed · {}", method.display_label())
        } else {
            "Not installed".to_string()
        };
        let action: Option<AnyElement> = if scanning {
            Some(detpill(s, "Scanning…", DetTone::Wait, DETPILL_DEFAULT).into_any_element())
        } else if running {
            Some(
                detpill(
                    s,
                    div()
                        .flex()
                        .items_center()
                        .child(div().mr(s.px(8.0)).child(spinner(
                            s,
                            14.0,
                            1.5,
                            self.opened_at,
                            now,
                        )))
                        .child(if queued {
                            "Waiting…"
                        } else {
                            "Installing…"
                        }),
                    DetTone::Wait,
                    DETPILL_DEFAULT,
                )
                .into_any_element(),
            )
        } else if checking {
            Some(detpill(s, "Checking…", DetTone::Wait, DETPILL_DEFAULT).into_any_element())
        } else if let (true, Some(directory)) = (installed, path_directory.clone()) {
            let key = key.clone();
            let id = SharedString::from(format!("add-path-{agent_id}"));
            Some(
                self.control(
                    s,
                    install_button(
                        s,
                        id.clone(),
                        "plus",
                        "Add to PATH",
                        false,
                        INSTALL_BUTTON_DEFAULT,
                    )
                    .tooltip(move |window, cx| {
                        Tooltip::new(format!("Add {directory} to your PATH")).build(window, cx)
                    }),
                    id,
                    interact::Ring::new(9.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.cli_add_to_path(&key, cx),
                )
                .into_any_element(),
            )
        } else if installed {
            None
        } else if let (true, Some(method)) = (self.cli_available, method.clone()) {
            let disabled = method.unavailable_reason.is_some();
            let title = method.tooltip();
            let key = key.clone();
            let id = SharedString::from(format!("install-{agent_id}"));
            let button = install_button(
                s,
                id.clone(),
                if error.is_some() { "refresh" } else { "plus" },
                if error.is_some() { "Retry" } else { "Install" },
                disabled,
                INSTALL_BUTTON_DEFAULT,
            )
            .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx));
            Some(if disabled {
                button.into_any_element()
            } else {
                self.control(
                    s,
                    button,
                    id,
                    interact::Ring::new(9.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.cli_install(&key, cx),
                )
                .into_any_element()
            })
        } else {
            let id = SharedString::from(format!("guide-{agent_id}"));
            Some(
                self.control(
                    s,
                    guide_button(s, id.clone()),
                    id,
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.open_install_guide(cx),
                )
                .into_any_element(),
            )
        };
        let error_title = error.clone();
        let row_key = format!("missing-row-{agent_id}");
        agent_row(s, &row_key, false, scanning, compact, row_height)
            .id(SharedString::from(row_key.clone()))
            .map(|this| interact::watch_hover(this, row_key))
            .child(div().w(s.px(18.0)).mr(s.px(18.0)).flex_none())
            .child(agent_box(s, compact).child(agent_logo(s, &self.catalog, agent_id, 28.0)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(nm(s, 17.0, 500.0).child(name.to_string()))
                    .child(
                        ss(s, 12.5)
                            .id(SharedString::from(format!("missing-sub-{agent_id}")))
                            .when(error.is_some(), |this| {
                                this.text_color(hex(0xff6b62))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                            })
                            .when_some(error_title, |this, title| {
                                this.tooltip(move |window, cx| {
                                    Tooltip::new(title.clone()).build(window, cx)
                                })
                            })
                            .child(subtitle),
                    ),
            )
            .children(action)
            .into_any_element()
    }
}

/// `.arow3`: `transition: border-color 0.25s, background 0.25s, opacity 0.3s` (the backgrounds
/// are gradients, which swap at once).
fn agent_row(
    s: S,
    key: &str,
    selected: bool,
    pending: bool,
    compact: bool,
    row_height: f32,
) -> Div {
    let _ = compact;
    let border = interact::tween_color(
        key,
        "border",
        if selected {
            rgba(170, 188, 240, 0.5)
        } else if interact::hovered(key) {
            rgba(180, 195, 235, 0.24)
        } else {
            rgba(150, 165, 205, 0.12)
        },
        250,
    );
    let opacity = interact::tween_value(key, "opacity", if pending { 0.5 } else { 1.0 }, 300);
    glass(s)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .h(s.px(row_height))
        .pl(s.px(22.0))
        .pr(s.px(20.0))
        .border_color(border)
        .when(selected, |this| {
            this.bg(linear_gradient(
                180.0,
                linear_color_stop(rgba(28, 33, 48, 0.88), 0.0),
                linear_color_stop(rgba(14, 17, 26, 0.88), 1.0),
            ))
            .shadow(vec![
                inset_shadow(white(0.06), 0.0, 1.0, 0.0, 0.0, s),
                shadow(rgba(60, 100, 255, 0.08), 0.0, 0.0, 30.0, 0.0, s),
            ])
        })
        .opacity(opacity)
}

fn agent_box(s: S, compact: bool) -> Div {
    if compact {
        abox(s, 40.0, 12.0, 14.0)
    } else {
        abox(s, 48.0, 12.0, 16.0)
    }
}

/// `.arow3 .radio`: `transition: border-color 0.2s`.
fn radio(s: S, key: &str, selected: bool) -> Div {
    let border = interact::tween_color(
        key,
        "radio",
        if selected { hex(0xe8ecf4) } else { white(0.18) },
        200,
    );
    div()
        .size(s.px(18.0))
        .flex_none()
        .mr(s.px(18.0))
        .rounded_full()
        .border(s.px(1.5))
        .border_color(border)
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |this| {
            this.child(
                div()
                    .size(s.px(18.0 - 3.0 - 7.0))
                    .rounded_full()
                    .bg(hex(0xe8ecf4)),
            )
        })
}

/// `.vrow` / `.curow`: a glass row with an icon, copy and a trailing switch.
/// `transition: border-color 0.25s`; `on_border` is `.curow.on`, which outranks `:hover`.
pub(crate) fn switch_row(
    s: S,
    id: &'static str,
    top: f32,
    height: f32,
    (right, left): (f32, f32),
    gap: f32,
    on_border: Option<gpui::Hsla>,
) -> gpui::Stateful<Div> {
    let border = interact::tween_color(
        id,
        "border",
        on_border.unwrap_or_else(|| {
            if interact::hovered(id) {
                rgba(180, 195, 235, 0.22)
            } else {
                rgba(150, 165, 205, 0.12)
            }
        }),
        250,
    );
    glass(s)
        .absolute()
        .left(s.px(48.0))
        .top(s.px(top))
        .w(s.px(706.0))
        .h(s.px(height))
        .flex()
        .items_center()
        .gap(s.px(gap))
        .pl(s.px(left))
        .pr(s.px(right))
        .border_color(border)
        .id(id)
        .cursor_pointer()
}

fn perm_pill(s: S, on: bool) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(s.px(24.0))
        .px(s.px(10.0))
        .rounded(s.px(7.0))
        .border_1()
        .font_family(super::fonts::dm_sans())
        .text_size(s.px(11.5))
        .whitespace_nowrap()
        .map(|this| {
            if on {
                this.text_color(hex(0x3be3a2))
                    .border_color(rgba(59, 227, 162, 0.4))
                    .bg(rgba(59, 227, 162, 0.07))
            } else {
                this.text_color(hex(0xc8d2ee))
                    .border_color(rgba(150, 175, 255, 0.3))
                    .bg(rgba(60, 90, 200, 0.12))
            }
        })
}

/// `.wtab`: a titlebar-style tab with an icon. `transition: background 0.2s, color 0.2s`.
pub(crate) fn window_tab(
    s: S,
    id: SharedString,
    icon_name: &str,
    label: &str,
    on: bool,
    height: f32,
    font: f32,
) -> gpui::Stateful<Div> {
    let hover = interact::hovered(&id);
    let color = interact::tween_color(
        &id,
        "color",
        if on || hover {
            gpui::white()
        } else {
            hex(0x9aa3b3)
        },
        200,
    );
    let bg = interact::tween_color(
        &id,
        "bg",
        if on {
            white(0.08)
        } else if hover {
            white(0.04)
        } else {
            white(0.0)
        },
        200,
    );
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(7.0))
        .h(s.px(height))
        .px(s.px(if height >= 34.0 { 13.0 } else { 11.0 }))
        .rounded(s.px(8.0))
        .font_family(super::fonts::dm_sans())
        .text_size(s.px(font))
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(color)
        .bg(bg)
        .when(on, |this| {
            this.shadow(vec![inset_shadow(
                rgba(110, 150, 255, 0.3),
                0.0,
                0.0,
                0.0,
                1.0,
                s,
            )])
        })
        .child(icon(
            s,
            icon_name,
            14.0,
            1.6,
            if on { hex(0x6d93ff) } else { color },
        ))
        .child(label.to_string())
}

fn track_chip(s: S, label: &str, phase: Phase, epoch: Instant, now: Instant) -> AnyElement {
    let (color, border, background, glow) = match phase {
        "active" => (
            hex(0xeef1f7),
            rgba(110, 150, 255, 0.6),
            rgba(24, 32, 58, 0.85),
            Some(rgba(60, 100, 255, 0.2)),
        ),
        "done" => (
            hex(0xeef1f7),
            rgba(59, 227, 162, 0.35),
            rgba(10, 12, 20, 0.72),
            None,
        ),
        "here" => (
            gpui::white(),
            rgba(59, 227, 162, 0.55),
            rgba(20, 40, 40, 0.6),
            Some(rgba(59, 227, 162, 0.16)),
        ),
        _ => (hex(0x7c8598), white(0.07), rgba(10, 12, 20, 0.72), None),
    };
    div()
        .w(s.px(190.0))
        .h(s.px(56.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap(s.px(12.0))
        .rounded(s.px(12.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .when_some(glow, |this, glow| {
            this.shadow(vec![shadow(glow, 0.0, 0.0, 24.0, 0.0, s)])
        })
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(500.0))
        .text_size(s.px(16.0))
        .text_color(color)
        .map(|this| match phase {
            "active" => this.child(spinner(s, 18.0, 2.0, epoch, now)),
            "idle" => this.child(
                div()
                    .size(s.px(18.0))
                    .rounded_full()
                    .border(s.px(1.5))
                    .border_color(white(0.2)),
            ),
            _ => this.child(icon(s, "checkCircle", 20.0, 1.7, hex(0x3be3a2))),
        })
        .child(label.to_string())
        .into_any_element()
}
