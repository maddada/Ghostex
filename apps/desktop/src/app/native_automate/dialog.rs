//! The create/edit automation dialog, ported from the deleted React
//! apps/desktop/views/project-board/automation-dialog.tsx (in git history), drawn with the app modals' kit
//! (window/native_modal_kit/) in its own child window. This file owns its state and what its
//! controls do; dialog_render.rs lays it out.

use super::drafts::{
    AutomationDraft, ExecutionKind, SCHEDULE_PRESETS, ScheduleMode, TIMER_UNITS, WEEKDAYS,
};
use super::model::{AutomationSessionOption, AutomationState};
use super::view::NativeAutomateView;
use crate::app::window::{ModalPalette, ModalScrollFit, ModalSelect, ModalSelectKey};
use ghostex_gx_chat_core::menus::catalog::{AgentModelCatalog, CatalogAgent};
use gpui::{
    AppContext as _, Context, Entity, FocusHandle, KeyDownEvent, Subscription, WeakEntity, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};

pub(crate) struct AutomationDialogConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) all_projects: bool,
    pub(crate) project_name: String,
    pub(crate) state: AutomationState,
    pub(crate) sessions: Vec<AutomationSessionOption>,
    pub(crate) draft: AutomationDraft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogSelect {
    Project,
    Agent,
    Model,
    Effort,
    Repeat,
    Weekday,
    TimerUnit,
    Session,
}

impl DialogSelect {
    pub(crate) const ALL: [Self; 8] = [
        Self::Project,
        Self::Agent,
        Self::Model,
        Self::Effort,
        Self::Repeat,
        Self::Weekday,
        Self::TimerUnit,
        Self::Session,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }

    pub(crate) fn trigger_id(self) -> &'static str {
        match self {
            Self::Project => "automation-dialog-project",
            Self::Agent => "automation-dialog-agent",
            Self::Model => "automation-dialog-model",
            Self::Effort => "automation-dialog-effort",
            Self::Repeat => "automation-dialog-repeat",
            Self::Weekday => "automation-dialog-weekday",
            Self::TimerUnit => "automation-dialog-timer-unit",
            Self::Session => "automation-dialog-session",
        }
    }

    pub(crate) fn menu_id(self) -> &'static str {
        match self {
            Self::Project => "automation-dialog-project-menu",
            Self::Agent => "automation-dialog-agent-menu",
            Self::Model => "automation-dialog-model-menu",
            Self::Effort => "automation-dialog-effort-menu",
            Self::Repeat => "automation-dialog-repeat-menu",
            Self::Weekday => "automation-dialog-weekday-menu",
            Self::TimerUnit => "automation-dialog-timer-unit-menu",
            Self::Session => "automation-dialog-session-menu",
        }
    }
}

/// One single-line text field of the form and the draft string it writes. The prompt is the
/// form's one text area, [`AutomationDialog::prompt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogField {
    Name,
    ScheduleTime,
    TimerAmount,
    RunAt,
    Cron,
    SetupCommand,
    ExpiresAt,
}

impl DialogField {
    const ALL: [Self; 7] = [
        Self::Name,
        Self::ScheduleTime,
        Self::TimerAmount,
        Self::RunAt,
        Self::Cron,
        Self::SetupCommand,
        Self::ExpiresAt,
    ];

    fn slot(self, draft: &mut AutomationDraft) -> &mut String {
        match self {
            Self::Name => &mut draft.name,
            Self::ScheduleTime => &mut draft.schedule_time,
            Self::TimerAmount => &mut draft.timer_amount,
            Self::RunAt => &mut draft.run_at,
            Self::Cron => &mut draft.cron_expression,
            Self::SetupCommand => &mut draft.setup_command,
            Self::ExpiresAt => &mut draft.expires_at,
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::ScheduleTime => "HH:MM",
            Self::RunAt | Self::ExpiresAt => "YYYY-MM-DD HH:MM",
            Self::Cron => "*/15 * * * *",
            Self::SetupCommand => "Use project worktree command",
            _ => "",
        }
    }
}

pub(crate) struct AutomationDialog {
    pub(crate) view: WeakEntity<NativeAutomateView>,
    pub(crate) palette: ModalPalette,
    pub(crate) all_projects: bool,
    pub(crate) project_name: String,
    pub(crate) state: AutomationState,
    pub(crate) sessions: Vec<AutomationSessionOption>,
    pub(crate) draft: AutomationDraft,
    /// This computer's agent model catalog, the lineup the New Coordinator dialog offers.
    pub(crate) catalog: Option<AgentModelCatalog>,
    pub(crate) inputs: Vec<(DialogField, Entity<InputState>)>,
    /// The Prompt text area, which writes `draft.prompt`.
    pub(crate) prompt: Entity<TextareaState>,
    pub(crate) selects: Vec<ModalSelect>,
    pub(crate) saving: bool,
    pub(crate) error: Option<String>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) fit: ModalScrollFit,
    _subscriptions: Vec<Subscription>,
}

impl AutomationDialog {
    pub(crate) fn new(
        config: AutomationDialogConfig,
        view: WeakEntity<NativeAutomateView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut inputs = Vec::new();
        let mut subscriptions = Vec::new();
        for field in DialogField::ALL {
            let mut draft = config.draft.clone();
            let value = field.slot(&mut draft).clone();
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(field.placeholder())
                    .default_value(value)
            });
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        *field.slot(&mut this.draft) = input.read(cx).value().to_string();
                        this.error = None;
                        cx.notify();
                    }
                },
            ));
            inputs.push((field, input));
        }
        let prompt =
            cx.new(|cx| TextareaState::new(window, cx).default_value(config.draft.prompt.clone()));
        subscriptions.push(cx.subscribe_in(
            &prompt,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.prompt = input.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        ));
        if let Some((_, name)) = inputs.first() {
            name.update(cx, |input, cx| input.focus(window, cx));
        }
        Self {
            view,
            palette: config.palette,
            all_projects: config.all_projects,
            project_name: config.project_name,
            state: config.state,
            sessions: config.sessions,
            draft: config.draft,
            catalog: crate::app::gx_chat::current_model_catalog(),
            inputs,
            prompt,
            selects: DialogSelect::ALL
                .iter()
                .map(|_| ModalSelect::new())
                .collect(),
            saving: false,
            error: None,
            focus_handle: cx.focus_handle(),
            fit: ModalScrollFit::new(),
            _subscriptions: subscriptions,
        }
    }

    pub(crate) fn input(&self, field: DialogField) -> &Entity<InputState> {
        &self
            .inputs
            .iter()
            .find(|(candidate, _)| *candidate == field)
            .expect("every dialog field has an input")
            .1
    }

    pub(crate) fn select(&self, kind: DialogSelect) -> &ModalSelect {
        &self.selects[kind.index()]
    }

    /// Worktree mode needs a project that can host worktrees (`automationDraftCanUseWorktrees`).
    pub(crate) fn can_use_worktrees(&self) -> bool {
        if self.all_projects {
            self.state
                .project(&self.draft.project_id)
                .is_some_and(|project| project.can_use_worktrees)
        } else {
            self.state.project_can_use_worktrees
        }
    }

    pub(crate) fn worktree_unavailable_reason(&self) -> Option<String> {
        if self.all_projects {
            self.state
                .project(&self.draft.project_id)
                .and_then(|project| project.worktree_unavailable_reason.clone())
        } else {
            self.state.worktree_unavailable_reason.clone()
        }
    }

    /// The catalog lineup of the chosen agent: by its id, else by its icon (a custom agent that
    /// runs Claude or Codex carries that agent's icon).
    fn catalog_agent(&self) -> Option<&CatalogAgent> {
        let agents = &self.catalog.as_ref()?.agents;
        let option = self.state.agent(&self.draft.agent_id);
        agents.get(self.draft.agent_id.as_str()).or_else(|| {
            option
                .and_then(|agent| agent.icon.as_deref())
                .and_then(|icon| agents.get(icon))
        })
    }

    /// Whether the Model and Effort pickers show: not for a thread run, whose session keeps its
    /// own, and not for an agent the catalog has no lineup for unless a choice is already saved.
    pub(crate) fn shows_launch_choice(&self) -> bool {
        self.draft.execution_kind != ExecutionKind::Thread
            && (self
                .catalog_agent()
                .is_some_and(|agent| !agent.models.is_empty())
                || !self.draft.model.trim().is_empty()
                || !self.draft.effort.trim().is_empty())
    }

    /// Concrete default model `(value, label)` for the chosen agent from the catalog.
    pub(crate) fn resolved_default_model(&self) -> Option<(String, String)> {
        let agent = self.catalog_agent()?;
        let family = self
            .state
            .agent(&self.draft.agent_id)
            .and_then(|a| a.icon.as_deref())
            .unwrap_or(&self.draft.agent_id);
        ghostex_gx_chat_core::menus::catalog::preferred_default_model(family, agent)
            .map(|m| (m.value.clone(), m.label.clone()))
    }

    /// Concrete default model label shown when no explicit override is picked yet.
    pub(crate) fn default_model_label(&self) -> String {
        self.resolved_default_model()
            .map(|(_, label)| label)
            .unwrap_or_else(|| "Model".to_string())
    }

    /// Concrete default effort label shown when no explicit effort override is picked yet.
    pub(crate) fn default_effort_label(&self) -> String {
        let Some(agent) = self.catalog_agent() else {
            return "Medium".to_string();
        };
        let family = self
            .state
            .agent(&self.draft.agent_id)
            .and_then(|a| a.icon.as_deref())
            .unwrap_or(&self.draft.agent_id);
        let model_val = self.draft.model.trim();
        let model = if model_val.is_empty() {
            ghostex_gx_chat_core::menus::catalog::preferred_default_model(family, agent)
        } else {
            agent.model(model_val)
        };
        let effort = model
            .and_then(|m| {
                ghostex_gx_chat_core::menus::catalog::preferred_default_effort(family, agent, m)
            })
            .or_else(|| agent.default_effort.clone())
            .or_else(|| agent.efforts.first().cloned());
        match effort {
            Some(eff) => self
                .catalog
                .as_ref()
                .map(|c| c.effort_label(&eff))
                .unwrap_or(eff),
            None => "Medium".to_string(),
        }
    }

    /// The Model picker's concrete models (never "Agent default" or "Auto"), plus any saved one
    /// not listed by the catalog so an edit never drops it.
    fn model_values(&self) -> Vec<(String, String)> {
        let mut values = self
            .catalog_agent()
            .map(|agent| {
                agent
                    .models
                    .iter()
                    .filter(|model| {
                        !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(
                            &model.value,
                        ) && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_label(
                            &model.label,
                        )
                    })
                    .map(|model| (model.value.clone(), model.label.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let saved = self.draft.model.trim();
        if !saved.is_empty() && !values.iter().any(|(value, _)| value == saved) {
            values.push((saved.to_string(), saved.to_string()));
        }
        values
    }

    /// The Effort picker's concrete values: the chosen model's efforts (or the default model's
    /// efforts while no model override is saved), plus any saved one not among them.
    fn effort_values(&self) -> Vec<(String, String)> {
        let label = |effort: &str| {
            self.catalog
                .as_ref()
                .map(|catalog| catalog.effort_label(effort))
                .unwrap_or_else(|| effort.to_string())
        };
        let mut values = self
            .catalog_agent()
            .map(|agent| {
                let model = self.draft.model.trim();
                if model.is_empty() {
                    if let Some((default_val, _)) = self.resolved_default_model() {
                        agent.efforts_for_model(&default_val)
                    } else {
                        agent.efforts.clone()
                    }
                } else {
                    agent.efforts_for_model(model)
                }
            })
            .unwrap_or_default()
            .into_iter()
            .map(|effort| {
                let shown = label(&effort);
                (effort, shown)
            })
            .collect::<Vec<_>>();
        let saved = self.draft.effort.trim();
        if !saved.is_empty() && !values.iter().any(|(value, _)| value == saved) {
            values.push((saved.to_string(), label(saved)));
        }
        values
    }

    fn launch_values(&self, kind: DialogSelect) -> Vec<(String, String)> {
        match kind {
            DialogSelect::Model => self.model_values(),
            _ => self.effort_values(),
        }
    }

    pub(crate) fn items(&self, kind: DialogSelect) -> Vec<String> {
        match kind {
            DialogSelect::Project => self
                .state
                .projects
                .iter()
                .map(|project| project.label.clone())
                .collect(),
            DialogSelect::Agent => self
                .state
                .agents
                .iter()
                .map(|agent| agent.display_label().to_string())
                .collect(),
            DialogSelect::Model | DialogSelect::Effort => self
                .launch_values(kind)
                .into_iter()
                .map(|(_, label)| label)
                .collect(),
            DialogSelect::Repeat => SCHEDULE_PRESETS
                .iter()
                .map(|preset| preset.label().to_string())
                .collect(),
            DialogSelect::Weekday => WEEKDAYS.iter().map(|day| day.to_string()).collect(),
            DialogSelect::TimerUnit => TIMER_UNITS
                .iter()
                .map(|unit| unit.label().to_string())
                .collect(),
            DialogSelect::Session => self
                .sessions
                .iter()
                .map(|session| session.label.clone())
                .collect(),
        }
    }

    pub(crate) fn selected_index(&self, kind: DialogSelect) -> Option<usize> {
        let draft = &self.draft;
        match kind {
            DialogSelect::Project => self
                .state
                .projects
                .iter()
                .position(|project| project.project_id == draft.project_id),
            DialogSelect::Agent => self
                .state
                .agents
                .iter()
                .position(|agent| agent.agent_id == draft.agent_id),
            DialogSelect::Model => {
                let target = if draft.model.trim().is_empty() {
                    self.resolved_default_model()
                        .map(|(value, _)| value)
                        .unwrap_or_default()
                } else {
                    draft.model.trim().to_string()
                };
                self.launch_values(kind)
                    .iter()
                    .position(|(value, _)| *value == target)
            }
            DialogSelect::Effort => {
                let values = self.launch_values(kind);
                let target = draft.effort.trim();
                if target.is_empty() {
                    let default_label = self.default_effort_label();
                    values.iter().position(|(_, label)| *label == default_label)
                } else {
                    values.iter().position(|(value, _)| value == target)
                }
            }
            DialogSelect::Repeat => SCHEDULE_PRESETS
                .iter()
                .position(|preset| *preset == draft.schedule_preset),
            DialogSelect::Weekday => Some(draft.weekly_day.min(6)),
            DialogSelect::TimerUnit => TIMER_UNITS
                .iter()
                .position(|unit| *unit == draft.timer_unit),
            DialogSelect::Session => self
                .sessions
                .iter()
                .position(|session| session.session_id == draft.thread_session_id),
        }
    }

    pub(crate) fn selected_label(&self, kind: DialogSelect) -> Option<String> {
        self.selected_index(kind)
            .and_then(|index| self.items(kind).get(index).cloned())
            .or_else(|| match kind {
                DialogSelect::Model => Some(self.default_model_label()),
                DialogSelect::Effort => Some(self.default_effort_label()),
                _ => None,
            })
    }

    pub(crate) fn toggle_select(&mut self, kind: DialogSelect, cx: &mut Context<Self>) {
        let selected = self.selected_index(kind);
        for (index, select) in self.selects.iter_mut().enumerate() {
            if index == kind.index() {
                select.toggle(selected);
            } else {
                select.close();
            }
        }
        cx.notify();
    }

    pub(crate) fn close_select(&mut self, kind: DialogSelect, cx: &mut Context<Self>) {
        self.selects[kind.index()].close();
        cx.notify();
    }

    pub(crate) fn choose(&mut self, kind: DialogSelect, index: usize, cx: &mut Context<Self>) {
        self.selects[kind.index()].close();
        match kind {
            DialogSelect::Project => {
                if let Some(project_id) = self
                    .state
                    .projects
                    .get(index)
                    .map(|project| project.project_id.clone())
                    && project_id != self.draft.project_id
                {
                    self.draft.project_id = project_id.clone();
                    self.draft.thread_session_id.clear();
                    self.draft.thread_agent_session_id.clear();
                    self.sessions.clear();
                    let _ = self.view.update(cx, |view, cx| {
                        view.request_sessions(&project_id, cx);
                    });
                }
            }
            DialogSelect::Agent => {
                if let Some(agent) = self.state.agents.get(index)
                    && agent.agent_id != self.draft.agent_id
                {
                    self.draft.agent_id = agent.agent_id.clone();
                    // A model and effort belong to the agent they were picked for.
                    self.draft.model.clear();
                    self.draft.effort.clear();
                }
            }
            DialogSelect::Model => {
                if let Some((value, _)) = self.launch_values(kind).into_iter().nth(index) {
                    self.draft.model = value;
                    // Keep the effort only when the new model offers it.
                    let effort = self.draft.effort.trim().to_string();
                    if !effort.is_empty()
                        && !self
                            .effort_values()
                            .iter()
                            .any(|(value, _)| *value == effort)
                    {
                        self.draft.effort.clear();
                    }
                }
            }
            DialogSelect::Effort => {
                if let Some((value, _)) = self.launch_values(kind).into_iter().nth(index) {
                    self.draft.effort = value;
                }
            }
            DialogSelect::Repeat => {
                if let Some(preset) = SCHEDULE_PRESETS.get(index) {
                    self.draft.schedule_preset = *preset;
                }
            }
            DialogSelect::Weekday => self.draft.weekly_day = index.min(6),
            DialogSelect::TimerUnit => {
                if let Some(unit) = TIMER_UNITS.get(index) {
                    self.draft.timer_unit = *unit;
                }
            }
            DialogSelect::Session => {
                if let Some(session) = self.sessions.get(index) {
                    self.draft.thread_session_id = session.session_id.clone();
                    self.draft.thread_agent_session_id =
                        session.agent_session_id.clone().unwrap_or_default();
                    if let Some(agent_id) = session.agent_id.clone() {
                        self.draft.agent_id = agent_id;
                    }
                }
            }
        }
        self.error = None;
        cx.notify();
    }

    pub(crate) fn set_schedule_mode(&mut self, mode: ScheduleMode, cx: &mut Context<Self>) {
        self.draft.schedule_mode = mode;
        cx.notify();
    }

    /// The Worktree segment is disabled while the project cannot host worktrees.
    pub(crate) fn set_execution_kind(&mut self, kind: ExecutionKind, cx: &mut Context<Self>) {
        if kind == ExecutionKind::Worktree && !self.can_use_worktrees() {
            return;
        }
        self.draft.execution_kind = kind;
        cx.notify();
    }

    pub(crate) fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        self.draft.enabled = !self.draft.enabled;
        cx.notify();
    }

    /// The store answered the session list request.
    pub(crate) fn set_sessions(
        &mut self,
        sessions: Vec<AutomationSessionOption>,
        cx: &mut Context<Self>,
    ) {
        self.sessions = sessions;
        cx.notify();
    }

    pub(crate) fn save_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.saving = false;
        self.error = Some(error);
        cx.notify();
    }

    pub(crate) fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let draft = self.draft.clone();
        let result = self
            .view
            .update(cx, |view, cx| view.save_draft(&draft, cx))
            .unwrap_or_else(|_| Err("The Automate view is no longer open.".to_string()));
        match result {
            Ok(()) => self.saving = true,
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        let _ = self.view.update(cx, |view, _| view.dialog = None);
    }

    pub(crate) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        for kind in DialogSelect::ALL {
            let count = self.items(kind).len();
            match self.selects[kind.index()].handle_key(key, count) {
                ModalSelectKey::Consumed => {
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                ModalSelectKey::Choose(index) => {
                    self.choose(kind, index, cx);
                    cx.stop_propagation();
                    return;
                }
                ModalSelectKey::Ignored => {}
            }
        }
        if key == "escape" {
            cx.stop_propagation();
            self.cancel(window, cx);
        }
    }
}

impl crate::app::window::ModalCornerClose for AutomationDialog {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
