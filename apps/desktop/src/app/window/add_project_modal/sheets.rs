//! The picker's side panels, drawn where the preview is: Clone a repo (the five sources with their
//! readiness and CLI install, the repository, where it goes, the branch and the two clone options,
//! then the clone's progress) and New project (the name, where it is made, Space, Group and Start,
//! with each field's list). The folders stay on the left, so the destination is picked there.
use super::super::native_modal_kit::*;
use super::copy::*;
use super::new_project::*;
use super::paths::is_repository_clone_branch_name_input_valid;
use super::render::SHEET_WIDTH;
use super::skin::*;
use super::window::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};

impl GpuiAddProjectModalWindow {
    fn sheet_label(&self, skin: &AddProjectSkin, text: impl Into<SharedString>) -> AnyElement {
        div()
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(skin.muted()))
            .child(text.into())
            .into_any_element()
    }

    fn sheet_hint(
        &self,
        skin: &AddProjectSkin,
        text: impl Into<SharedString>,
        bad: bool,
    ) -> AnyElement {
        div()
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(hsla(if bad { skin.destructive } else { skin.muted() }))
            .child(text.into())
            .into_any_element()
    }

    fn sheet_field(
        &self,
        skin: &AddProjectSkin,
        input: &Entity<InputState>,
        icon: Option<&'static str>,
        invalid: bool,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = gpui::Focusable::focus_handle(input.read(cx), cx).is_focused(window);
        h_flex()
            .h(px(32.0))
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(if invalid {
                skin.destructive
            } else if focused {
                skin.bar_focus_border
            } else {
                skin.bar_border
            }))
            .bg(hsla(skin.bar_background))
            .when(disabled, |this| this.opacity(0.5))
            .children(icon.map(|icon| {
                div()
                    .flex_shrink_0()
                    .child(modal_icon(icon, 14.0, skin.muted()))
            }))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(input)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .disabled(disabled)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(13.0))
                        .text_color(hsla(skin.fg())),
                ),
            )
            .into_any_element()
    }

    /// The folder something is made in or cloned into, shown in full.
    fn destination_box(
        &self,
        skin: &AddProjectSkin,
        text: impl Into<SharedString>,
        blocked: bool,
    ) -> AnyElement {
        div()
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(if blocked {
                css_fade(skin.destructive, 0.6)
            } else {
                skin.border_at(0.8)
            }))
            .bg(hsla(skin.p.panel))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(skin.fg()))
            .child(text.into())
            .into_any_element()
    }

    fn sheet_header(
        &self,
        skin: &AddProjectSkin,
        icon: &'static str,
        title: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .h(px(44.0))
            .items_center()
            .gap(px(8.0))
            .pl(px(16.0))
            .pr(px(8.0))
            .border_b_1()
            .border_color(hsla(skin.border_at(0.7)))
            .child(modal_icon(icon, 15.0, skin.fg()))
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(skin.fg()))
                    .child(title.into()),
            )
            .child(self.icon_button(
                skin,
                "add-project-sheet-close",
                ICON_X,
                self.busy == Some(Busy::Clone),
                std::rc::Rc::new(
                    |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.close_sheet(window, cx)
                    },
                ),
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_sheet(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (header, body, footer) = match self.sheet {
            Some(Sheet::Clone) => {
                let header = self.sheet_header(skin, ICON_GIT_BRANCH, "Clone a repo", cx);
                let (body, footer) = self.clone_sheet(skin, d, window, cx);
                (header, body, footer)
            }
            Some(Sheet::NewProjectPick(field)) => {
                let header = self.sheet_header(
                    skin,
                    ICON_ARROW_LEFT,
                    new_project_group_label(Some(field)),
                    cx,
                );
                (header, self.pick_sheet(skin, field, cx), None)
            }
            _ => {
                let header = self.sheet_header(skin, ICON_PLUS, "New project", cx);
                let (body, footer) = self.new_project_sheet(skin, window, cx);
                (header, body, footer)
            }
        };
        v_flex()
            .w(px(SHEET_WIDTH))
            .h_full()
            .child(header)
            .child(
                div()
                    .id("add-project-sheet-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.sheet_scroll)
                    .child(
                        v_flex()
                            .gap(px(14.0))
                            .px(px(16.0))
                            .py(px(14.0))
                            .children(body),
                    ),
            )
            .children(footer.map(|buttons| {
                h_flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_t_1()
                    .border_color(hsla(skin.border_at(0.7)))
                    .children(buttons)
            }))
            .into_any_element()
    }

    // -- clone a repo ---------------------------------------------------------------------------

    fn clone_sheet(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, Option<Vec<AnyElement>>) {
        let source = self.clone_source;
        let readiness = d.readiness[source.index()].clone();
        let busy = self.busy.is_some();
        let cloning = self.busy == Some(Busy::Clone);
        let mut body: Vec<AnyElement> = Vec::new();
        // The sources: GitHub first, then Git URL, then the other providers (ready first).
        let hover = skin.muted_fill_at(0.7);
        let sources: Vec<AnyElement> = ordered_sources(&d.readiness)
            .into_iter()
            .map(|candidate| {
                let chosen = candidate == source;
                let ready = d.readiness[candidate.index()].ready;
                h_flex()
                    .id(SharedString::from(format!(
                        "add-project-source-{}",
                        candidate.wire()
                    )))
                    .flex_shrink_0()
                    .h(px(28.0))
                    .items_center()
                    .gap(px(6.0))
                    .px(px(9.0))
                    .rounded(px(6.0))
                    .text_size(px(12.5))
                    .text_color(hsla(skin.fg()))
                    .bg(hsla(if chosen {
                        skin.muted_fill
                    } else {
                        skin.p.raised
                    }))
                    .when(chosen, |this| {
                        this.border_1().border_color(hsla(skin.border_at(1.0)))
                    })
                    .when(!ready, |this| this.opacity(0.6))
                    .when(!busy, |this| {
                        this.hover(move |this| this.bg(hsla(hover)))
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.choose_clone_source(candidate);
                                this.settle(window, cx);
                            }))
                    })
                    .child(modal_icon(source_icon(candidate), 13.0, skin.fg()))
                    .child(source_label(candidate))
                    .into_any_element()
            })
            .collect();
        body.push(
            v_flex()
                .gap(px(6.0))
                .child(self.sheet_label(skin, "From"))
                .child(h_flex().flex_wrap().gap(px(4.0)).children(sources))
                .into_any_element(),
        );
        if self.pending_discovery_machine_id.is_some() {
            body.push(self.sheet_hint(skin, "Checking source control providers...", false));
        }
        if !readiness.ready {
            let install_status = self
                .tool_install
                .as_ref()
                .filter(|install| install.source == source)
                .and_then(|install| install.status_line());
            let hint = install_status
                .or(readiness.hint.clone())
                .unwrap_or_default();
            let installing = self
                .tool_install
                .as_ref()
                .is_some_and(|install| install.running);
            let install = readiness.install_tool.clone().map(|tool| {
                let label = if installing {
                    "Installing…".to_string()
                } else {
                    format!("Install {}", super::tool_install::tool_label(&tool))
                };
                self.filled_button(
                    skin,
                    "add-project-install-tool",
                    label,
                    None,
                    true,
                    installing,
                    std::rc::Rc::new(
                        move |this: &mut Self, _window: &mut Window, cx: &mut Context<Self>| {
                            this.start_tool_install(source, tool.clone(), cx);
                        },
                    ),
                    cx,
                )
            });
            body.push(
                v_flex()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(skin.border_at(0.6)))
                    .bg(hsla(skin.p.panel))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(17.0))
                            .text_color(hsla(skin.fg()))
                            .child(hint),
                    )
                    .children(install.map(|button| div().child(button)))
                    .into_any_element(),
            );
        }
        body.push(
            v_flex()
                .gap(px(6.0))
                .child(self.sheet_label(skin, "Repository"))
                .child(self.sheet_field(
                    skin,
                    &self.repo_input.clone(),
                    Some(source_icon(source)),
                    false,
                    busy,
                    window,
                    cx,
                ))
                .into_any_element(),
        );
        let destination = self.clone_destination(d);
        let blocked = self
            .clone_preview
            .as_ref()
            .is_some_and(|preview| preview.destination_blocked);
        let mut destination_lines = vec![
            self.sheet_label(skin, "Clone into"),
            self.destination_box(
                skin,
                destination
                    .as_deref()
                    .map(|path| self.tildify(path))
                    .unwrap_or_else(|| "Pick a folder on the left".to_string()),
                blocked,
            ),
        ];
        if let Some(preview) = &self.clone_preview {
            if blocked {
                destination_lines.push(self.sheet_hint(
                    skin,
                    preview.warning.clone().unwrap_or_else(|| {
                        "Choose a different destination before cloning.".to_string()
                    }),
                    true,
                ));
            } else if let Some(warning) = &preview.warning {
                destination_lines.push(self.sheet_hint(skin, warning.clone(), false));
            }
        } else {
            destination_lines.push(self.sheet_hint(
                skin,
                "Click folders on the left to clone somewhere else.",
                false,
            ));
        }
        body.push(
            v_flex()
                .gap(px(6.0))
                .children(destination_lines)
                .into_any_element(),
        );
        let invalid_branch =
            !is_repository_clone_branch_name_input_valid(&self.clone_options.branch_name);
        body.push(
            v_flex()
                .gap(px(6.0))
                .child(self.sheet_label(skin, "Branch"))
                .child(self.sheet_field(
                    skin,
                    &self.branch_input.clone(),
                    Some(ICON_GIT_BRANCH),
                    invalid_branch,
                    busy,
                    window,
                    cx,
                ))
                .child(self.sheet_hint(
                    skin,
                    if invalid_branch {
                        "Enter a valid Git branch name."
                    } else {
                        "Leave empty to use the repository default branch."
                    },
                    invalid_branch,
                ))
                .into_any_element(),
        );
        body.push(self.clone_option(
            skin,
            "add-project-clone-main-only",
            self.clone_options.clone_main_only,
            "Clone branch only",
            "Fetch only the selected branch.",
            true,
            cx,
        ));
        body.push(self.clone_option(
            skin,
            "add-project-shallow-clone",
            self.clone_options.shallow_clone,
            "Shallow clone",
            "Fetch only the latest commit history.",
            false,
            cx,
        ));
        if cloning {
            let progress = self
                .clone_progress
                .clone()
                .unwrap_or_else(|| "Cloning…".to_string());
            let can_cancel = self.clone_job_id.is_some();
            body.push(
                v_flex()
                    .gap(px(6.0))
                    .child(self.sheet_hint(skin, progress, false))
                    .when(self.is_slow, |this| {
                        this.child(self.sheet_hint(
                            skin,
                            "Still cloning. The machine may be reconnecting.",
                            false,
                        ))
                    })
                    .when(can_cancel, |this| {
                        this.child(div().child(self.filled_button(
                            skin,
                            "add-project-cancel-clone",
                            "Cancel clone",
                            None,
                            true,
                            false,
                            std::rc::Rc::new(
                                |this: &mut Self, _window: &mut Window, cx: &mut Context<Self>| {
                                    this.cancel_clone(cx)
                                },
                            ),
                            cx,
                        )))
                    })
                    .into_any_element(),
            );
        } else if self.is_slow && busy {
            body.push(self.sheet_hint(
                skin,
                "Still working. The machine may be reconnecting.",
                false,
            ));
        }
        let label = match self.busy {
            Some(Busy::Lookup) => "Looking up",
            Some(Busy::Preview) => "Checking",
            Some(Busy::Clone) => "Cloning",
            _ => "Clone & Add",
        };
        let can_submit = readiness.ready
            && !busy
            && !blocked
            && !invalid_branch
            && !self.clone_repo_text.trim().is_empty()
            && destination.is_some();
        let footer = vec![
            self.filled_button(
                skin,
                "add-project-clone-cancel",
                "Cancel",
                None,
                false,
                cloning,
                std::rc::Rc::new(
                    |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.close_sheet(window, cx)
                    },
                ),
                cx,
            ),
            self.filled_button(
                skin,
                "add-project-clone-submit",
                label,
                None,
                false,
                !can_submit,
                std::rc::Rc::new(
                    |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.submit_clone_sheet(window, cx)
                    },
                ),
                cx,
            ),
        ];
        (body, Some(footer))
    }

    #[allow(clippy::too_many_arguments)]
    fn clone_option(
        &self,
        skin: &AddProjectSkin,
        id: &'static str,
        checked: bool,
        title: &'static str,
        description: &'static str,
        main_only: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = self.busy.is_some();
        h_flex()
            .id(id)
            .items_start()
            .gap(px(9.0))
            .when(!disabled, |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.set_clone_option(main_only, window, cx);
                    }))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .mt(px(1.0))
                    .when(disabled, |this| this.opacity(0.5))
                    .child(square_checkbox(skin, checked, false)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(skin.fg()))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(hsla(skin.muted()))
                            .child(description),
                    ),
            )
            .into_any_element()
    }

    // -- new project ------------------------------------------------------------------------------

    fn form_row(
        &self,
        skin: &AddProjectSkin,
        index: usize,
        row: &FormRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = skin.muted_fill_at(0.6);
        let action = row.action.clone();
        let clickable = action.is_some() && self.busy.is_none();
        h_flex()
            .id(("add-project-form-row", index))
            .w_full()
            .min_h(px(40.0))
            .items_center()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .bg(hsla(skin.p.raised))
            .when(clickable, |this| {
                this.hover(move |this| this.bg(hsla(hover)))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        match action.clone() {
                            Some(FormAction::Pick(field)) => this.open_new_project_pick(field),
                            Some(FormAction::Choose { field, value }) => {
                                this.choose_new_project_value(field, value)
                            }
                            None => {}
                        }
                        this.settle(window, cx);
                    }))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(row.icon, 15.0, skin.muted())),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(hsla(skin.fg()))
                            .child(row.title.clone()),
                    )
                    .children(row.description.clone().map(|description| {
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(hsla(skin.muted()))
                            .child(description)
                    })),
            )
            .when(matches!(row.action, Some(FormAction::Pick(_))), |this| {
                this.child(modal_icon(ICON_CHEVRON_RIGHT, 12.0, skin.muted()))
            })
            .into_any_element()
    }

    fn new_project_sheet(
        &mut self,
        skin: &AddProjectSkin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, Option<Vec<AnyElement>>) {
        let problem = self
            .new_project
            .as_ref()
            .and_then(|form| form.name_problem());
        let typed = self
            .new_project
            .as_ref()
            .is_some_and(|form| !form.name.trim().is_empty());
        let busy = self.busy.is_some();
        let mut body = vec![
            v_flex()
                .gap(px(6.0))
                .child(self.sheet_label(skin, "Project name"))
                .child(self.sheet_field(
                    skin,
                    &self.project_name_input.clone(),
                    None,
                    typed && problem.is_some(),
                    busy,
                    window,
                    cx,
                ))
                .when(typed && problem.is_some(), |this| {
                    this.child(self.sheet_hint(skin, problem.unwrap_or_default(), true))
                })
                .into_any_element(),
        ];
        let rows = self.build_new_project_rows();
        body.push(
            v_flex()
                .gap(px(6.0))
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(index, row)| self.form_row(skin, index, row, cx)),
                )
                .into_any_element(),
        );
        body.push(self.sheet_hint(
            skin,
            "Open another folder on the left to make the project there.",
            false,
        ));
        let footer = vec![
            self.filled_button(
                skin,
                "add-project-new-project-cancel",
                "Cancel",
                None,
                false,
                false,
                std::rc::Rc::new(
                    |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.close_sheet(window, cx)
                    },
                ),
                cx,
            ),
            self.filled_button(
                skin,
                "add-project-new-project-create",
                if busy { "Creating" } else { "Create" },
                Some(ICON_FOLDER_CHECK),
                false,
                problem.is_some() || busy,
                std::rc::Rc::new(
                    |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.submit_new_project(window, cx)
                    },
                ),
                cx,
            ),
        ];
        (body, Some(footer))
    }

    fn pick_sheet(
        &mut self,
        skin: &AddProjectSkin,
        field: NewProjectField,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let rows = self.build_new_project_pick_rows(field);
        vec![
            v_flex()
                .gap(px(6.0))
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(index, row)| self.form_row(skin, index, row, cx)),
                )
                .into_any_element(),
        ]
    }
}
