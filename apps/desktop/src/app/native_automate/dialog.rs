//! The create/edit automation dialog: the native twin of
//! apps/desktop/views/project-board/automation-dialog.tsx, drawn with the app modals' kit
//! (window/native_modal_kit.rs) in its own child window. This file owns its state and what its
//! controls do; dialog_render.rs lays it out.

use super::drafts::{
    AutomationDraft, ExecutionKind, SCHEDULE_PRESETS, ScheduleMode, TIMER_UNITS, WEEKDAYS,
};
use super::model::{AutomationSessionOption, AutomationState};
use super::view::NativeAutomateView;
use crate::app::window::{ModalPalette, ModalScrollFit, ModalSelect, ModalSelectKey};
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
    Repeat,
    Weekday,
    TimerUnit,
    Session,
}

impl DialogSelect {
    pub(crate) const ALL: [Self; 6] = [
        Self::Project,
        Self::Agent,
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
                if let Some(agent) = self.state.agents.get(index) {
                    self.draft.agent_id = agent.agent_id.clone();
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
