//! Native GPUI editors for an orchestrator's Goal & Instructions and its Memory, opened from the
//! orchestrator row's context menu. Both edit a local copy and save it with one
//! `/api/updateCoordinator` call (Memory sends the whole list), so Cancel leaves everything as it
//! was. The same window edits the user's Playbook overrides (every coordinator's, on that
//! orchestrator's computer), saved with `/api/setCoordinatorPlaybook`, and its View playbook shows
//! the playbook that coordinator reads, read-only (CDXC:Coordinators 2026-11-08 in
//! window/new_coordinator_playbook.rs; the user asked for it here too, Q1998).
//!
//!
//! CDXC:Coordinators 2026-10-14 DECISION:
//! User (Q76): "the desktop Goal & Instructions and Memory editors must stay open until the save succeeds (like the phone sheet): show a saving state, keep the edits and show the error inline ... close only on success." Save sends the edit with a reply handle and the editor waits on it showing "Saving…"; a rejected save re-enables Save and shows gxserver's message above the buttons. Cancel still closes at once.
//! SEE-ALSO: the user's decision on `coordinator_rows` in packages/gx-core/src/sidebar_menu/session.rs, apps/desktop/src/app/coordinator_details_modal_lifecycle.rs (open, save), apps/desktop/src/app/gx_store/coordinator_actions.rs (the menu rows), apps/mobile/app/src/screens/sessions-screen/CoordinatorDetailsSheet.tsx (the phone's editors).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    Focusable as _, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window,
    div, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const COORDINATOR_DETAILS_MODAL_WIDTH: f32 = 560.0;
pub(crate) const COORDINATOR_DETAILS_MODAL_INITIAL_HEIGHT: f32 = 600.0;

const GOAL_TITLE: &str = "Goal & Instructions";
const MEMORY_TITLE: &str = "Memory";
const OVERRIDES_TITLE: &str = "Playbook overrides";
const FIELD_GOAL: &str = "Goal";
const GOAL_PLACEHOLDER: &str = "One line it works toward, e.g. Ship the new checkout by Friday";
const FIELD_INSTRUCTIONS: &str = "Standing instructions";
const INSTRUCTIONS_PLACEHOLDER: &str = "Rules every thread follows, e.g. Branch from main.";
const ADD_PLACEHOLDER: &str = "A preference, decision or pitfall, in one line";
const CANCEL: &str = "Cancel";
const SAVE: &str = "Save";
const SAVING: &str = "Saving…";
const ADD: &str = "Add";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoordinatorDetailsMode {
    Goal,
    Memory,
    /// The playbook overrides for every coordinator.
    Overrides,
}

/// Where a save's outcome goes: the editor that sent it, in its own window.
pub(crate) struct CoordinatorDetailsSaveReply {
    view: WeakEntity<GpuiCoordinatorDetailsModalWindow>,
    window: AnyWindowHandle,
}

impl CoordinatorDetailsSaveReply {
    /// Hands the outcome to the editor: success closes it, a failure shows the message and keeps
    /// the edits. False when the editor is gone (cancelled meanwhile).
    pub(crate) fn finish(self, result: Result<(), String>, cx: &mut App) -> bool {
        let view = self.view;
        self.window
            .update(cx, |_root, window, cx| {
                view.update(cx, |editor, cx| editor.finish_save(result, window, cx))
            })
            .is_ok_and(|delivered| delivered.is_ok())
    }
}

pub(crate) enum CoordinatorDetailsModalCommand {
    SaveGoal {
        goal: String,
        instructions: String,
        reply: CoordinatorDetailsSaveReply,
    },
    SaveMemory {
        notes: Vec<String>,
        reply: CoordinatorDetailsSaveReply,
    },
    SaveOverrides {
        overrides: String,
        reply: CoordinatorDetailsSaveReply,
    },
    Cancel,
}

pub(crate) type CoordinatorDetailsModalHost = Rc<dyn Fn(CoordinatorDetailsModalCommand, &mut App)>;

pub(crate) struct CoordinatorDetailsModalConfig {
    pub(crate) mode: CoordinatorDetailsMode,
    pub(crate) title: String,
    pub(crate) goal: String,
    pub(crate) instructions: String,
    pub(crate) notes: Vec<String>,
    /// The playbook overrides, and the note about old full copies still in use (Overrides mode).
    pub(crate) overrides: String,
    pub(crate) old_copies_note: Option<String>,
    /// Ghostex's part of the playbook the coordinator reads (`/api/readCoordinatorGuide`), or why
    /// it could not be read; `None` leaves View playbook out (Overrides mode only).
    pub(crate) guide: Option<Result<String, String>>,
    /// Opens on the read-only playbook page (the menus' View playbook rows), whose Close then
    /// closes the window rather than going back to the editor.
    pub(crate) on_playbook: bool,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiCoordinatorDetailsModalWindow {
    host: CoordinatorDetailsModalHost,
    palette: ModalPalette,
    mode: CoordinatorDetailsMode,
    title: String,
    goal_input: Entity<InputState>,
    instructions_input: Entity<TextareaState>,
    overrides_input: Entity<TextareaState>,
    note_input: Entity<InputState>,
    goal: String,
    instructions: String,
    overrides: String,
    old_copies_note: Option<String>,
    guide: Option<Result<String, String>>,
    /// The read-only playbook page is open (Overrides mode).
    viewing: bool,
    /// The window opened on that page, so leaving it closes the window.
    opened_on_playbook: bool,
    note: String,
    notes: Vec<String>,
    /// A save is on its way to gxserver; the editor waits for its answer.
    saving: bool,
    /// Why the last save was refused, shown above the buttons.
    error: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiCoordinatorDetailsModalWindow {
    pub(crate) fn new(
        config: CoordinatorDetailsModalConfig,
        host: CoordinatorDetailsModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let goal_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(GOAL_PLACEHOLDER)
                .default_value(config.goal.clone())
        });
        let instructions_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(INSTRUCTIONS_PLACEHOLDER)
                .default_value(config.instructions.clone())
        });
        let overrides_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(8, 14)
                .placeholder(super::new_coordinator_modal::OVERRIDES_PLACEHOLDER)
                .default_value(config.overrides.clone())
        });
        let note_input = cx.new(|cx| InputState::new(window, cx).placeholder(ADD_PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe_in(
                &goal_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.goal = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &instructions_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.instructions = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &overrides_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.overrides = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &note_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.note = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
        ];
        match config.mode {
            CoordinatorDetailsMode::Goal => {
                goal_input.update(cx, |input, cx| input.focus(window, cx))
            }
            CoordinatorDetailsMode::Memory => {
                note_input.update(cx, |input, cx| input.focus(window, cx))
            }
            CoordinatorDetailsMode::Overrides => {
                overrides_input.update(cx, |input, cx| input.focus(window, cx))
            }
        }
        let opened_on_playbook = config.on_playbook
            && config.mode == CoordinatorDetailsMode::Overrides
            && config.guide.is_some();
        let focus_handle = cx.focus_handle();
        if opened_on_playbook {
            focus_handle.focus(window, cx);
        }
        Self {
            host,
            palette: config.palette,
            mode: config.mode,
            title: config.title,
            goal_input,
            instructions_input,
            overrides_input,
            note_input,
            goal: config.goal,
            instructions: config.instructions,
            overrides: config.overrides,
            old_copies_note: config.old_copies_note,
            guide: config.guide.map(|guide| {
                guide.map(|text| super::new_coordinator_modal::guide_ghostex_part(&text))
            }),
            viewing: opened_on_playbook,
            opened_on_playbook,
            note: String::new(),
            notes: config.notes,
            saving: false,
            error: None,
            fit: ModalFit::fixed(),
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: CoordinatorDetailsModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    /// Opens or leaves the read-only playbook page; leaving it puts the cursor back in the
    /// overrides.
    pub(crate) fn set_viewing(
        &mut self,
        viewing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.viewing = viewing;
        self.opened_on_playbook &= viewing;
        if viewing {
            self.focus_handle.focus(window, cx);
        } else {
            self.overrides_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(CoordinatorDetailsModalCommand::Cancel, window, cx);
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let reply = CoordinatorDetailsSaveReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        };
        let command = match self.mode {
            CoordinatorDetailsMode::Goal => CoordinatorDetailsModalCommand::SaveGoal {
                goal: self.goal.trim().to_string(),
                instructions: self.instructions.trim().to_string(),
                reply,
            },
            CoordinatorDetailsMode::Memory => {
                // A note typed but not added yet is saved too, rather than dropped silently.
                self.add_note(window, cx);
                CoordinatorDetailsModalCommand::SaveMemory {
                    notes: self.notes.clone(),
                    reply,
                }
            }
            CoordinatorDetailsMode::Overrides => CoordinatorDetailsModalCommand::SaveOverrides {
                overrides: self.overrides.trim().to_string(),
                reply,
            },
        };
        self.saving = true;
        self.error = None;
        cx.notify();
        (self.host)(command, cx);
    }

    /// The answer to a save: success closes the editor, a refusal keeps it open with the message.
    fn finish_save(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => window.remove_window(),
            Err(message) => {
                self.saving = false;
                self.error = Some(message);
                cx.notify();
            }
        }
    }

    fn add_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.note.split_whitespace().collect::<Vec<_>>().join(" ");
        if note.is_empty() {
            return;
        }
        if !self
            .notes
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&note))
        {
            self.notes.push(note);
        }
        self.note.clear();
        self.note_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn remove_note(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.notes.len() {
            self.notes.remove(index);
            cx.notify();
        }
    }

    /// Enter adds the typed note in Memory and saves from the goal; in the instructions it is a
    /// newline, and Cmd/Ctrl+Enter saves from anywhere.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewing {
            cx.stop_propagation();
            return;
        }
        let in_instructions = self
            .instructions_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            || self
                .overrides_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
        if in_instructions && !action.secondary {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        let in_note = self.note_input.read(cx).focus_handle(cx).is_focused(window);
        if in_note && !action.secondary {
            self.add_note(window, cx);
            return;
        }
        self.save(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.viewing && !self.opened_on_playbook {
            self.set_viewing(false, window, cx);
            return;
        }
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewing {
            if event.keystroke.key == "escape" {
                self.on_escape_action(&Escape, window, cx);
            }
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.save(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_goal_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(16.0))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_GOAL))
                    .child(modal_text_input(&p, &self.goal_input, false, window, cx)),
            )
            .child(
                v_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_INSTRUCTIONS))
                    .child(modal_text_area(
                        &p,
                        &self.instructions_input,
                        None,
                        false,
                        window,
                        cx,
                    ))
                    .child(modal_hint(
                        &p,
                        "Every thread it starts from now on gets the goal and these instructions with its task.",
                    )),
            )
            .into_any_element()
    }

    fn render_overrides_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut body = v_flex()
            .w_full()
            .gap(px(8.0))
            .child(modal_text_area(
                &p,
                &self.overrides_input,
                None,
                false,
                window,
                cx,
            ))
            .child(modal_hint(&p, super::new_coordinator_modal::OVERRIDES_HINT));
        if let Some(note) = &self.old_copies_note {
            body = body.child(modal_hint(&p, note.clone()));
        }
        body.into_any_element()
    }

    fn render_memory_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut list = v_flex()
            .id("coordinator-memory-notes")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(6.0));
        if self.notes.is_empty() {
            list = list.child(modal_hint(
                &p,
                "No notes yet. The coordinator saves the lasting preferences you confirm, and you can add one below.",
            ));
        }
        for (index, note) in self.notes.iter().enumerate() {
            list = list.child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .bg(hsla(p.raised))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(20.0))
                            .text_size(px(13.0))
                            .text_color(p.muted)
                            .child(format!("{}.", index + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.5))
                            .text_color(p.foreground)
                            .child(note.clone()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "coordinator-memory-remove-{index}"
                            )))
                            .role(gpui::Role::Button)
                            .aria_label(SharedString::from(format!("Remove note {}", index + 1)))
                            .flex_shrink_0()
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .hover(move |this| this.bg(hsla(p.raised_hover)))
                            .on_press(cx, move |this: &mut Self, _window, cx| {
                                this.remove_note(index, cx)
                            })
                            .child(modal_icon("modals/kit/x.svg", 14.0, p.muted)),
                    ),
            );
        }
        let add = modal_action_button(
            &p,
            "coordinator-memory-add",
            ADD,
            None,
            ModalButtonTone::Neutral,
            self.note.trim().is_empty(),
            |this, window, cx| this.add_note(window, cx),
            cx,
        );
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            .child(list)
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .child(div().flex_1().min_w_0().child(modal_text_input(
                        &p,
                        &self.note_input,
                        false,
                        window,
                        cx,
                    )))
                    .child(div().flex_shrink_0().w(px(88.0)).child(add)),
            )
            .child(modal_hint(
                &p,
                "The coordinator passes on the notes a thread needs when it briefs it.",
            ))
            .into_any_element()
    }

    /// The read-only playbook page: Ghostex's part, then the overrides as last saved.
    fn render_playbook_view(&self) -> Vec<AnyElement> {
        let p = self.palette;
        let colors = super::new_coordinator_modal::PlaybookViewColors {
            light: p.light,
            foreground: p.foreground,
            muted: p.muted,
            panel: p.panel,
            hairline: p.hairline,
            destructive: p.destructive,
        };
        vec![
            modal_header(
                &p,
                super::new_coordinator_modal::PLAYBOOK_VIEW_TITLE,
                Some(super::new_coordinator_modal::PLAYBOOK_VIEW_DESCRIPTION),
            ),
            div()
                .id("coordinator-details-playbook-view-scroll")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(super::new_coordinator_modal::playbook_view_body(
                    colors,
                    "coordinator-details-playbook-view",
                    self.guide.as_ref(),
                    &self.overrides,
                ))
                .into_any_element(),
        ]
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        if self.viewing {
            let back = if self.opened_on_playbook {
                modal_action_button(
                    &p,
                    "coordinator-details-playbook-close",
                    "Close",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this, window, cx| this.cancel(window, cx),
                    cx,
                )
            } else {
                modal_action_button(
                    &p,
                    "coordinator-details-playbook-back",
                    "Back",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this, window, cx| this.set_viewing(false, window, cx),
                    cx,
                )
            };
            let edit = modal_action_button(
                &p,
                "coordinator-details-playbook-edit",
                super::new_coordinator_modal::EDIT_OVERRIDES,
                None,
                ModalButtonTone::Primary,
                false,
                |this, window, cx| this.set_viewing(false, window, cx),
                cx,
            );
            return h_flex()
                .w_full()
                .gap(px(8.0))
                .child(div().flex_1())
                .child(h_flex().w(px(96.0)).child(back))
                .child(h_flex().w(px(140.0)).child(edit))
                .into_any_element();
        }
        let cancel = modal_action_button(
            &p,
            "coordinator-details-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let save = modal_action_button(
            &p,
            "coordinator-details-save",
            if self.saving { SAVING } else { SAVE },
            None,
            ModalButtonTone::Primary,
            self.saving,
            |this, window, cx| this.save(window, cx),
            cx,
        );
        if self.mode != CoordinatorDetailsMode::Overrides || self.guide.is_none() {
            return modal_footer(vec![cancel, save]);
        }
        // View playbook on the left, Cancel and Save on the right.
        let view = modal_action_button(
            &p,
            "coordinator-details-playbook-view",
            super::new_coordinator_modal::VIEW_PLAYBOOK,
            None,
            ModalButtonTone::Neutral,
            self.saving,
            |this, window, cx| this.set_viewing(true, window, cx),
            cx,
        );
        h_flex()
            .w_full()
            .gap(px(8.0))
            .child(h_flex().w(px(132.0)).child(view))
            .child(div().flex_1())
            .child(h_flex().w(px(96.0)).child(cancel))
            .child(h_flex().w(px(96.0)).child(save))
            .into_any_element()
    }
}

impl Render for GpuiCoordinatorDetailsModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let (title, body) = match self.mode {
            CoordinatorDetailsMode::Goal => (GOAL_TITLE, self.render_goal_body(window, cx)),
            CoordinatorDetailsMode::Memory => (MEMORY_TITLE, self.render_memory_body(window, cx)),
            CoordinatorDetailsMode::Overrides => {
                (OVERRIDES_TITLE, self.render_overrides_body(window, cx))
            }
        };
        let description = match self.mode {
            CoordinatorDetailsMode::Goal => format!(
                "What {} works toward, and the rules its threads follow.",
                self.title
            ),
            CoordinatorDetailsMode::Memory => format!(
                "Lasting preferences, decisions and pitfalls {} remembers.",
                self.title
            ),
            CoordinatorDetailsMode::Overrides => {
                super::new_coordinator_modal::OVERRIDES_DESCRIPTION.to_string()
            }
        };
        let mut content = if self.viewing {
            self.render_playbook_view()
        } else {
            vec![modal_header(&p, title, Some(description)), body]
        };
        if let Some(error) = &self.error {
            content.push(modal_error(&p, error.clone()));
        }
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-coordinator-details-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiCoordinatorDetailsModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
