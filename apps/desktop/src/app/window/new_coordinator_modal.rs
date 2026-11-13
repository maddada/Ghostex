//! Native GPUI New Coordinator dialog: name the coordinator, pick where it runs, see its playbook
//! and the locked line its first message starts with; Next opens the agent picker.
//!
//! CDXC:Coordinators 2026-10-17 DECISION:
//! User: "we should just include a line that can't be removed saying: "Read and adhere to this skill: /ghostex-orchestrator" ... also remove this [First request] and this [Goal]". The dialog shows the skill line as a locked first message and has no Goal or First request field; the goal stays settable with `ghostex orchestrator set-goal` and Goal & Instructions…. Supersedes the 2026-09-30 WHY that mirrored Claude's New project dialog with an optional goal and first request.
//!
//! CDXC:Coordinators 2026-11-13 DECISION:
//! User (Q2980 "Build it: auto-send + remind on drift"): the form drops its Agent, Model and Effort fields (the 2026-10-17 Agent dropdown); Next opens the normal New Thread picker titled "Pick the coordinator's agent" (any agent, any account on Tab), and the pick creates the coordinator and sends its first message. Its model is the agent's own start model (Opus 5.5 at High on Claude, High effort where the launch takes one), changed afterwards in its chat like any session's. The first message line is shown without an agent's spelling, since the agent is picked next.
//! SEE-ALSO: apps/desktop/src/app/new_coordinator_modal_lifecycle.rs (open, create), apps/desktop/src/app/gx_store/create/coordinator.rs (the gxserver calls), server/src/coordinators/role.rs (the skill and the first message), apps/mobile/app/src/screens/sessions-screen/NewCoordinatorSheet.tsx (the phone's same form).
//!
//! The Playbook overrides row shows the user's overrides (what every coordinator reads after
//! Ghostex's playbook) and its Edit button turns the dialog into their editor
//! (new_coordinator_playbook.rs). For a project on more than one computer (or cloned twice
//! here) the Computer field picks where it runs (new_coordinator_computer.rs).
use super::native_modal_kit::*;
#[path = "new_coordinator_computer.rs"]
mod computer;
#[path = "new_coordinator_playbook.rs"]
mod playbook;
pub(crate) use computer::*;
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
pub(crate) const NEW_COORDINATOR_MODAL_INITIAL_HEIGHT: f32 = 540.0;

const TITLE: &str = "New Coordinator";
const FIELD_NAME: &str = "Name";
const NAME_PLACEHOLDER: &str = "e.g. Checkout redesign";
const NAME_HINT: &str =
    "Shown in the sidebar so you can find this coordinator later. It keeps this name.";
const FIELD_FIRST_MESSAGE: &str = "First message";
const FIRST_MESSAGE_HINT: &str = "Every coordinator starts with this line, which loads its playbook, written the way its agent runs a skill. Write your first request in its chat.";
const LOCK_ICON: &str = "titlebar/lock.svg";
/// CDXC:Coordinators 2026-10-01 DECISION:
/// User: a coordinator on a Claude launcher starts on Opus 5.5 ("my preferences should be the preferences for this feature for customers using the same setup, since that's what I tested"). `opus[1m]` is Opus 5.5's row in the Claude lineup; a Codex lineup has no such row and keeps its own default model. Its effort is High since 2026-10-29 (Low is offered too since 2026-10-31) (`orchestrator_efforts` in gx-chat-core; XHigh from 2026-10-17 until then).
/// SEE-ALSO: DEFAULT_CLAUDE_COORDINATOR_MODEL in server/src/ghostex_cli/coordinator/command.rs (`ghostex coordinator create` keeps the same default).
pub(crate) const DEFAULT_CLAUDE_COORDINATOR_MODEL: &str = "opus[1m]";
const FIELD_GOAL: &str = "Goal (optional)";
const GOAL_PLACEHOLDER: &str = "One line it works toward, e.g. Ship the new checkout by Friday";
const CANCEL: &str = "Cancel";
const NEXT: &str = "Next";

/// The locked first line the form shows before the agent is picked: the skill by its name, which
/// each agent's first message writes in its own spelling (gx-chat-core `coordinator_skill_line`).
fn coordinator_first_line() -> String {
    use ghostex_gx_chat_core::composer::skill_invocation::{
        COORDINATOR_SKILL_NAME, SKILL_LINE_PREFIX,
    };
    format!("{SKILL_LINE_PREFIX}{COORDINATOR_SKILL_NAME}")
}

/// An agent set up on the copy's computer, with the warning the agent picker shows for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewCoordinatorAgent {
    pub(crate) agent_id: String,
    /// "Coordinator quality may be degraded on <model>" when the model and effort it starts a
    /// coordinator on give a concern (gx-protocol `coordinator_quality_concern`).
    pub(crate) warning: Option<String>,
}

pub(crate) enum NewCoordinatorModalCommand {
    /// Next: the dialog closes and the agent picker opens for the copy `group_id` names; the
    /// pick creates the coordinator named `name`.
    PickAgent {
        group_id: String,
        name: String,
    },
    /// Saves the playbook overrides of the copy `group_id` names, or moves off the old full
    /// copies. The dialog stays open and waits on `reply`.
    SavePlaybook {
        group_id: String,
        change: PlaybookChange,
        reply: NewCoordinatorPlaybookReply,
    },
    /// The Computer field picked another copy: read its playbook (for the pick `generation`), and
    /// its computer's models when they are not known yet.
    ChooseCopy {
        group_id: String,
        generation: u64,
    },
    /// The Computer menu opened: send the copies as they are now (`set_copies`).
    RefreshCopies,
    Cancel,
}

pub(crate) type NewCoordinatorModalHost = Rc<dyn Fn(NewCoordinatorModalCommand, &mut App)>;

pub(crate) struct NewCoordinatorModalConfig {
    pub(crate) project_name: String,
    /// Where it can run, with each computer's agents: one copy for a project on one computer.
    pub(crate) copies: Vec<NewCoordinatorCopy>,
    /// The copy the dialog opens on.
    pub(crate) selected_copy: usize,
    /// `None` until the host has read it; the row is left out while it is unknown.
    pub(crate) playbook: Option<OrchestratorPlaybookInfo>,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiNewCoordinatorModalWindow {
    host: NewCoordinatorModalHost,
    palette: ModalPalette,
    project_name: String,
    copies: Vec<NewCoordinatorCopy>,
    copy_index: usize,
    copy_select: ModalSelect,
    /// The Computer field shows: more than one copy, now or since the dialog opened.
    copies_shown: bool,
    /// Bumped on every pick of a copy, so a playbook read or save for an earlier one is dropped.
    playbook_generation: u64,
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
        let subscriptions = vec![
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
        let copy_index = config
            .selected_copy
            .min(config.copies.len().saturating_sub(1));
        Self {
            host,
            palette: config.palette,
            project_name: config.project_name,
            copies_shown: config.copies.len() > 1,
            copies: config.copies,
            copy_index,
            copy_select: ModalSelect::new(),
            playbook_generation: 0,
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
        }
    }

    fn close_menus(&mut self) {
        self.copy_select.close();
    }

    fn any_menu_open(&self) -> bool {
        self.copy_select.open
    }

    fn can_create(&self) -> bool {
        self.copy_ready()
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
        if !self.can_create() {
            return;
        }
        let Some(group_id) = self.chosen_group_id().map(str::to_string) else {
            return;
        };
        let name = self.name.split_whitespace().collect::<Vec<_>>().join(" ");
        self.close_window_and_send(
            NewCoordinatorModalCommand::PickAgent { group_id, name },
            window,
            cx,
        );
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
        if self.handle_copy_key(key, cx) {
            cx.stop_propagation();
            return;
        }
        match key {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held && self.can_create() => self.create(window, cx),
            _ => return,
        }
        cx.stop_propagation();
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
        if let Some(field) = self.render_computer_field(cx) {
            body = body.child(field);
        }
        if let Some(message) = self.copy_agents_problem() {
            body = body.child(modal_error(&p, &message));
        }
        if let Some(row) = self.render_playbook_row(cx) {
            body = body.child(row);
        }
        body = body.child(self.render_first_message(&coordinator_first_line()));
        body.into_any_element()
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

    /// Why the picked copy has no agent to pick from yet, or none at all.
    fn copy_agents_problem(&self) -> Option<String> {
        let copy = self.copy()?;
        if !copy.usable() {
            return None;
        }
        let place = if self.copies_shown {
            format!(" on {}", copy.computer)
        } else {
            String::new()
        };
        match &copy.agents {
            None if self.copies_shown => Some(format!(
                "Couldn't read {}'s agents yet. Try again in a moment.",
                copy.computer
            )),
            Some(agents) if agents.is_empty() => Some(format!(
                "No agent is set up{place}. Add one in Settings > Agents first."
            )),
            _ => None,
        }
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

    /// The one open select's list, floated above the dialog. Every select goes through
    /// `modal_select_menu` on every render, open or not, because under window glass the list
    /// draws in a frosted window of its own that only a closed select's render hides.
    ///
    /// CDXC:AppModal 2026-11-02 WHY:
    /// Issue #395: this drew only the open select's list and asked nothing of the closed ones, so a picked agent closed the Agent select but its frosted list stayed on screen (only the Effort list was ever told to hide). Never skip a closed select here.
    fn render_open_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.render_copy_menu(window, cx)
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
            NEXT,
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
            "\"{}\" keeps its conversation and keeps running: nothing restarts or interrupts it. It gets the coordinator's crown now, and its coordinator playbook arrives once its current turn is over.",
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
