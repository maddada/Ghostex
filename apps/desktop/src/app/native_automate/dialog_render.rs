//! Layout of the create/edit automation dialog, field for field the React `AutomationDialog`.

use super::dialog::{AutomationDialog, DialogField, DialogSelect};
use super::drafts::{ExecutionKind, ScheduleMode, SchedulePreset};
use crate::app::window::{
    ModalButtonTone, ModalSegmentedItem, capture_child_bounds, hsla, modal_action_button,
    modal_error, modal_footer, modal_header, modal_hint, modal_section_title,
    modal_segmented_control, modal_select_menu, modal_select_trigger, modal_shell_scrolling,
    modal_spinner, modal_switch, modal_text_area, modal_text_input,
};
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::Escape;
use gpui_component::{h_flex, v_flex};

impl AutomationDialog {
    /// A labelled field: the kit's 12px label over the control.
    fn field(&self, label: &'static str, control: AnyElement) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .gap(px(6.0))
            .child(modal_section_title(&self.palette, label))
            .child(control)
            .into_any_element()
    }

    fn text_field(
        &self,
        label: &'static str,
        field: DialogField,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.field(
            label,
            modal_text_input(&self.palette, self.input(field), false, window, cx),
        )
    }

    fn select_field(
        &self,
        label: &'static str,
        kind: DialogSelect,
        placeholder: &'static str,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger = h_flex()
            .w_full()
            .on_children_prepainted(capture_child_bounds(
                self.select(kind).trigger_bounds.clone(),
                0,
            ))
            .child(modal_select_trigger(
                &self.palette,
                self.select(kind),
                kind.trigger_id(),
                self.selected_label(kind),
                placeholder,
                disabled,
                move |this: &mut Self, _window, cx| this.toggle_select(kind, cx),
                cx,
            ))
            .into_any_element();
        self.field(label, trigger)
    }

    /// Two fields side by side (`.project-automation-form-grid`).
    fn grid(fields: Vec<AnyElement>) -> AnyElement {
        h_flex()
            .w_full()
            .gap(px(12.0))
            .items_start()
            .children(fields)
            .into_any_element()
    }

    fn section(&self, title: &'static str, children: Vec<AnyElement>) -> AnyElement {
        v_flex()
            .w_full()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(hsla(self.palette.foreground))
                    .child(title),
            )
            .children(children)
            .into_any_element()
    }

    fn render_timing(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.draft.schedule_mode;
        let modes = [
            ScheduleMode::Repeat,
            ScheduleMode::Timer,
            ScheduleMode::Date,
        ];
        let segmented = modal_segmented_control(
            &self.palette,
            "automation-dialog-schedule-mode",
            &[
                ModalSegmentedItem {
                    icon: None,
                    label: "Repeat",
                },
                ModalSegmentedItem {
                    icon: None,
                    label: "Timer",
                },
                ModalSegmentedItem {
                    icon: None,
                    label: "Date",
                },
            ],
            modes
                .iter()
                .position(|candidate| *candidate == mode)
                .unwrap_or(0),
            move |this: &mut Self, index, _window, cx| this.set_schedule_mode(modes[index], cx),
            cx,
        );
        let mut children = vec![segmented];
        match mode {
            ScheduleMode::Repeat => {
                let preset = self.draft.schedule_preset;
                let mut fields =
                    vec![self.select_field("Repeat", DialogSelect::Repeat, "", false, cx)];
                if preset == SchedulePreset::Weekly {
                    fields.push(self.select_field("Day", DialogSelect::Weekday, "", false, cx));
                }
                if preset.uses_time() {
                    fields.push(self.text_field("Time", DialogField::ScheduleTime, window, cx));
                }
                children.push(Self::grid(fields));
                if preset == SchedulePreset::Cron {
                    children.push(self.text_field("Cron", DialogField::Cron, window, cx));
                }
            }
            ScheduleMode::Timer => children.push(Self::grid(vec![
                self.text_field("Run in", DialogField::TimerAmount, window, cx),
                self.select_field("Unit", DialogSelect::TimerUnit, "", false, cx),
            ])),
            ScheduleMode::Date => {
                children.push(self.text_field("Run on", DialogField::RunAt, window, cx));
            }
        }
        self.section("Timing", children)
    }

    fn render_execution(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let kind = self.draft.execution_kind;
        let kinds = [
            ExecutionKind::Worktree,
            ExecutionKind::Local,
            ExecutionKind::Thread,
        ];
        let can_use_worktrees = self.can_use_worktrees();
        let segmented = modal_segmented_control(
            &self.palette,
            "automation-dialog-execution",
            &[
                ModalSegmentedItem {
                    icon: None,
                    label: "Worktree",
                },
                ModalSegmentedItem {
                    icon: None,
                    label: "Local",
                },
                ModalSegmentedItem {
                    icon: None,
                    label: "Thread",
                },
            ],
            kinds
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap_or(0),
            move |this: &mut Self, index, _window, cx| this.set_execution_kind(kinds[index], cx),
            cx,
        );
        let mut children = vec![segmented];
        if !can_use_worktrees && let Some(reason) = self.worktree_unavailable_reason() {
            children.push(modal_hint(&self.palette, reason).into_any_element());
        }
        match kind {
            ExecutionKind::Worktree => {
                children.push(self.text_field(
                    "Setup command",
                    DialogField::SetupCommand,
                    window,
                    cx,
                ));
            }
            ExecutionKind::Thread => {
                let placeholder = if self.sessions.is_empty() {
                    "No sessions in this project"
                } else {
                    "Choose session"
                };
                children.push(Self::grid(vec![
                    self.select_field("Session", DialogSelect::Session, placeholder, false, cx),
                    self.text_field("Expires", DialogField::ExpiresAt, window, cx),
                ]));
                let agent_session_id = self.draft.thread_agent_session_id.trim();
                if !agent_session_id.is_empty() {
                    children.push(
                        modal_hint(
                            &self.palette,
                            format!(
                                "This automation will resume agent conversation {agent_session_id} if its Ghostex pane is closed."
                            ),
                        )
                        .into_any_element(),
                    );
                }
            }
            ExecutionKind::Local => {}
        }
        self.section("Execution", children)
    }

    fn render_enabled(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .id("automation-dialog-enabled")
            .gap(px(8.0))
            .items_center()
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(hsla(self.palette.foreground).opacity(0.85))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_enabled(cx)))
            .child(modal_switch(&self.palette, self.draft.enabled, false))
            .child("Enabled")
            .into_any_element()
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let no_agents = self.state.agents.is_empty();
        let mut top_row = Vec::new();
        if self.all_projects {
            top_row.push(self.select_field(
                "Project",
                DialogSelect::Project,
                "Choose project",
                false,
                cx,
            ));
        }
        top_row.push(self.select_field(
            "Agent",
            DialogSelect::Agent,
            if no_agents {
                "No agents configured"
            } else {
                "Choose agent"
            },
            no_agents,
            cx,
        ));
        v_flex()
            .w_full()
            .gap(px(18.0))
            .pb(px(2.0))
            .child(self.text_field("Name", DialogField::Name, window, cx))
            .child(Self::grid(top_row))
            .child(self.render_timing(window, cx))
            .child(self.render_execution(window, cx))
            .child(self.field(
                "Prompt",
                modal_text_area(&self.palette, &self.prompt, Some(120.0), false, window, cx),
            ))
            .child(self.render_enabled(cx))
            .children(
                self.error
                    .clone()
                    .map(|error| modal_error(&self.palette, error)),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let cancel = modal_action_button(
            &self.palette,
            "automation-dialog-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, window, cx| this.cancel(window, cx),
            cx,
        );
        let save = modal_action_button(
            &self.palette,
            "automation-dialog-save",
            "Save",
            self.saving
                .then(|| modal_spinner(self.palette.primary_foreground)),
            ModalButtonTone::Primary,
            self.saving,
            |this: &mut Self, window, cx| this.save(window, cx),
            cx,
        );
        modal_footer(vec![cancel, save])
    }

    fn open_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let kind = DialogSelect::ALL
            .into_iter()
            .find(|kind| self.select(*kind).open)?;
        let items = self.items(kind);
        modal_select_menu(
            &self.palette,
            self.select(kind),
            kind.menu_id(),
            &items,
            self.selected_index(kind),
            move |this: &mut Self, index, _window, cx| this.choose(kind, index, cx),
            move |this: &mut Self, _window, cx| this.close_select(kind, cx),
            window,
            cx,
        )
    }
}

impl Render for AutomationDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let description = if self.all_projects {
            "Schedule agent work once or repeatedly for a selected project.".to_string()
        } else {
            format!(
                "Schedule agent work once or repeatedly for {}.",
                self.project_name
            )
        };
        let title = if self.draft.id.is_some() {
            "Edit automation"
        } else {
            "Create automation"
        };
        let header = modal_header(&self.palette, title, Some(description));
        let body = self.render_body(window, cx);
        let footer = self.render_footer(cx);
        let menu = self.open_menu(window, cx);
        modal_shell_scrolling(
            &self.palette,
            "ghostex-gpui-automation-dialog",
            "ghostex-gpui-automation-dialog-body",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            header,
            body,
            Some(footer),
            menu,
            cx,
        )
        .capture_action(cx.listener(|this, _: &Escape, window, cx| {
            cx.stop_propagation();
            if let Some(kind) = DialogSelect::ALL
                .into_iter()
                .find(|kind| this.select(*kind).open)
            {
                this.close_select(kind, cx);
            } else {
                this.cancel(window, cx);
            }
        }))
    }
}
