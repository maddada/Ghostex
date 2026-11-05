//! Drawing of the Add Project dialog's path steps (machines, sources, browse, new folder,
//! repository, clone destination) and the window root with its keyboard model. The review step
//! is `review.rs`.
use super::super::native_modal_kit::*;
use super::copy::*;
use super::skin::*;
use super::window::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseMoveEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{
    Backspace, Enter, Escape, IndentInline, Input, MoveDown, MoveUp, OutdentInline,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};

/// `edge-fade`: the list fades out over 16px at an edge it can still scroll past.
const EDGE_FADE: f32 = 16.0;

impl GpuiAddProjectModalWindow {
    fn on_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.new_folder_name.is_none() && self.clone_step() != Some(CloneStep::Review) {
            self.move_highlight(-1, cx);
        }
    }

    fn on_move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.new_folder_name.is_none() && self.clone_step() != Some(CloneStep::Review) {
            self.move_highlight(1, cx);
        }
    }

    fn on_enter(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.clone_step() == Some(CloneStep::Review) {
            return;
        }
        self.press_enter(action.secondary, window, cx);
    }

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.clone_step() == Some(CloneStep::Review) {
            return;
        }
        if self.press_backspace(window, cx) {
            cx.stop_propagation();
        }
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.close(window, cx);
    }

    fn on_tab(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cycle_focus(1, window, cx);
    }

    fn on_shift_tab(&mut self, _: &OutdentInline, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cycle_focus(-1, window, cx);
    }

    /// Keys that reach the dialog while a chrome button (not a text field) has focus.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => self.close(window, cx),
            "tab" => {
                let direction = if event.keystroke.modifiers.shift {
                    -1
                } else {
                    1
                };
                self.cycle_focus(direction, window, cx);
            }
            "enter" | "space" => {
                let Some(slot) = self
                    .chrome_focus
                    .iter()
                    .find(|(_, handle)| handle.is_focused(window))
                    .map(|(slot, _)| *slot)
                else {
                    return;
                };
                self.activate_slot(slot, window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Enter or Space on a focused chrome button: the same action as its click.
    fn activate_slot(&mut self, slot: FocusSlot, window: &mut Window, cx: &mut Context<Self>) {
        match slot {
            FocusSlot::PathBack => {
                if self.new_folder_name.is_some() {
                    self.cancel_new_folder(window, cx);
                } else {
                    self.pop_view();
                    self.focus_path_input(window, cx);
                    self.settle(window, cx);
                }
            }
            FocusSlot::ReviewBack | FocusSlot::FooterBack => {
                self.return_to_clone_destination(window, cx)
            }
            FocusSlot::CloneMainOnly => self.set_clone_option(true, window, cx),
            FocusSlot::ShallowClone => self.set_clone_option(false, window, cx),
            FocusSlot::FooterClone => self.submit_clone(window, cx),
            // An Install button installs the provider's CLI; the host never wired
            // `onOpenSourceControlSettings`, so Setup Required only explains itself.
            FocusSlot::SetupRequired(index) => {
                let install = self
                    .derive()
                    .rows
                    .get(index)
                    .and_then(|row| row.setup_required.clone())
                    .and_then(|(source, _, tool)| tool.map(|tool| (source, tool)));
                if let Some((source, tool)) = install {
                    self.start_tool_install(source, tool, cx);
                }
            }
            FocusSlot::PathInput | FocusSlot::BranchInput => {}
        }
    }

    fn render_path_bar(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused =
            gpui::Focusable::focus_handle(self.path_input.read(cx), cx).is_focused(window);
        let show_back = d.is_new_folder_step || d.can_pop_view;
        let leading = if show_back {
            let handle = self.chrome_focus_handle(FocusSlot::PathBack, cx);
            let back_focused = handle.is_focused(window);
            let muted_fill = skin.muted_fill;
            div()
                .id("add-project-back")
                .group("add-project-back")
                .track_focus(&handle)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .w(px(40.0))
                .h_full()
                .border_r_1()
                .border_color(hsla(skin.border_at(0.7)))
                .when(back_focused, |this| this.bg(hsla(muted_fill)))
                .hover(move |this| this.bg(hsla(muted_fill)))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.activate_slot(FocusSlot::PathBack, window, cx);
                }))
                .child(
                    modal_icon(
                        ICON_ARROW_LEFT,
                        16.0,
                        if back_focused {
                            skin.fg()
                        } else {
                            skin.muted()
                        },
                    )
                    .group_hover("add-project-back", {
                        let fg = hsla(skin.fg());
                        move |style| style.text_color(fg)
                    }),
                )
                .into_any_element()
        } else {
            let (icon, color) = if d.is_browsing {
                (ICON_FOLDER_PLUS, skin.muted())
            } else {
                (ICON_SEARCH, skin.muted_at(0.5))
            };
            div()
                .flex_shrink_0()
                .pl(px(12.0))
                .flex()
                .items_center()
                .child(modal_icon(icon, 16.0, color))
                .into_any_element()
        };
        let mut actions: Vec<AnyElement> = Vec::new();
        if d.is_new_folder_step {
            let empty = self
                .new_folder_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty();
            let label = match (self.busy, self.add_new_folder_as_project) {
                (Some(Busy::CreateFolder), _) => "Creating",
                (_, true) => "Create & Add",
                (_, false) => "Create Folder",
            };
            actions.push(self.bar_button(
                skin,
                "add-project-new-folder-submit",
                label,
                None,
                empty || self.busy.is_some(),
                |this, window, cx| this.submit_new_folder(window, cx),
                cx,
            ));
        } else if d.is_repository_step {
            let source = self.clone_flow.as_ref().map(|flow| flow.source);
            let label = if self.busy == Some(Busy::Lookup) {
                "Working"
            } else {
                source.map(repository_action_label).unwrap_or("Continue")
            };
            actions.push(self.bar_button(
                skin,
                "add-project-repository-action",
                label,
                None,
                self.query.trim().is_empty() || self.busy.is_some(),
                |this, window, cx| this.submit_repository(window, cx),
                cx,
            ));
        } else if d.is_browsing {
            actions.push(self.bar_button(
                skin,
                "add-project-new-folder",
                "New Folder",
                Some(ICON_FOLDER_PLUS),
                !d.can_create_new_folder || self.busy.is_some(),
                |this, window, cx| this.start_new_folder(window, cx),
                cx,
            ));
            let label = match self.busy {
                Some(Busy::Add) => "Adding",
                Some(Busy::Clone) => "Cloning",
                Some(Busy::Preview) => "Reviewing",
                Some(Busy::CreateFolder) => "Creating",
                Some(Busy::Lookup) => "Working",
                None => d.submit_action_label,
            };
            actions.push(self.bar_button(
                skin,
                "add-project-submit",
                label,
                (label == "Create & Add").then_some(ICON_FOLDER_CHECK),
                !d.can_submit_browse_path || self.busy.is_some(),
                |this, window, cx| this.submit_resolved_path(window, cx),
                cx,
            ));
        }
        let has_actions = !actions.is_empty();
        let p = skin.p;
        h_flex()
            .w_full()
            .h(px(40.0))
            .items_center()
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(if focused {
                skin.bar_focus_border
            } else {
                skin.bar_border
            }))
            .bg(hsla(skin.bar_background))
            .overflow_hidden()
            .child(leading)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .pl(px(6.0))
                    .pr(px(if has_actions { 6.0 } else { 12.0 }))
                    .child(
                        Input::new(&self.path_input)
                            .with_size(ComponentSize::Small)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(13.0))
                            .text_color(hsla(p.foreground)),
                    ),
            )
            .when(has_actions, |this| {
                this.child(
                    h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.0))
                        .pr(px(6.0))
                        .children(actions),
                )
            })
            .into_any_element()
    }

    /// A path-bar action: a filled pill inset in the bar in the dialog's button fill
    /// (`AddProjectSkin::button_fills`, CDXC:AddProject 2026-10-14), lighter on hover and press. Every action in the bar shares this one look (the 2026-10-02 rule that New
    /// Folder matches the Add button and both read as clickable).
    #[allow(clippy::too_many_arguments)]
    fn bar_button(
        &self,
        skin: &AddProjectSkin,
        id: &'static str,
        label: &'static str,
        icon: Option<&'static str>,
        disabled: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fills = skin.button_fills();
        let text = skin.button_text();
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(28.0))
            .items_center()
            .gap(px(6.0))
            .pl(px(if icon.is_some() { 8.0 } else { 10.0 }))
            .pr(px(10.0))
            .rounded(px(6.0))
            .bg(hsla(fills.rest))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(text))
            .whitespace_nowrap()
            .when(disabled, |this| this.opacity(0.5))
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(fills.hover)))
                    .active(move |this| this.bg(hsla(fills.pressed)))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_click(this, window, cx);
                    }))
            })
            .children(icon.map(|icon| modal_icon(icon, 12.0, text)))
            .child(label)
            .into_any_element()
    }

    fn render_repository_card(&self, skin: &AddProjectSkin) -> Option<AnyElement> {
        let flow = self
            .clone_flow
            .as_ref()
            .filter(|flow| flow.step == CloneStep::Destination)?;
        let name = flow
            .repository
            .as_ref()
            .map(|repository| repository.name_with_owner.clone())
            .unwrap_or_else(|| flow.repository_input.clone());
        let url = flow
            .repository
            .as_ref()
            .map(|repository| repository.url.clone())
            .unwrap_or_else(|| flow.remote_url.clone());
        Some(
            v_flex()
                .flex_shrink_0()
                .min_w_0()
                .mx(px(12.0))
                .mt(px(8.0))
                .gap(px(4.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(skin.border_at(0.6)))
                .child(label_medium(skin, "Repository"))
                .child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_shrink_0().child(modal_icon(
                            source_icon(flow.source),
                            16.0,
                            skin.muted_at(0.8),
                        )))
                        .child(
                            v_flex()
                                .min_w_0()
                                .child(
                                    text_sm(div())
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(hsla(skin.fg()))
                                        .child(name),
                                )
                                .child(
                                    text_sm(div())
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_color(hsla(skin.muted_at(0.85)))
                                        .child(url),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The "still working" notice, with the clone's cancel link while a clone runs.
    pub(super) fn render_slow_notice(
        &self,
        skin: &AddProjectSkin,
        text: impl Into<gpui::SharedString>,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let cloning = self.busy == Some(Busy::Clone);
        let fg = skin.fg();
        h_flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(skin.border_at(0.6)))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(skin.muted()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(text.into()),
            )
            .when(cloning && self.clone_job_id.is_some(), |this| {
                this.child(
                    div()
                        .id("add-project-cancel-clone")
                        .flex_shrink_0()
                        .underline()
                        .cursor_pointer()
                        .hover(move |this| this.text_color(hsla(fg)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.cancel_clone(cx);
                        }))
                        .child("Cancel clone"),
                )
            })
    }

    fn render_row(
        &mut self,
        skin: &AddProjectSkin,
        index: usize,
        row: &AddProjectRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = self.highlight.as_deref() == Some(row.value.as_str());
        let disabled = row.disabled;
        let installing_source = self
            .tool_install
            .as_ref()
            .filter(|install| install.running)
            .map(|install| install.source);
        let trailing = row
            .setup_required
            .as_ref()
            .map(|(source, hint, install_tool)| {
                let handle = self.chrome_focus_handle(FocusSlot::SetupRequired(index), cx);
                let focused = handle.is_focused(window);
                // The dialog's button fill (CDXC:AddProject 2026-10-14).
                let fills = skin.button_fills();
                let hint: SharedString = hint.clone().into();
                let installing = installing_source == Some(*source);
                let label: SharedString = match install_tool {
                    Some(_) if installing => "Installing…".into(),
                    Some(tool) => {
                        format!("Install {}", super::tool_install::tool_label(tool)).into()
                    }
                    None => "Setup Required".into(),
                };
                let install = install_tool
                    .clone()
                    .filter(|_| installing_source.is_none())
                    .map(|tool| (*source, tool));
                let button = div()
                    .id(SharedString::from(format!(
                        "add-project-setup-{}",
                        source.wire()
                    )))
                    .track_focus(&handle)
                    .ml_auto()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .h(px(24.0))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(transparent())
                    .bg(hsla(fills.rest))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(skin.button_text()))
                    .whitespace_nowrap()
                    .hover(move |this| this.bg(hsla(fills.hover)))
                    .active(move |this| this.bg(hsla(fills.pressed)))
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some((source, tool)) = install.clone() {
                            this.start_tool_install(source, tool, cx);
                        }
                    }))
                    .child(label);
                skin.focus_ring(button, focused).into_any_element()
            });
        let value = row.value.clone();
        let action = row.action.clone();
        h_flex()
            .id(("add-project-row", index))
            .w_full()
            .flex_shrink_0()
            .min_h(px(36.0))
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(10.0))
            .items_center()
            .rounded(px(6.0))
            .cursor_default()
            .when(disabled, |this| this.opacity(0.6))
            .when(highlighted, |this| this.bg(hsla(skin.muted_fill)))
            .when(!disabled, |this| {
                this.on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _window, cx| {
                    this.hover_row(&value, cx);
                }))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        this.select_row(action.clone(), window, cx);
                    },
                ))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(row.icon, 16.0, skin.muted_at(0.8))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        text_sm(div())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(hsla(skin.fg()))
                            .child(row.title.clone()),
                    )
                    .children(row.description.clone().map(|description| {
                        text_sm(div())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(hsla(skin.muted_at(0.85)))
                            .child(description)
                    })),
            )
            .children(trailing)
            .into_any_element()
    }

    fn empty_state_message(&self, d: &Derived) -> String {
        if d.path_inspection.as_ref().is_some_and(|inspection| {
            inspection.kind == Some(super::model::AddProjectInspectionKind::File)
        }) && d.suggested_project_path.is_none()
        {
            return "This is a file outside a Git repository. Choose a project folder.".to_string();
        }
        if let Some(name) = &self.new_folder_name {
            // The listing's directory names the parent until gxserver's answer resolves it.
            let parent = if d.new_folder_parent_path.is_empty() {
                &d.browse_directory_path
            } else {
                &d.new_folder_parent_path
            };
            return new_folder_message(name, parent, self.add_new_folder_as_project);
        }
        let clone_step = self.clone_step().map(|step| match step {
            CloneStep::Repository => AddProjectEmptyCloneStep::Repository,
            CloneStep::Destination | CloneStep::Review => AddProjectEmptyCloneStep::Destination,
        });
        empty_state_message(&AddProjectEmptyStateInput {
            clone_step,
            clone_source: self.clone_flow.as_ref().map(|flow| flow.source),
            is_loading_machines: self.is_loading_machines,
            has_machines: !self.machines.is_empty(),
            relative_path_needs_active_project: d.relative_path_needs_active_project,
            unsupported_windows_path: d.unsupported_windows_path,
            will_create_project_path: d.will_create_project_path,
        })
        .to_string()
    }

    fn group_label(&self, d: &Derived) -> &'static str {
        if d.ambiguous.is_some() {
            "Choose how to open this"
        } else if d.is_browsing {
            if d.is_clone_destination_step {
                "Select where to clone"
            } else if self
                .browse_result
                .as_ref()
                .is_some_and(|result| result.is_drive_list)
            {
                "Drives"
            } else {
                "Directories"
            }
        } else if matches!(self.current_view(), Some(AddProjectView::Machines)) {
            "Machines"
        } else {
            "Sources"
        }
    }

    fn render_list(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pending = self.pending_discovery_machine_id.is_some()
            && matches!(self.current_view(), Some(AddProjectView::Sources { .. }));
        let pending_line = pending.then(|| {
            text_sm(div())
                .flex_shrink_0()
                .px(px(8.0))
                .py(px(8.0))
                .text_color(hsla(skin.muted()))
                .child("Checking source control providers...")
                .into_any_element()
        });
        if d.rows.is_empty() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .px(px(8.0))
                .pb(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_h(px(96.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .px(px(24.0))
                        .text_center()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(skin.muted()))
                        .child(self.empty_state_message(d)),
                )
                .children(pending_line)
                .into_any_element();
        }
        let mut children = vec![
            label_medium(skin, self.group_label(d))
                .flex_shrink_0()
                .px(px(8.0))
                .pt(px(12.0))
                .pb(px(6.0))
                .into_any_element(),
        ];
        for (index, row) in d.rows.iter().enumerate() {
            children.push(self.render_row(skin, index, row, window, cx));
        }
        children.extend(pending_line);
        let offset = self.list_scroll.offset().y;
        let max_offset = self.list_scroll.max_offset().y;
        let show_top = f32::from(offset) < -0.5;
        let show_bottom =
            f32::from(max_offset) > 0.5 && f32::from(offset) > -f32::from(max_offset) + 0.5;
        let surface = skin.p.surface;
        let fade = |top: bool| {
            let ramp = div().absolute().left_0().right_0().h(px(EDGE_FADE));
            let ramp = if top { ramp.top_0() } else { ramp.bottom_0() };
            ramp.bg(gpui::linear_gradient(
                if top { 0.0 } else { 180.0 },
                gpui::linear_color_stop(hsla(surface).opacity(0.0), 0.0),
                gpui::linear_color_stop(hsla(surface), 1.0),
            ))
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                v_flex()
                    .id("add-project-list")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.list_scroll)
                    .px(px(8.0))
                    .pb(px(8.0))
                    .children(children),
            )
            // A ramp can only fade into a solid colour; under window glass the list ends cleanly.
            .when(!skin.p.glass && show_top, |this| this.child(fade(true)))
            .when(!skin.p.glass && show_bottom, |this| this.child(fade(false)))
            .into_any_element()
    }

    fn render_footer(&self, skin: &AddProjectSkin, d: &Derived) -> AnyElement {
        let mut hints: Vec<AnyElement> = Vec::new();
        if !d.is_new_folder_step {
            hints.push(footer_hint(skin, "↑ ↓", "Navigate"));
        }
        if d.is_new_folder_step {
            hints.push(footer_hint(skin, "Enter", "Create folder"));
        } else if d.is_repository_step {
            let label = self
                .clone_flow
                .as_ref()
                .map(|flow| repository_action_label(flow.source))
                .unwrap_or("");
            hints.push(footer_hint(skin, "Enter", label));
        } else if d.is_browsing {
            hints.push(footer_hint(
                skin,
                &d.add_shortcut_label,
                d.submit_action_label,
            ));
        } else {
            hints.push(footer_hint(skin, "Enter", "Select"));
        }
        if d.is_new_folder_step {
            hints.push(footer_hint(skin, "Backspace", "Cancel"));
        } else if d.can_pop_view {
            hints.push(footer_hint(skin, "Backspace", "Back"));
        }
        hints.push(footer_hint(skin, "Esc", "Close"));
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(16.0))
            .border_t_1()
            .border_color(hsla(skin.border_at(0.7)))
            .px(px(16.0))
            .py(px(10.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(skin.muted()))
            .children(hints)
            .children(
                d.machine
                    .as_ref()
                    .map(|machine| machine_label(skin, &machine.label)),
            )
            .into_any_element()
    }

    fn render_path_step(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path_bar = self.render_path_bar(skin, d, window, cx);
        let list = self.render_list(skin, d, window, cx);
        v_flex()
            .size_full()
            .min_w_0()
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(12.0))
                    .pt(px(12.0))
                    .child(path_bar),
            )
            .children(self.render_repository_card(skin))
            .children(self.error.as_ref().map(|error| {
                error_region(skin, error)
                    .flex_shrink_0()
                    .mx(px(12.0))
                    .mt(px(8.0))
                    .into_any_element()
            }))
            .children((self.is_slow && self.busy.is_some()).then(|| {
                self.render_slow_notice(skin, "Still working. The machine may be reconnecting.", cx)
                    .flex_shrink_0()
                    .mx(px(12.0))
                    .mt(px(8.0))
                    .into_any_element()
            }))
            .child(list)
            .child(self.render_footer(skin, d))
            .into_any_element()
    }
}

/// The footer's machine label: a 12px folder-plus icon and the machine name, pushed right.
pub(super) fn machine_label(skin: &AddProjectSkin, label: &str) -> AnyElement {
    h_flex()
        .ml_auto()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .flex_shrink_0()
                .child(modal_icon(ICON_FOLDER_PLUS, 12.0, skin.muted())),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label.to_string()),
        )
        .into_any_element()
}

impl Render for GpuiAddProjectModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let skin = AddProjectSkin::resolve(self.palette);
        let d = self.derive();
        let review = self.clone_step() == Some(CloneStep::Review) && self.clone_preview.is_some();
        let body = if review {
            self.render_review(&skin, &d, window, cx)
        } else {
            self.render_path_step(&skin, &d, window, cx)
        };
        let focus_handle = self.focus_handle.clone();
        div()
            .id("add-project-modal")
            .size_full()
            .overflow_hidden()
            .bg(hsla(skin.p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(skin.fg()))
            // Only the review step has no text field to hold focus; the path steps keep the
            // caret in the path input the way the React rows' `preventDefault` does.
            .when(review, |this| this.track_focus(&focus_handle))
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_backspace))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_shift_tab))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(body)
    }
}
