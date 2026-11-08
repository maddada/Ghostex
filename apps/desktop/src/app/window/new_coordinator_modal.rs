//! Native GPUI New Coordinator dialog: name the orchestrator, pick its agent, model and effort, and
//! see the locked line its first message starts with.
//!
//! CDXC:Coordinators 2026-10-17 DECISION:
//! User: "let's please allow picking more cli agents here (should be a dropdown). we should just include a line that can't be removed saying: "Read and adhere to this skill: /ghostex-orchestrator" ... also remove this [First request] and this [Goal]". The Agent field is a dropdown of every launcher an orchestrator can run on (gxserver `COORDINATOR_AGENT_FAMILIES`), the dialog shows the skill line (in the chosen agent's own skill spelling) as a locked first message, and it has no Goal or First request field; the goal stays settable with `ghostex orchestrator set-goal` and Goal & Instructions…. Supersedes the 2026-09-30 WHY that mirrored Claude's New project dialog with an optional goal and first request.
//! SEE-ALSO: apps/desktop/src/app/new_coordinator_modal_lifecycle.rs (open, create), apps/desktop/src/app/gx_store/create/coordinator.rs (the gxserver calls), server/src/coordinators/role.rs (the skill and the first message), apps/mobile/app/src/screens/sessions-screen/NewCoordinatorSheet.tsx (the phone's same form).
//!
//! The Playbook overrides row shows the user's overrides (what every coordinator reads after
//! Ghostex's playbook) and its Edit button turns the dialog into their editor
//! (new_coordinator_playbook.rs).
use super::native_modal_kit::*;
#[path = "new_coordinator_playbook.rs"]
mod playbook;
use ghostex_gx_chat_core::menus::orchestrator_effort::{
    default_orchestrator_effort, orchestrator_efforts,
};
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    Focusable as _, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    Styled as _, Subscription, WeakEntity, Window, div, px, svg,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
pub(crate) use playbook::*;
use serde_json::Value;
use std::rc::Rc;

pub(crate) const NEW_COORDINATOR_MODAL_WIDTH: f32 = 540.0;
pub(crate) const NEW_COORDINATOR_MODAL_INITIAL_HEIGHT: f32 = 700.0;

const TITLE: &str = "New Coordinator";
const FIELD_NAME: &str = "Name";
const NAME_PLACEHOLDER: &str = "e.g. Checkout redesign";
const NAME_HINT: &str =
    "Shown in the sidebar so you can find this coordinator later. It keeps this name.";
const FIELD_AGENT: &str = "Agent";
const FIELD_MODEL: &str = "Model";
const FIELD_EFFORT: &str = "Effort";
const MODEL_FILTER_PLACEHOLDER: &str = "Search models";
const MODEL_EMPTY_MESSAGE: &str = "No models match.";
/// A lineup longer than this gets a search field in its Model list (OpenCode lists hundreds).
const MODEL_SEARCH_THRESHOLD: usize = 12;
const FIELD_FIRST_MESSAGE: &str = "First message";
const FIRST_MESSAGE_HINT: &str = "Every coordinator starts with this line, which loads its playbook. Write your first request in its chat.";
const LOCK_ICON: &str = "titlebar/lock.svg";
/// CDXC:Coordinators 2026-10-01 DECISION:
/// User: a coordinator on a Claude launcher starts on Opus 5.5 ("my preferences should be the preferences for this feature for customers using the same setup, since that's what I tested"). `opus[1m]` is Opus 5.5's row in the Claude lineup; a Codex lineup has no such row and keeps its own default model. Its effort is High since 2026-10-29 (Low is offered too since 2026-10-31) (`orchestrator_efforts` in gx-chat-core; XHigh from 2026-10-17 until then).
/// SEE-ALSO: DEFAULT_CLAUDE_COORDINATOR_MODEL in server/src/ghostex_cli/coordinator/command.rs (`ghostex coordinator create` keeps the same default).
pub(crate) const DEFAULT_CLAUDE_COORDINATOR_MODEL: &str = "opus[1m]";
const FIELD_GOAL: &str = "Goal (optional)";
const GOAL_PLACEHOLDER: &str = "One line it works toward, e.g. Ship the new checkout by Friday";
const CANCEL: &str = "Cancel";
const CREATE: &str = "Create";

/// An agent an orchestrator can run on, with its model lineup and the line its first message
/// starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewCoordinatorAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    /// Empty for an agent whose launch takes no model: it starts on its configured one.
    pub(crate) models: Vec<NewCoordinatorModel>,
    pub(crate) skill_line: String,
}

/// One model of the lineup, with the efforts it accepts as `(value, label)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewCoordinatorModel {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) efforts: Vec<(String, String)>,
    pub(crate) default: bool,
}

pub(crate) enum NewCoordinatorModalCommand {
    Create {
        agent_id: String,
        name: String,
        model: Option<String>,
        effort: Option<String>,
    },
    /// Saves the playbook overrides, or moves off the old full copies. The dialog stays open and
    /// waits on `reply`.
    SavePlaybook {
        change: PlaybookChange,
        reply: NewCoordinatorPlaybookReply,
    },
    Cancel,
}

pub(crate) type NewCoordinatorModalHost = Rc<dyn Fn(NewCoordinatorModalCommand, &mut App)>;

pub(crate) struct NewCoordinatorModalConfig {
    pub(crate) project_name: String,
    pub(crate) agents: Vec<NewCoordinatorAgent>,
    pub(crate) selected_agent: usize,
    /// `None` until the host has read it; the row is left out while it is unknown.
    pub(crate) playbook: Option<OrchestratorPlaybookInfo>,
    pub(crate) palette: ModalPalette,
    /// Show all effort levels (`orchestratorAllEfforts`): the Effort menu lists every level the
    /// model has instead of the orchestrator levels.
    pub(crate) all_efforts: bool,
}

pub(crate) struct GpuiNewCoordinatorModalWindow {
    host: NewCoordinatorModalHost,
    palette: ModalPalette,
    project_name: String,
    agents: Vec<NewCoordinatorAgent>,
    selected_agent: usize,
    model_index: usize,
    effort: Option<String>,
    all_efforts: bool,
    agent_select: ModalSelect,
    model_select: ModalSelect,
    model_filter: Entity<InputState>,
    effort_select: ModalSelect,
    name_input: Entity<InputState>,
    name: String,
    playbook: Option<OrchestratorPlaybookInfo>,
    playbook_input: Entity<TextareaState>,
    editor: Option<PlaybookEditor>,
    /// Ghostex's part of the playbook in effect (`/api/readCoordinatorGuide`), once read.
    playbook_guide: Option<Result<String, String>>,
    /// The read-only playbook page is open.
    viewing: bool,
    /// The read-only playbook page's scroll position.
    playbook_view_scroll: gpui::ScrollHandle,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiNewCoordinatorModalWindow {
    pub(crate) fn new(
        config: NewCoordinatorModalConfig,
        host: NewCoordinatorModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        let playbook_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(8, 14)
                .placeholder(OVERRIDES_PLACEHOLDER)
        });
        let model_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(MODEL_FILTER_PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe_in(
                &model_filter,
                window,
                |this: &mut Self, _input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.model_select.highlight = Some(0);
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &name_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.name = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &playbook_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let value = input.read(cx).value().to_string();
                    if let Some(editor) = this.editor.as_mut() {
                        editor.draft = value;
                        cx.notify();
                    }
                },
            ),
        ];
        name_input.update(cx, |input, cx| input.focus(window, cx));
        let selected_agent = config
            .selected_agent
            .min(config.agents.len().saturating_sub(1));
        let mut this = Self {
            host,
            palette: config.palette,
            project_name: config.project_name,
            agents: config.agents,
            selected_agent,
            model_index: 0,
            effort: None,
            all_efforts: config.all_efforts,
            agent_select: ModalSelect::new(),
            model_select: ModalSelect::new(),
            model_filter,
            effort_select: ModalSelect::new(),
            name_input,
            name: String::new(),
            playbook: config.playbook,
            playbook_input,
            editor: None,
            playbook_guide: None,
            viewing: false,
            playbook_view_scroll: gpui::ScrollHandle::new(),
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.reset_model();
        this
    }

    fn agent(&self) -> Option<&NewCoordinatorAgent> {
        self.agents.get(self.selected_agent)
    }

    fn model(&self) -> Option<&NewCoordinatorModel> {
        self.agent()?.models.get(self.model_index)
    }

    /// Whether the Model list has a search field: only a long lineup.
    fn model_searchable(&self) -> bool {
        self.agent()
            .is_some_and(|agent| agent.models.len() > MODEL_SEARCH_THRESHOLD)
    }

    /// The lineup indices the Model list shows: every model, or those the search matches by
    /// label or id.
    fn visible_models(&self, cx: &App) -> Vec<usize> {
        let Some(agent) = self.agent() else {
            return Vec::new();
        };
        let query = if self.model_searchable() {
            self.model_filter.read(cx).value().to_string()
        } else {
            String::new()
        };
        agent
            .models
            .iter()
            .enumerate()
            .filter(|(_, model)| {
                modal_select_filter_matches(&query, &format!("{} {}", model.label, model.value))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The model's efforts an orchestrator is offered: Low, Medium, High and XHigh, or Low up to the
    /// highest below Max, or all of them while Show all effort levels is on.
    fn efforts(&self) -> Vec<(String, String)> {
        let Some(model) = self.model() else {
            return Vec::new();
        };
        let values: Vec<&str> = model
            .efforts
            .iter()
            .map(|(value, _)| value.as_str())
            .collect();
        let offered = orchestrator_efforts(&values, self.all_efforts, None);
        model
            .efforts
            .iter()
            .filter(|(value, _)| offered.contains(value))
            .cloned()
            .collect()
    }

    /// Opus 5.5 on Claude, otherwise the agent's default model, and its highest orchestrator
    /// effort (High where it has it).
    fn reset_model(&mut self) {
        self.model_index = self
            .agent()
            .and_then(|agent| {
                agent
                    .models
                    .iter()
                    .position(|model| model.value == DEFAULT_CLAUDE_COORDINATOR_MODEL)
                    .or_else(|| agent.models.iter().position(|model| model.default))
            })
            .unwrap_or(0);
        self.effort = None;
        self.reset_effort();
    }

    fn reset_effort(&mut self) {
        let efforts = self.efforts();
        let keep = self
            .effort
            .as_ref()
            .filter(|effort| efforts.iter().any(|(value, _)| value == *effort))
            .cloned();
        let values: Vec<&str> = efforts.iter().map(|(value, _)| value.as_str()).collect();
        // The recommended level (High) even while every level is listed.
        self.effort = keep.or_else(|| default_orchestrator_effort(&values));
    }

    fn close_menus(&mut self) {
        self.agent_select.close();
        self.model_select.close();
        self.effort_select.close();
    }

    fn toggle_agent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.len() < 2 {
            return;
        }
        let open = self.agent_select.open;
        self.close_menus();
        self.focus_handle.focus(window, cx);
        if !open {
            self.agent_select.toggle(Some(self.selected_agent));
        }
        cx.notify();
    }

    fn toggle_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agent().is_none_or(|agent| agent.models.is_empty()) {
            return;
        }
        let open = self.model_select.open;
        self.close_menus();
        self.focus_handle.focus(window, cx);
        if !open {
            if self.model_searchable() {
                self.model_filter.update(cx, |filter, cx| {
                    filter.set_value("", window, cx);
                    filter.focus(window, cx);
                });
            }
            self.model_select.toggle(Some(self.model_index));
        }
        cx.notify();
    }

    pub(crate) fn toggle_effort_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let efforts = self.efforts();
        if efforts.is_empty() {
            return;
        }
        let open = self.effort_select.open;
        self.close_menus();
        self.focus_handle.focus(window, cx);
        if !open {
            let selected = efforts
                .iter()
                .position(|(value, _)| Some(value) == self.effort.as_ref());
            self.effort_select.toggle(selected);
        }
        cx.notify();
    }

    fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_agent = index;
        self.agent_select.close();
        self.reset_model();
        cx.notify();
    }

    /// Picks the `visible_index`th row the Model list shows.
    fn choose_model(&mut self, visible_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.visible_models(cx).get(visible_index).copied() else {
            return;
        };
        self.model_index = index;
        self.model_select.close();
        self.focus_handle.focus(window, cx);
        self.reset_effort();
        cx.notify();
    }

    fn choose_effort(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some((value, _)) = self.efforts().get(index) {
            self.effort = Some(value.clone());
        }
        self.effort_select.close();
        cx.notify();
    }

    fn any_menu_open(&self) -> bool {
        self.agent_select.open || self.model_select.open || self.effort_select.open
    }

    fn can_create(&self) -> bool {
        !self.agents.is_empty()
    }

    fn close_window_and_send(
        &mut self,
        command: NewCoordinatorModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(NewCoordinatorModalCommand::Cancel, window, cx);
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.agents.get(self.selected_agent).cloned() else {
            return;
        };
        let name = self.name.split_whitespace().collect::<Vec<_>>().join(" ");
        let command = NewCoordinatorModalCommand::Create {
            agent_id: agent.agent_id,
            name,
            model: self.model().map(|model| model.value.clone()),
            effort: self.effort.clone().filter(|_| !self.efforts().is_empty()),
        };
        self.close_window_and_send(command, window, cx);
    }

    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewing {
            cx.stop_propagation();
            return;
        }
        if self.editor.is_some() {
            // Enter is a newline in the overrides; Cmd/Ctrl+Enter saves.
            let in_text = self
                .playbook_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
            if in_text && !action.secondary {
                cx.propagate();
                return;
            }
            cx.stop_propagation();
            self.save_playbook(window, cx);
            return;
        }
        cx.stop_propagation();
        // Enter in the Model search picks the highlighted model.
        if self.model_select.open {
            let highlight = self.model_select.highlight.unwrap_or(0);
            self.choose_model(highlight, window, cx);
            return;
        }
        if self.can_create() {
            self.create(window, cx);
        }
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.viewing {
            self.close_playbook_view(window, cx);
            return;
        }
        if self.editor.is_some() {
            self.close_playbook_editor(window, cx);
            return;
        }
        if self.any_menu_open() {
            self.close_menus();
            self.focus_handle.focus(window, cx);
            cx.notify();
            return;
        }
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self.viewing {
            if key == "escape" {
                self.close_playbook_view(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        if self.editor.is_some() {
            if key == "escape" {
                self.close_playbook_editor(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        match self.agent_select.handle_key(key, self.agents.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_agent(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        // While the Model list has a search field, typed keys belong to it.
        if self.model_select.open
            && self.model_searchable()
            && !matches!(key, "up" | "down" | "enter" | "escape")
        {
            return;
        }
        let models = self.visible_models(cx).len();
        match self.model_select.handle_key(key, models) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_model(index, window, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match self.effort_select.handle_key(key, self.efforts().len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_effort(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match key {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held && self.can_create() => self.create(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// A select under its label, with its trigger bounds captured for the floating list.
    fn select_field(
        &self,
        label: &'static str,
        select: &ModalSelect,
        trigger: AnyElement,
    ) -> gpui::Div {
        v_flex()
            .gap(px(8.0))
            .child(modal_section_title(&self.palette, label))
            .child(
                h_flex()
                    .w_full()
                    .on_children_prepainted(capture_child_bounds(select.trigger_bounds.clone(), 0))
                    .child(trigger),
            )
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut body = v_flex().w_full().flex_1().min_h_0().gap(px(16.0));
        body = body.child(
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(modal_section_title(&p, FIELD_NAME))
                .child(modal_text_input(&p, &self.name_input, false, window, cx))
                .child(modal_hint(&p, NAME_HINT)),
        );
        let Some(agent) = self.agent() else {
            return body
                .child(modal_error(
                    &p,
                    "A coordinator runs on Claude, Codex, ZCode, Cursor, Grok, Copilot, Antigravity, Pi, OMP, OpenCode or DeepSeek Harness. Add one of them in Settings > Agents first.",
                ))
                .into_any_element();
        };
        let agent_trigger = modal_select_trigger(
            &p,
            &self.agent_select,
            "new-coordinator-agent-select",
            Some(agent.name.clone()),
            "Choose an agent",
            self.agents.len() < 2,
            |this, window, cx| this.toggle_agent_menu(window, cx),
            cx,
        );
        body = body.child(
            self.select_field(FIELD_AGENT, &self.agent_select, agent_trigger)
                .w_full(),
        );
        if let Some(row) = self.render_model_row(cx) {
            body = body.child(row);
        }
        if let Some(row) = self.render_playbook_row(cx) {
            body = body.child(row);
        }
        body.child(self.render_first_message(&agent.skill_line))
            .into_any_element()
    }

    /// The skill line, drawn as a read-only box with a lock: it cannot be edited or removed.
    fn render_first_message(&self, line: &str) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(8.0))
            .child(modal_section_title(&p, FIELD_FIRST_MESSAGE))
            .child(
                h_flex()
                    .id("new-coordinator-first-message")
                    .w_full()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.hairline)
                    .bg(p.raised)
                    .child(
                        svg()
                            .path(LOCK_ICON)
                            .size(px(14.0))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(p.muted)
                            .child(line.to_string()),
                    ),
            )
            .child(modal_hint(&p, FIRST_MESSAGE_HINT))
            .into_any_element()
    }

    /// Model and effort side by side; the effort half is left out for a model without efforts,
    /// and the row for an agent whose launch takes no model.
    fn render_model_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let agent = self.agent()?;
        if agent.models.is_empty() {
            return None;
        }
        let p = self.palette;
        let model = modal_select_trigger(
            &p,
            &self.model_select,
            "new-coordinator-model-select",
            self.model().map(|model| model.label.clone()),
            "Choose a model",
            false,
            |this, window, cx| this.toggle_model_menu(window, cx),
            cx,
        );
        let efforts = self.efforts();
        let mut row = h_flex().w_full().gap(px(12.0)).child(
            self.select_field(FIELD_MODEL, &self.model_select, model)
                .flex_1()
                .min_w_0(),
        );
        if !efforts.is_empty() {
            let label = efforts
                .iter()
                .find(|(value, _)| Some(value) == self.effort.as_ref())
                .map(|(_, label)| label.clone());
            let effort = modal_select_trigger(
                &p,
                &self.effort_select,
                "new-coordinator-effort-select",
                label,
                "Effort",
                false,
                |this, window, cx| this.toggle_effort_menu(window, cx),
                cx,
            );
            row = row.child(
                self.select_field(FIELD_EFFORT, &self.effort_select, effort)
                    .w(px(170.0))
                    .flex_shrink_0(),
            );
        }
        Some(row.into_any_element())
    }

    /// The one open select's list, floated above the dialog. Every select goes through
    /// `modal_select_menu` on every render, open or not, because under window glass the list
    /// draws in a frosted window of its own that only a closed select's render hides.
    ///
    /// CDXC:AppModal 2026-11-02 WHY:
    /// Issue #395: this drew only the open select's list and asked nothing of the closed ones, so a picked agent closed the Agent select but its frosted list stayed on screen (only the Effort list was ever told to hide). Never skip a closed select here.
    fn render_open_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let agent_labels = self
            .agents
            .iter()
            .map(|agent| agent.name.clone())
            .collect::<Vec<_>>();
        let agent_menu = modal_select_menu(
            &p,
            &self.agent_select,
            "new-coordinator-agent-menu",
            &agent_labels,
            Some(self.selected_agent),
            |this: &mut Self, index, _window, cx| this.choose_agent(index, cx),
            |this: &mut Self, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let visible = self.visible_models(cx);
        let model_menu = if self.model_searchable() {
            // The searchable list draws inside the dialog (its field needs the keyboard), so a
            // frosted list of the same select is hidden here.
            hide_hosted_modal_popover("new-coordinator-model-menu", window, cx);
            let rows = self
                .agent()
                .map(|agent| {
                    visible
                        .iter()
                        .map(|index| ModalSearchableSelectRow {
                            label: agent.models[*index].label.clone(),
                            disabled: false,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            modal_searchable_select_menu(
                &p,
                &self.model_select,
                "new-coordinator-model-search-menu",
                &self.model_filter,
                &rows,
                MODEL_EMPTY_MESSAGE,
                |this: &mut Self, index, window, cx| this.choose_model(index, window, cx),
                |this: &mut Self, window, cx| {
                    this.model_filter.update(cx, |filter, cx| {
                        filter.set_value("", window, cx);
                        filter.focus(window, cx);
                    });
                    this.model_select.highlight = Some(0);
                    cx.notify();
                },
                |this: &mut Self, window, cx| {
                    this.model_select.close();
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                },
                window,
                cx,
            )
        } else {
            let model_labels = self
                .agent()
                .map(|agent| {
                    agent
                        .models
                        .iter()
                        .map(|model| model.label.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            modal_select_menu(
                &p,
                &self.model_select,
                "new-coordinator-model-menu",
                &model_labels,
                Some(self.model_index),
                |this: &mut Self, index, window, cx| this.choose_model(index, window, cx),
                |this: &mut Self, _window, cx| {
                    this.model_select.close();
                    cx.notify();
                },
                window,
                cx,
            )
        };
        let efforts = self.efforts();
        let effort_labels = efforts
            .iter()
            .map(|(_, label)| label.clone())
            .collect::<Vec<_>>();
        let effort_menu = modal_select_menu(
            &p,
            &self.effort_select,
            "new-coordinator-effort-menu",
            &effort_labels,
            efforts
                .iter()
                .position(|(value, _)| Some(value) == self.effort.as_ref()),
            |this: &mut Self, index, _window, cx| this.choose_effort(index, cx),
            |this: &mut Self, _window, cx| {
                this.effort_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        agent_menu.or(model_menu).or(effort_menu)
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        if self.viewing {
            return self.render_playbook_view_footer(cx);
        }
        if self.editor.is_some() {
            return self.render_playbook_footer(cx);
        }
        let cancel = modal_action_button(
            &p,
            "new-coordinator-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let create = modal_action_button(
            &p,
            "new-coordinator-create",
            CREATE,
            None,
            ModalButtonTone::Primary,
            !self.can_create(),
            |this, window, cx| {
                if this.can_create() {
                    this.create(window, cx)
                }
            },
            cx,
        );
        modal_footer(vec![cancel, create])
    }
}

impl Render for GpuiNewCoordinatorModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = format!(
            "One agent you talk to about {}. It plans the work, hands each task to a thread (its own agent session, optionally in its own worktree), and reports back when threads finish or need you.",
            self.project_name
        );
        let content = if self.viewing {
            self.render_playbook_view()
        } else if self.editor.is_some() {
            self.render_playbook_page(window, cx)
        } else {
            vec![
                modal_header(&p, TITLE, Some(description)),
                self.render_body(window, cx),
            ]
        };
        let footer = self.render_footer(cx);
        // Rendered on the playbook page too, so a list left open when Edit was pressed hides.
        let menu = self
            .render_open_menu(window, cx)
            .filter(|_| self.editor.is_none() && !self.viewing);
        modal_shell(
            &p,
            "ghostex-gpui-new-coordinator-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            menu,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiNewCoordinatorModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}

pub(crate) const MAKE_COORDINATOR_MODAL_INITIAL_HEIGHT: f32 = 330.0;

pub(crate) enum MakeCoordinatorModalCommand {
    Make { goal: String },
    Cancel,
}

pub(crate) type MakeCoordinatorModalHost = Rc<dyn Fn(MakeCoordinatorModalCommand, &mut App)>;

/// The Make Coordinator dialog: the session's Advanced > Make Coordinator. One optional field
/// (the goal) and a confirm, in the New Coordinator dialog's look.
pub(crate) struct GpuiMakeCoordinatorModalWindow {
    host: MakeCoordinatorModalHost,
    palette: ModalPalette,
    session_title: String,
    goal_input: Entity<InputState>,
    goal: String,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiMakeCoordinatorModalWindow {
    pub(crate) fn new(
        session_title: String,
        palette: ModalPalette,
        host: MakeCoordinatorModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let goal_input = cx.new(|cx| InputState::new(window, cx).placeholder(GOAL_PLACEHOLDER));
        let subscriptions = vec![cx.subscribe_in(
            &goal_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.goal = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        )];
        goal_input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            host,
            palette,
            session_title,
            goal_input,
            goal: String::new(),
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: MakeCoordinatorModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(MakeCoordinatorModalCommand::Cancel, window, cx);
    }

    fn make(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let goal = self.goal.trim().to_string();
        self.close_window_and_send(MakeCoordinatorModalCommand::Make { goal }, window, cx);
    }

    fn on_enter_action(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.make(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.make(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl Render for GpuiMakeCoordinatorModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = format!(
            "\"{}\" keeps its conversation and keeps running: nothing restarts or interrupts it. It gets the crown now, and its coordinator playbook arrives once its current turn is over.",
            self.session_title
        );
        let body = v_flex()
            .w_full()
            .gap(px(8.0))
            .child(modal_section_title(&p, FIELD_GOAL))
            .child(modal_text_input(&p, &self.goal_input, false, window, cx))
            .child(modal_hint(
                &p,
                "Sessions it started before are not its threads yet; ask it to adopt them (ghostex coordinator link).",
            ))
            .into_any_element();
        let cancel = modal_action_button(
            &p,
            "make-coordinator-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let make = modal_action_button(
            &p,
            "make-coordinator-confirm",
            "Make Coordinator",
            None,
            ModalButtonTone::Primary,
            false,
            |this, window, cx| this.make(window, cx),
            cx,
        );
        modal_shell(
            &p,
            "ghostex-gpui-make-coordinator-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            vec![
                modal_header(&p, "Make Coordinator", Some(description)),
                body,
            ],
            modal_footer(vec![cancel, make]),
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiMakeCoordinatorModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
