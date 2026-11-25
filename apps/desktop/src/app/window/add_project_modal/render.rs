//! Drawing of the Add Project picker: the toolbar with its one path field, the sidebar of places
//! and computers, the folder columns with the preview of the selected folder, Suggested, the
//! notices and the footer, plus the window root with its keyboard routing. The side panels
//! (Clone a repo, New project) are `sheets.rs`.
use super::super::native_modal_kit::*;
use super::keyboard::Field;
use super::model::*;
use super::paths::*;
use super::skin::*;
use super::steps::folder_contains;
use super::window::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement as _, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, canvas, div, px,
};
use gpui_component::input::{Enter, Escape, IndentInline, Input, MoveDown, MoveUp};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};

pub(super) const SIDEBAR_WIDTH: f32 = 212.0;
pub(super) const PREVIEW_WIDTH: f32 = 250.0;
pub(super) const SHEET_WIDTH: f32 = 300.0;
const ROW_HEIGHT: f32 = 28.0;

pub(super) type Action = std::rc::Rc<
    dyn Fn(&mut GpuiAddProjectModalWindow, &mut Window, &mut Context<GpuiAddProjectModalWindow>),
>;

pub(super) fn action(
    f: impl Fn(&mut GpuiAddProjectModalWindow, &mut Window, &mut Context<GpuiAddProjectModalWindow>)
    + 'static,
) -> Action {
    std::rc::Rc::new(f)
}

/// A sidebar row: what it shows and what a click does.
struct SidebarItem {
    icon: &'static str,
    label: String,
    note: Option<String>,
    active: bool,
    on_click: Action,
    /// A pinned folder's path, which gets an unpin button.
    unpin: Option<String>,
}

fn platform_word(platform: Option<&str>) -> &'static str {
    match platform.unwrap_or_default() {
        p if p.starts_with("Win") => "Windows",
        p if p.starts_with("Mac") || p == "darwin" => "macOS",
        "" => "",
        _ => "Linux",
    }
}

fn last_segment(path: &str) -> String {
    let trimmed = trim_folder(path);
    trimmed
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(&trimmed)
        .to_string()
}

impl GpuiAddProjectModalWindow {
    // -- keyboard routing -----------------------------------------------------------------------

    fn on_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.focused_field(window, cx) == Some(Field::Repository)
            && self.move_repo_highlight(-1, cx)
        {
            cx.stop_propagation();
            return;
        }
        if matches!(self.focused_field(window, cx), Some(Field::Path) | None) {
            cx.stop_propagation();
            self.move_highlight(-1, cx);
        }
    }

    fn on_move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.focused_field(window, cx) == Some(Field::Repository)
            && self.move_repo_highlight(1, cx)
        {
            cx.stop_propagation();
            return;
        }
        if matches!(self.focused_field(window, cx), Some(Field::Path) | None) {
            cx.stop_propagation();
            self.move_highlight(1, cx);
        }
    }

    fn on_enter(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        match self.focused_field(window, cx) {
            Some(Field::FolderName) => self.submit_new_folder(window, cx),
            Some(Field::Repository) => {
                if !self.press_repo_enter(window, cx) {
                    self.submit_clone_sheet(window, cx);
                }
            }
            Some(Field::Branch) => self.submit_clone_sheet(window, cx),
            Some(Field::ProjectName) => self.submit_new_project(window, cx),
            Some(Field::Path) | None => self.press_enter(action.secondary, window, cx),
        }
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.press_escape(window, cx);
    }

    fn on_tab(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        if self.focused_field(window, cx) == Some(Field::Path) {
            cx.stop_propagation();
            self.complete_path(window, cx);
        }
    }

    /// The picker's shortcuts: New folder, Back, Forward and Up.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let key = keystroke.key.as_str();
        let alt_nav = cfg!(not(target_os = "macos")) && modifiers.alt;
        let handled = if modifiers.secondary(cx) && modifiers.shift && key == "n" {
            self.start_new_folder(window, cx);
            true
        } else if (modifiers.secondary(cx) && key == "[") || (alt_nav && key == "left") {
            self.go_back();
            self.settle(window, cx);
            true
        } else if (modifiers.secondary(cx) && key == "]") || (alt_nav && key == "right") {
            self.go_forward();
            self.settle(window, cx);
            true
        } else if ((cfg!(target_os = "macos") && modifiers.platform) || alt_nav) && key == "up" {
            let d = self.derive();
            self.go_up(&d);
            self.settle(window, cx);
            true
        } else {
            false
        };
        if handled {
            cx.stop_propagation();
        }
    }

    // -- small pieces -----------------------------------------------------------------------------

    /// A 28px icon button (Back, Forward, Up, a panel's close).
    pub(super) fn icon_button(
        &self,
        skin: &AddProjectSkin,
        id: &'static str,
        icon: &'static str,
        disabled: bool,
        on_click: Action,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = skin.muted_fill_at(0.7);
        let color = if disabled {
            skin.muted_at(0.4)
        } else {
            skin.muted()
        };
        div()
            .id(id)
            .flex_shrink_0()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .when(!disabled, |this| {
                this.hover(move |this| this.bg(hsla(hover)))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_click(this, window, cx);
                    }))
            })
            .child(modal_icon(icon, 16.0, color))
            .into_any_element()
    }

    /// A filled button in the dialog's one button fill (CDXC:AddProject 2026-10-14).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn filled_button(
        &self,
        skin: &AddProjectSkin,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        icon: Option<&'static str>,
        small: bool,
        disabled: bool,
        on_click: Action,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fills = skin.button_fills();
        let text = skin.button_text();
        h_flex()
            .id(id.into())
            .flex_shrink_0()
            .h(px(if small { 26.0 } else { 30.0 }))
            .items_center()
            .gap(px(6.0))
            .px(px(if small { 9.0 } else { 12.0 }))
            .rounded(px(7.0))
            .bg(hsla(fills.rest))
            .text_size(px(if small { 12.5 } else { 13.0 }))
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
            .children(icon.map(|icon| modal_icon(icon, 13.0, text)))
            .child(label.into())
            .into_any_element()
    }

    /// A toolbar command (New folder, New project, Clone a repo): icon and label, a fill while its
    /// panel is open.
    #[allow(clippy::too_many_arguments)]
    fn command_button(
        &self,
        skin: &AddProjectSkin,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        pressed: bool,
        disabled: bool,
        on_click: Action,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = skin.muted_fill_at(0.7);
        let pressed_fill = skin.muted_fill;
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(30.0))
            .items_center()
            .gap(px(6.0))
            .px(px(9.0))
            .rounded(px(6.0))
            .text_size(px(13.0))
            .text_color(hsla(skin.fg()))
            .whitespace_nowrap()
            .when(pressed, |this| this.bg(hsla(pressed_fill)))
            .when(disabled, |this| this.opacity(0.4))
            .when(!disabled, |this| {
                this.hover(move |this| this.bg(hsla(hover)))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_click(this, window, cx);
                    }))
            })
            .child(modal_icon(icon, 15.0, skin.muted()))
            .child(label)
            .into_any_element()
    }

    /// A one-line notice over the folders, with buttons at its end.
    fn notice(
        &self,
        skin: &AddProjectSkin,
        icon: &'static str,
        text: impl Into<SharedString>,
        buttons: Vec<AnyElement>,
    ) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .min_w_0()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(skin.border_at(0.6)))
            .bg(hsla(skin.p.panel))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(skin.fg()))
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(icon, 14.0, skin.muted())),
            )
            .child(div().flex_1().min_w_0().child(text.into()))
            .children(buttons)
            .into_any_element()
    }

    // -- toolbar ----------------------------------------------------------------------------------

    fn render_toolbar(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused =
            gpui::Focusable::focus_handle(self.path_input.read(cx), cx).is_focused(window);
        let can_go_up = !self.suggested_mode
            && d.selected_folder
                .as_deref()
                .and_then(|folder| self.parent_folder(folder, &d.platform))
                .is_some();
        let busy = self.busy.is_some();
        let p = skin.p;
        let path_field = h_flex()
            .flex_1()
            .min_w_0()
            .h(px(32.0))
            .items_center()
            .gap(px(8.0))
            .pl(px(10.0))
            .pr(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(if focused {
                skin.bar_focus_border
            } else {
                skin.bar_border
            }))
            .bg(hsla(skin.bar_background))
            .child(div().flex_shrink_0().child(modal_icon(
                if d.clone_detected.is_some() {
                    ICON_GIT_BRANCH
                } else {
                    ICON_FOLDER
                },
                15.0,
                skin.muted(),
            )))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.path_input)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .disabled(busy)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(13.0))
                        .text_color(hsla(p.foreground)),
                ),
            );
        h_flex()
            .flex_shrink_0()
            .h(px(52.0))
            .items_center()
            .gap(px(4.0))
            .px(px(10.0))
            .border_b_1()
            .border_color(hsla(skin.border_at(0.7)))
            .child(self.icon_button(
                skin,
                "add-project-back",
                ICON_ARROW_LEFT,
                self.history_back.is_empty() || busy,
                action(|this, window, cx| {
                    this.go_back();
                    this.settle(window, cx);
                }),
                cx,
            ))
            .child(self.icon_button(
                skin,
                "add-project-forward",
                ICON_ARROW_RIGHT,
                self.history_forward.is_empty() || busy,
                action(|this, window, cx| {
                    this.go_forward();
                    this.settle(window, cx);
                }),
                cx,
            ))
            .child(self.icon_button(
                skin,
                "add-project-up",
                ICON_ARROW_UP,
                !can_go_up || busy,
                action(|this, window, cx| {
                    let d = this.derive();
                    this.go_up(&d);
                    this.settle(window, cx);
                }),
                cx,
            ))
            .child(div().w(px(4.0)))
            .child(path_field)
            .child(div().w(px(6.0)))
            .child(self.command_button(
                skin,
                "add-project-new-folder",
                ICON_FOLDER_PLUS,
                "New folder",
                self.new_folder_name.is_some(),
                !d.can_create_new_folder || busy || self.suggested_mode,
                action(|this, window, cx| this.start_new_folder(window, cx)),
                cx,
            ))
            .child(self.command_button(
                skin,
                "add-project-new-project",
                ICON_PLUS,
                "New project",
                matches!(
                    self.sheet,
                    Some(Sheet::NewProject | Sheet::NewProjectPick(_))
                ),
                d.machine_id.is_none() || busy,
                action(|this, window, cx| {
                    if matches!(
                        this.sheet,
                        Some(Sheet::NewProject | Sheet::NewProjectPick(_))
                    ) {
                        this.sheet = Some(Sheet::NewProject);
                        this.close_sheet(window, cx);
                    } else {
                        this.open_new_project_sheet(window, cx);
                    }
                }),
                cx,
            ))
            .child(self.command_button(
                skin,
                "add-project-clone",
                ICON_GIT_BRANCH,
                "Clone a repo",
                self.sheet == Some(Sheet::Clone),
                d.machine_id.is_none() || busy,
                action(|this, window, cx| {
                    if this.sheet == Some(Sheet::Clone) {
                        this.close_sheet(window, cx);
                    } else {
                        this.open_clone_sheet(None, window, cx);
                    }
                }),
                cx,
            ))
            .into_any_element()
    }

    // -- sidebar ----------------------------------------------------------------------------------

    fn sidebar_items(&self, d: &Derived) -> Vec<(Option<&'static str>, Vec<SidebarItem>)> {
        let mut sections: Vec<(Option<&'static str>, Vec<SidebarItem>)> = Vec::new();
        let places = self.current_places().cloned();
        let active_folder = if self.suggested_mode {
            None
        } else {
            d.selected_folder.clone()
        };
        let is_active = |path: &str| active_folder.as_deref() == Some(trim_folder(path).as_str());
        let place = |icon: &'static str, label: String, path: String, note: Option<String>| {
            let active = is_active(&path);
            let target = path.clone();
            SidebarItem {
                icon,
                label,
                note,
                active,
                on_click: action(move |this, window, cx| {
                    this.go_to_folder(&target, Some(target.clone()));
                    this.settle(window, cx);
                }),
                unpin: None,
            }
        };
        let mut top = Vec::new();
        if let Some(places) = &places {
            let count = places.suggested.len();
            top.push(SidebarItem {
                icon: ICON_SPARKLES,
                label: "Suggested".to_string(),
                note: (count > 0).then(|| count.to_string()),
                active: self.suggested_mode,
                on_click: action(|this, window, cx| {
                    this.open_suggested();
                    this.settle(window, cx);
                }),
                unpin: None,
            });
            top.push(place(
                ICON_HOME,
                "Home".to_string(),
                places.home.clone(),
                None,
            ));
        } else {
            // An older machine, or one still answering: home and the disk's root.
            let home = place(ICON_HOME, "Home".to_string(), "~".to_string(), None);
            top.push(SidebarItem {
                active: !self.suggested_mode && self.query == "~/",
                ..home
            });
        }
        sections.push((None, top));
        let windows = d.platform.starts_with("Win");
        match &places {
            Some(places) => {
                if !places.volumes.is_empty() {
                    let items = places
                        .volumes
                        .iter()
                        .map(|volume| {
                            let icon = if volume.kind.as_deref() == Some("wsl") {
                                ICON_TERMINAL
                            } else {
                                ICON_DISK
                            };
                            let note = match volume.kind.as_deref() {
                                Some("external") => Some("External".to_string()),
                                Some("network") => Some("Network".to_string()),
                                _ => None,
                            };
                            place(icon, volume.name.clone(), volume.path.clone(), note)
                        })
                        .collect();
                    sections.push((Some(if windows { "Drives" } else { "Locations" }), items));
                }
                if !places.standard.is_empty() {
                    sections.push((
                        Some("Folders"),
                        places
                            .standard
                            .iter()
                            .map(|folder| {
                                place(ICON_FOLDER, folder.name.clone(), folder.path.clone(), None)
                            })
                            .collect(),
                    ));
                }
            }
            None if windows => {
                let root = "/".to_string();
                sections.push((
                    Some("Drives"),
                    vec![place(ICON_DISK, "All drives".to_string(), root, None)],
                ));
            }
            None => {
                sections.push((
                    Some("Locations"),
                    vec![place(ICON_DISK, "/".to_string(), "/".to_string(), None)],
                ));
            }
        }
        // Project folders: the Default projects folder first, then the usual ones.
        let mut project = Vec::new();
        let default = d
            .machine
            .as_ref()
            .and_then(|machine| machine.add_project_base_directory.as_deref())
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
            .map(|folder| {
                expand_home(
                    &trim_folder(folder),
                    places.as_ref().map(|places| places.home.as_str()),
                )
            });
        if let Some(default) = &default {
            project.push(place(
                ICON_FOLDER,
                last_segment(default),
                default.clone(),
                Some("Default".to_string()),
            ));
        }
        if let Some(places) = &places {
            for folder in &places.project_folders {
                if default.as_deref().map(trim_folder) == Some(trim_folder(&folder.path)) {
                    continue;
                }
                project.push(place(
                    ICON_FOLDER,
                    folder.name.clone(),
                    folder.path.clone(),
                    None,
                ));
            }
        }
        if !project.is_empty() {
            sections.push((Some("Project folders"), project));
        }
        if let Some(places) = &places {
            if !places.pinned.is_empty() {
                sections.push((
                    Some("Pinned"),
                    places
                        .pinned
                        .iter()
                        .map(|folder| SidebarItem {
                            unpin: Some(folder.path.clone()),
                            ..place(ICON_PIN, folder.name.clone(), folder.path.clone(), None)
                        })
                        .collect(),
                ));
            }
            if !places.other_apps.is_empty() {
                sections.push((
                    Some("Found in other apps"),
                    places
                        .other_apps
                        .iter()
                        .map(|folder| {
                            let parent = self
                                .parent_folder(&folder.path, &d.platform)
                                .map(|parent| last_segment(&parent));
                            place(
                                ICON_GIT_BRANCH,
                                folder.name.clone(),
                                folder.path.clone(),
                                parent,
                            )
                        })
                        .collect(),
                ));
            }
            if !places.recent.is_empty() {
                sections.push((
                    Some("Recent"),
                    places
                        .recent
                        .iter()
                        .map(|folder| {
                            place(ICON_HISTORY, folder.name.clone(), folder.path.clone(), None)
                        })
                        .collect(),
                ));
            }
        }
        let computers = self
            .machines
            .iter()
            .map(|machine| {
                let id = machine.machine_id.clone();
                SidebarItem {
                    icon: if machine.machine_id == LOCAL_MACHINE_ID {
                        ICON_DEVICE_DESKTOP
                    } else {
                        ICON_SERVER
                    },
                    label: machine.label.clone(),
                    note: Some(platform_word(machine.platform.as_deref()).to_string())
                        .filter(|word| !word.is_empty()),
                    active: self.machine_id.as_deref() == Some(machine.machine_id.as_str()),
                    on_click: action(move |this, window, cx| {
                        this.choose_machine(id.clone());
                        this.settle(window, cx);
                    }),
                    unpin: None,
                }
            })
            .collect();
        sections.push((Some("Computers"), computers));
        sections
    }

    fn render_sidebar(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = skin.muted_fill_at(0.6);
        let active_fill = skin.muted_fill;
        let mut children: Vec<AnyElement> = Vec::new();
        let mut index = 0usize;
        for (title, items) in self.sidebar_items(d) {
            if let Some(title) = title {
                children.push(
                    div()
                        .flex_shrink_0()
                        .px(px(8.0))
                        .pt(px(12.0))
                        .pb(px(4.0))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(skin.muted()))
                        .child(title)
                        .into_any_element(),
                );
            }
            let busy = self.busy.is_some();
            for item in items {
                index += 1;
                let on_click = item.on_click.clone();
                let unpin = item.unpin.clone().map(|path| {
                    div()
                        .id(("add-project-unpin", index))
                        .flex_shrink_0()
                        .size(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .when(!busy, |this| {
                            this.hover(move |this| this.bg(hsla(hover)))
                                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    cx.stop_propagation();
                                    this.set_pinned(&path, false, cx);
                                }))
                        })
                        .child(modal_icon(ICON_X, 12.0, skin.muted()))
                });
                children.push(
                    h_flex()
                        .id(("add-project-place", index))
                        .flex_shrink_0()
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .text_size(px(13.0))
                        .text_color(hsla(skin.fg()))
                        .when(item.active, |this| this.bg(hsla(active_fill)))
                        .when(!item.active && !busy, |this| {
                            this.hover(move |this| this.bg(hsla(hover)))
                        })
                        .when(!busy, |this| {
                            this.on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_click(cx.listener(
                                move |this, _: &ClickEvent, window, cx| {
                                    on_click(this, window, cx);
                                },
                            ))
                        })
                        .child(div().flex_shrink_0().child(modal_icon(
                            item.icon,
                            15.0,
                            if item.active { skin.fg() } else { skin.muted() },
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(item.label),
                        )
                        .children(item.note.map(|note| {
                            div()
                                .flex_shrink_0()
                                .max_w(px(90.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(11.5))
                                .text_color(hsla(skin.muted()))
                                .child(note)
                        }))
                        .children(unpin)
                        .into_any_element(),
                );
            }
        }
        div()
            .id("add-project-sidebar")
            .flex_shrink_0()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar_scroll)
            .bg(hsla(skin.p.panel))
            .border_r_1()
            .border_color(hsla(skin.border_at(0.7)))
            .px(px(6.0))
            .pt(px(6.0))
            .pb(px(10.0))
            .child(v_flex().children(children))
            .into_any_element()
    }

    // -- columns ----------------------------------------------------------------------------------

    /// One folder row in a column.
    #[allow(clippy::too_many_arguments)]
    fn column_row(
        &self,
        skin: &AddProjectSkin,
        id: (SharedString, usize),
        entry: &AddProjectBrowseEntry,
        state: RowState,
        is_disk: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = skin.muted_fill_at(0.6);
        let fill = match state {
            RowState::Selected => Some(skin.muted_fill),
            RowState::OnPath => Some(skin.muted_fill_at(0.55)),
            RowState::Highlighted => Some(skin.muted_fill_at(0.8)),
            RowState::Plain => None,
        };
        let path = entry.full_path.clone();
        let strong = matches!(state, RowState::Selected | RowState::Highlighted);
        let busy = self.busy.is_some();
        h_flex()
            .id(id)
            .flex_shrink_0()
            .w_full()
            .h(px(ROW_HEIGHT))
            .items_center()
            .gap(px(7.0))
            .pl(px(8.0))
            .pr(px(6.0))
            .rounded(px(6.0))
            .text_size(px(13.0))
            .text_color(hsla(skin.fg()))
            .when_some(fill, |this, fill| this.bg(hsla(fill)))
            .when(fill.is_none() && !busy, |this| {
                this.hover(move |this| this.bg(hsla(hover)))
            })
            .when(!busy, |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_column_entry(&path);
                        this.settle(window, cx);
                    }))
            })
            .child(div().flex_shrink_0().child(modal_icon(
                if is_disk { ICON_DISK } else { ICON_FOLDER },
                15.0,
                if strong { skin.fg() } else { skin.muted() },
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(entry.name.clone()),
            )
            .when(entry.git, |this| {
                this.child(div().flex_shrink_0().child(modal_icon(
                    ICON_GIT_BRANCH,
                    12.0,
                    skin.muted(),
                )))
            })
            .when(entry.project_id.is_some(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(hsla(skin.muted())),
                )
            })
            .into_any_element()
    }

    fn column_shell(
        &self,
        skin: &AddProjectSkin,
        id: usize,
        scroll: Option<&ScrollHandle>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        v_flex()
            .id(("add-project-column", id))
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .when_some(scroll, |this, handle| this.track_scroll(handle))
            .border_r_1()
            .border_color(hsla(skin.border_at(0.7)))
            .p(px(4.0))
            .children(rows)
            .into_any_element()
    }

    fn column_message(&self, skin: &AddProjectSkin, text: impl Into<SharedString>) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(10.0))
            .text_size(px(12.5))
            .line_height(px(18.0))
            .text_color(hsla(skin.muted()))
            .child(text.into())
            .into_any_element()
    }

    /// What the open folder's column says when it has no folders to list.
    fn last_column_message(&self, d: &Derived) -> Option<String> {
        if d.machine_id.is_none() {
            return Some("No machine is available.".to_string());
        }
        if d.unsupported_windows_path {
            return Some("Windows-style paths are only supported on Windows machines.".to_string());
        }
        if d.relative_path_needs_active_project {
            return Some("Relative paths require an active project.".to_string());
        }
        if !d.is_browsing {
            return Some(if d.clone_detected.is_some() {
                "Press Enter to clone this repository.".to_string()
            } else if d.ambiguous.is_some() {
                "Choose above whether this is a folder or a repository.".to_string()
            } else {
                "Type a folder path, or pick a place on the left.".to_string()
            });
        }
        if self.is_browse_pending && self.browse_result.is_none() {
            return Some("Loading…".to_string());
        }
        if d.filtered.filtered.is_empty() && self.new_folder_name.is_none() {
            return Some(if has_trailing_path_separator(&self.query) {
                "No folders here.".to_string()
            } else {
                "No matching folders.".to_string()
            });
        }
        None
    }

    fn render_columns(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let machine_id = d.machine_id.clone().unwrap_or_default();
        let ancestors = self.column_folders(d);
        let mut chain: Vec<String> = ancestors.clone();
        if let Some(selected) = &d.selected_folder {
            chain.push(selected.clone());
        }
        let mut columns: Vec<AnyElement> = Vec::new();
        for (index, folder) in ancestors.iter().enumerate() {
            let next = chain.get(index + 1).cloned();
            let key = listing_key(&machine_id, folder);
            let rows: Vec<AnyElement> = match self.listings.get(&key) {
                Some(listing) => {
                    let is_disks = listing.is_drive_list;
                    let visible: Vec<AddProjectBrowseEntry> = listing
                        .entries
                        .iter()
                        .filter(|entry| !entry.name.starts_with('.'))
                        .cloned()
                        .collect();
                    if visible.is_empty() {
                        vec![self.column_message(skin, "No folders here.")]
                    } else {
                        visible
                            .iter()
                            .enumerate()
                            .map(|(row, entry)| {
                                let on_path =
                                    next.as_deref() == Some(trim_folder(&entry.full_path).as_str());
                                let state = if on_path && index + 1 == ancestors.len() {
                                    RowState::Selected
                                } else if on_path {
                                    RowState::OnPath
                                } else {
                                    RowState::Plain
                                };
                                self.column_row(
                                    skin,
                                    (SharedString::from(format!("col-{index}")), row),
                                    entry,
                                    state,
                                    is_disks,
                                    cx,
                                )
                            })
                            .collect()
                    }
                }
                None if self.failed_columns.contains(&key) => {
                    vec![self.column_message(skin, "Unable to open this folder.")]
                }
                None => vec![self.column_message(skin, "Loading…")],
            };
            columns.push(self.column_shell(skin, index, None, rows));
        }
        // The open folder's own contents, with the new folder being named at the top.
        let mut rows: Vec<AnyElement> = Vec::new();
        if self.new_folder_name.is_some() {
            let focused = gpui::Focusable::focus_handle(self.folder_name_input.read(cx), cx)
                .is_focused(window);
            let creating = self.busy == Some(Busy::CreateFolder);
            rows.push(
                h_flex()
                    .flex_shrink_0()
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .items_center()
                    .gap(px(7.0))
                    .pl(px(8.0))
                    .pr(px(4.0))
                    .rounded(px(6.0))
                    .bg(hsla(skin.muted_fill))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(modal_icon(ICON_FOLDER, 15.0, skin.fg())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(22.0))
                            .px(px(5.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(hsla(if focused {
                                skin.bar_focus_border
                            } else {
                                skin.bar_border
                            }))
                            .bg(hsla(skin.bar_background))
                            .child(
                                Input::new(&self.folder_name_input)
                                    .with_size(ComponentSize::XSmall)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .disabled(creating)
                                    .w_full()
                                    .px(px(0.0))
                                    .py(px(0.0))
                                    .text_size(px(13.0))
                                    .text_color(hsla(skin.fg())),
                            ),
                    )
                    .into_any_element(),
            );
        }
        let is_disks = d.is_drive_list;
        for (row, entry) in d.filtered.filtered.iter().enumerate() {
            let highlighted = self.highlight.as_deref() == Some(entry.full_path.as_str())
                || (!has_trailing_path_separator(&self.query)
                    && d.filtered.exact.as_ref().map(|exact| &exact.full_path)
                        == Some(&entry.full_path));
            let created =
                self.created_folder.as_deref() == Some(trim_folder(&entry.full_path).as_str());
            rows.push(self.column_row(
                skin,
                (SharedString::from("col-open"), row),
                entry,
                if highlighted || created {
                    RowState::Highlighted
                } else {
                    RowState::Plain
                },
                is_disks,
                cx,
            ));
        }
        if let Some(message) = self.last_column_message(d).filter(|_| rows.is_empty()) {
            rows.push(self.column_message(skin, message));
        }
        columns.push(self.column_shell(skin, 99, Some(&self.open_column_scroll), rows));
        // Three columns always share the width; they fill from the left.
        while columns.len() < 3 {
            columns.insert(
                columns.len(),
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .border_r_1()
                    .border_color(hsla(skin.border_at(0.7)))
                    .into_any_element(),
            );
        }
        h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(columns)
            .into_any_element()
    }

    // -- preview ----------------------------------------------------------------------------------

    /// The selected folder's entry in its parent's listing, for what it is.
    fn selected_entry(&self, d: &Derived) -> Option<AddProjectBrowseEntry> {
        let machine_id = d.machine_id.as_deref()?;
        let selected = d.selected_folder.clone()?;
        let parent = self.parent_folder(&selected, &d.platform)?;
        self.listings
            .get(&listing_key(machine_id, &parent))?
            .entries
            .iter()
            .find(|entry| trim_folder(&entry.full_path) == selected)
            .cloned()
    }

    fn render_preview(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let typed_entry = (!has_trailing_path_separator(&self.query))
            .then(|| d.filtered.exact.clone())
            .flatten();
        let inspection = d.path_inspection.clone().unwrap_or_default_inspection();
        let entry = typed_entry.clone().or_else(|| self.selected_entry(d));
        let (name, path) = if d.will_create_project_path {
            let path = d.resolved_add_project_path.clone();
            (last_segment(&path), path)
        } else if let Some(entry) = &typed_entry {
            (entry.name.clone(), entry.full_path.clone())
        } else if d.is_drive_list {
            (
                d.machine
                    .as_ref()
                    .map(|machine| machine.label.clone())
                    .unwrap_or_default(),
                String::new(),
            )
        } else if let Some(selected) = &d.selected_folder {
            (last_segment(selected), selected.clone())
        } else {
            (String::new(), String::new())
        };
        let git = entry.as_ref().is_some_and(|entry| entry.git) || inspection.branch.is_some();
        let branch = inspection
            .branch
            .clone()
            .or_else(|| entry.as_ref().and_then(|entry| entry.branch.clone()));
        let language = inspection
            .language
            .clone()
            .or_else(|| entry.as_ref().and_then(|entry| entry.language.clone()));
        let in_ghostex = inspection.project_id.is_some()
            || entry
                .as_ref()
                .is_some_and(|entry| entry.project_id.is_some());
        let mut facts: Vec<(&'static str, String)> = Vec::new();
        if d.will_create_project_path {
            facts.push(("Kind", "New folder".to_string()));
        } else if !d.is_drive_list && !path.is_empty() {
            let kind = if git { "Git repository" } else { "Folder" };
            facts.push((
                "Kind",
                match &language {
                    Some(language) => format!("{kind} · {language}"),
                    None => kind.to_string(),
                },
            ));
            if let Some(branch) = &branch {
                facts.push(("Branch", branch.clone()));
            }
            if let Some(root) = &inspection.repo_root {
                facts.push(("Repository", last_segment(root)));
            }
            facts.push((
                "In Ghostex",
                if in_ghostex { "Yes" } else { "No" }.to_string(),
            ));
        }
        let pinned = self.current_places().is_some_and(|places| {
            places
                .pinned
                .iter()
                .any(|pin| trim_folder(&pin.path) == trim_folder(&path))
        });
        let can_pin = !path.is_empty()
            && !d.will_create_project_path
            && !d.is_drive_list
            && self.current_places().is_some();
        let pin_path = path.clone();
        let icon = if d.is_drive_list {
            ICON_DISK
        } else if git {
            ICON_GIT_BRANCH
        } else if d.will_create_project_path {
            ICON_FOLDER_PLUS
        } else {
            ICON_FOLDER
        };
        v_flex()
            .flex_shrink_0()
            .w(px(PREVIEW_WIDTH))
            .h_full()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(16.0))
            .when(!name.is_empty(), |this| {
                this.child(
                    div()
                        .size(px(40.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.0))
                        .bg(hsla(skin.p.raised))
                        .child(modal_icon(icon, 20.0, skin.fg())),
                )
                .child(
                    v_flex()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .line_height(px(20.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(hsla(skin.fg()))
                                .child(name.clone()),
                        )
                        .when(!path.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(hsla(skin.muted()))
                                    .child(self.tildify(&path)),
                            )
                        }),
                )
            })
            .children(facts.into_iter().map(|(label, value)| {
                h_flex()
                    .gap(px(8.0))
                    .text_size(px(12.5))
                    .line_height(px(17.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(78.0))
                            .text_color(hsla(skin.muted()))
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(hsla(skin.fg()))
                            .child(value),
                    )
            }))
            .when(d.will_create_project_path, |this| {
                this.child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(17.0))
                        .text_color(hsla(skin.muted()))
                        .child("Create & Add makes this folder and adds it."),
                )
            })
            .when(can_pin, |this| {
                this.child(h_flex().child(self.filled_button(
                    skin,
                    "add-project-pin",
                    if pinned {
                        "Unpin from sidebar"
                    } else {
                        "Pin to sidebar"
                    },
                    Some(ICON_PIN),
                    true,
                    false,
                    action(move |this, _window, cx| {
                        this.set_pinned(&pin_path, !pinned, cx);
                    }),
                    cx,
                )))
            })
            .into_any_element()
    }

    // -- suggested ------------------------------------------------------------------------------

    fn render_suggested(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = d
            .machine
            .as_ref()
            .map(|machine| machine.label.clone())
            .unwrap_or_default();
        let suggestions = self
            .current_places()
            .map(|places| places.suggested.clone())
            .unwrap_or_default();
        let busy = self.busy.is_some();
        let cards: Vec<AnyElement> = suggestions
            .iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let from = match suggestion.source.as_str() {
                    "otherApps" => "Found in other apps".to_string(),
                    _ => suggestion
                        .from
                        .as_ref()
                        .map(|folder| format!("In {folder}"))
                        .unwrap_or_default(),
                };
                let details = [
                    Some(from).filter(|text| !text.is_empty()),
                    suggestion.branch.clone(),
                    suggestion.language.clone(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                let add_path = suggestion.path.clone();
                let show_path = suggestion.path.clone();
                v_flex()
                    .flex_shrink_0()
                    .w(px(220.0))
                    .gap(px(5.0))
                    .p(px(12.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(hsla(skin.border_at(0.7)))
                    .bg(hsla(skin.p.panel))
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(px(8.0))
                            .items_center()
                            .child(modal_icon(ICON_GIT_BRANCH, 15.0, skin.fg()))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(skin.fg()))
                                    .child(suggestion.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.0))
                            .text_color(hsla(skin.muted()))
                            .child(self.tildify(&suggestion.path)),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.0))
                            .text_color(hsla(skin.muted()))
                            .child(details),
                    )
                    .child(
                        h_flex()
                            .mt(px(4.0))
                            .gap(px(6.0))
                            .child(self.filled_button(
                                skin,
                                ("add-project-suggested-add", index),
                                "Add project",
                                None,
                                true,
                                busy,
                                action(move |this, window, cx| {
                                    this.submit_add_path(&add_path, false, cx);
                                    this.settle(window, cx);
                                }),
                                cx,
                            ))
                            .child(self.filled_button(
                                skin,
                                ("add-project-suggested-show", index),
                                "Show",
                                None,
                                true,
                                false,
                                action(move |this, window, cx| {
                                    let platform = this.derive().platform;
                                    let parent = this.parent_folder(&show_path, &platform);
                                    this.go_to_folder(&show_path, parent);
                                    this.settle(window, cx);
                                }),
                                cx,
                            )),
                    )
                    .into_any_element()
            })
            .collect();
        let empty = cards.is_empty();
        div()
            .id("add-project-suggested")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .p(px(14.0))
            .child(
                v_flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(hsla(skin.muted()))
                            .child(if self.current_places().is_none() {
                                "Looking for repositories…".to_string()
                            } else if empty {
                                format!("Every repository found on {label} is already in Ghostex.")
                            } else {
                                format!("Git repositories on {label} that are not in Ghostex yet")
                            }),
                    )
                    .child(h_flex().flex_wrap().gap(px(10.0)).children(cards)),
            )
            .into_any_element()
    }

    // -- notices ----------------------------------------------------------------------------------

    fn render_notices(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if let Some(error) = self.error.clone().or_else(|| d.detected_error.clone()) {
            out.push(
                error_region(skin, &error)
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }
        if self.sheet != Some(Sheet::Clone)
            && let Some(clone) = &d.clone_detected
        {
            let text = clone.query.clone();
            out.push(self.notice(
                skin,
                ICON_GIT_BRANCH,
                "This is a repository. Clone it into a folder you pick on the left.",
                vec![self.filled_button(
                    skin,
                    "add-project-notice-clone",
                    "Clone a repo",
                    None,
                    true,
                    false,
                    action(move |this, window, cx| {
                        this.open_clone_sheet(Some(text.clone()), window, cx)
                    }),
                    cx,
                )],
            ));
        }
        if let Some((query, _)) = &d.ambiguous {
            let folder_query = query.clone();
            let repo_query = query.clone();
            out.push(self.notice(
                skin,
                ICON_ALERT,
                format!("Choose how to open {query}"),
                vec![
                    self.filled_button(
                        skin,
                        "add-project-ambiguous-folder",
                        "Local folder",
                        Some(ICON_FOLDER),
                        true,
                        false,
                        action(move |this, window, cx| {
                            let base = this
                                .derive()
                                .selected_folder
                                .unwrap_or_else(|| this.start_query());
                            let folder = format!(
                                "{}{}",
                                ensure_browse_directory_path(&base),
                                folder_query.trim_end_matches('/')
                            );
                            this.go_to_folder(&folder, None);
                            this.settle(window, cx);
                        }),
                        cx,
                    ),
                    self.filled_button(
                        skin,
                        "add-project-ambiguous-github",
                        "GitHub repository",
                        Some(source_icon(AddProjectSourceId::Github)),
                        true,
                        false,
                        action(move |this, window, cx| {
                            this.open_clone_sheet(Some(repo_query.clone()), window, cx);
                            this.choose_clone_source(AddProjectSourceId::Github);
                            this.settle(window, cx);
                        }),
                        cx,
                    ),
                ],
            ));
        }
        if d.will_create_project_path
            && self.sheet.is_none()
            && !d.resolved_add_project_path.is_empty()
        {
            out.push(self.notice(
                skin,
                ICON_FOLDER_PLUS,
                format!(
                    "There is no {} here yet. Create & Add makes the folder and adds it.",
                    last_segment(&d.resolved_add_project_path)
                ),
                Vec::new(),
            ));
        }
        if let Some(inspection) = &d.path_inspection {
            if inspection.kind == Some(AddProjectInspectionKind::File)
                && d.suggested_project_path.is_none()
            {
                out.push(self.notice(
                    skin,
                    ICON_ALERT,
                    "This is a file outside a Git repository. Choose a project folder.",
                    Vec::new(),
                ));
            }
            if let Some(root) = inspection
                .repo_root
                .clone()
                .filter(|_| inspection.project_id.is_none())
            {
                let root_name = last_segment(&root);
                let folder_name = last_segment(
                    &inspection
                        .project_path
                        .clone()
                        .unwrap_or_else(|| d.resolved_add_project_path.clone()),
                );
                let busy = self.busy.is_some();
                out.push(self.notice(
                    skin,
                    ICON_GIT_BRANCH,
                    format!("{folder_name} is inside the Git repository {root_name}."),
                    vec![self.filled_button(
                        skin,
                        "add-project-add-root",
                        format!("Add {root_name} instead"),
                        None,
                        true,
                        busy,
                        action(move |this, window, cx| {
                            this.submit_add_path(&root, false, cx);
                            this.settle(window, cx);
                        }),
                        cx,
                    )],
                ));
            }
        }
        if let Some(text) = folder_access_notice(d, self.current_home().as_deref()) {
            out.push(self.notice(skin, ICON_ALERT, text, Vec::new()));
        }
        if self.is_slow && self.busy.is_some() && self.sheet != Some(Sheet::Clone) {
            out.push(self.notice(
                skin,
                ICON_ALERT,
                "Still working. The machine may be reconnecting.",
                Vec::new(),
            ));
        }
        out
    }

    // -- footer -----------------------------------------------------------------------------------

    fn render_footer(
        &mut self,
        skin: &AddProjectSkin,
        d: &Derived,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = d
            .machine
            .as_ref()
            .map(|machine| machine.label.clone())
            .unwrap_or_default();
        let local = d.machine_id.as_deref() == Some(LOCAL_MACHINE_ID);
        let hover = skin.muted_fill_at(0.6);
        let submit_label = match self.busy {
            Some(Busy::Add) if self.sheet.is_none() => "Adding",
            _ => d.submit_action_label,
        };
        // A side panel has its own Cancel and its own action.
        let panel_open = self.sheet.is_some();
        let busy = self.busy.is_some();
        let can_submit = d.can_submit_browse_path && !busy && !self.suggested_mode;
        let button_bounds = self.machine_button_bounds.clone();
        h_flex()
            .flex_shrink_0()
            .h(px(54.0))
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .border_t_1()
            .border_color(hsla(skin.border_at(0.7)))
            .child(
                h_flex()
                    .id("add-project-computer")
                    .relative()
                    .flex_shrink_0()
                    .h(px(30.0))
                    .items_center()
                    .gap(px(7.0))
                    .px(px(9.0))
                    .rounded(px(7.0))
                    .text_size(px(13.0))
                    .text_color(hsla(skin.fg()))
                    .when(busy, |this| this.opacity(0.5))
                    .when(!busy, |this| {
                        this.hover(move |this| this.bg(hsla(hover)))
                            .when(self.machine_menu_open, |this| {
                                this.bg(hsla(skin.muted_fill))
                            })
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.machine_menu_open = !this.machine_menu_open;
                                cx.notify();
                            }))
                    })
                    .child(
                        canvas(
                            move |bounds, _window, _cx| {
                                button_bounds.set(Some(bounds));
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(modal_icon(
                        if local {
                            ICON_DEVICE_DESKTOP
                        } else {
                            ICON_SERVER
                        },
                        15.0,
                        skin.muted(),
                    ))
                    .child(label)
                    .child(modal_icon(ICON_CHEVRON_DOWN, 12.0, skin.muted())),
            )
            .child(div().flex_1())
            .when(!panel_open, |this| {
                this.child(self.filled_button(
                    skin,
                    "add-project-cancel",
                    "Cancel",
                    None,
                    false,
                    false,
                    action(|this, window, cx| this.close(window, cx)),
                    cx,
                ))
            })
            .when(!panel_open, |this| {
                this.child(self.filled_button(
                    skin,
                    "add-project-submit",
                    submit_label,
                    (submit_label == "Create & Add").then_some(ICON_FOLDER_CHECK),
                    false,
                    !can_submit,
                    action(|this, window, cx| this.submit_resolved_path(window, cx)),
                    cx,
                ))
            })
            .into_any_element()
    }

    fn render_machine_menu(&mut self, skin: &AddProjectSkin, cx: &mut Context<Self>) -> AnyElement {
        let hover = skin.muted_fill_at(0.6);
        let busy = self.busy.is_some();
        let button_bounds = self.machine_button_bounds.clone();
        let rows: Vec<AnyElement> = self
            .machines
            .iter()
            .enumerate()
            .map(|(index, machine)| {
                let id = machine.machine_id.clone();
                let current = self.machine_id.as_deref() == Some(machine.machine_id.as_str());
                h_flex()
                    .id(("add-project-machine", index))
                    .h(px(34.0))
                    .items_center()
                    .gap(px(9.0))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .text_color(hsla(skin.fg()))
                    .when(!busy, |this| {
                        this.hover(move |this| this.bg(hsla(hover)))
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.choose_machine(id.clone());
                                this.settle(window, cx);
                            }))
                    })
                    .child(modal_icon(
                        if machine.machine_id == LOCAL_MACHINE_ID {
                            ICON_DEVICE_DESKTOP
                        } else {
                            ICON_SERVER
                        },
                        15.0,
                        skin.muted(),
                    ))
                    .child(div().flex_1().min_w_0().child(machine.label.clone()))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(skin.muted()))
                            .child(platform_word(machine.platform.as_deref())),
                    )
                    .when(current, |this| {
                        this.child(modal_icon(ICON_CHECK, 14.0, skin.fg()))
                    })
                    .into_any_element()
            })
            .collect();
        v_flex()
            .id("add-project-machine-menu")
            .occlude()
            .absolute()
            .left(px(12.0))
            .bottom(px(50.0))
            .w(px(260.0))
            .p(px(4.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(skin.p.menu_border))
            .bg(hsla(if skin.p.glass {
                skin.p.lifted_menu_fill()
            } else {
                skin.p.menu_background
            }))
            .shadow_lg()
            .on_mouse_down_out(
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    if button_bounds
                        .get()
                        .is_some_and(|bounds| bounds.contains(&event.position))
                    {
                        return;
                    }
                    this.machine_menu_open = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(6.0))
                    .pb(px(4.0))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(skin.muted()))
                    .child("Add the project on"),
            )
            .children(rows)
            .into_any_element()
    }
}

/// How a column row is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowState {
    /// The selected folder, in its parent's column.
    Selected,
    /// A folder on the way to the selected one.
    OnPath,
    /// The keyboard's (or the typed name's) row in the open folder.
    Highlighted,
    Plain,
}

trait InspectionDefault {
    fn unwrap_or_default_inspection(self) -> AddProjectPathInspection;
}

impl InspectionDefault for Option<AddProjectPathInspection> {
    fn unwrap_or_default_inspection(self) -> AddProjectPathInspection {
        self.unwrap_or(AddProjectPathInspection {
            kind: None,
            project_path: None,
            project_id: None,
            git_root: None,
            repo_root: None,
            branch: None,
            language: None,
        })
    }
}

impl Render for GpuiAddProjectModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let skin = AddProjectSkin::resolve(self.palette);
        let d = self.derive();
        let toolbar = self.render_toolbar(&skin, &d, window, cx);
        let notices = self.render_notices(&skin, &d, cx);
        let sidebar = self.render_sidebar(&skin, &d, cx);
        let folders = if self.suggested_mode {
            self.render_suggested(&skin, &d, cx)
        } else {
            self.render_columns(&skin, &d, window, cx)
        };
        let side_panel = match self.sheet {
            Some(_) => Some(self.render_sheet(&skin, &d, window, cx)),
            None if !self.suggested_mode => Some(self.render_preview(&skin, &d, cx)),
            None => None,
        };
        let footer = self.render_footer(&skin, &d, cx);
        let menu = self
            .machine_menu_open
            .then(|| self.render_machine_menu(&skin, cx));
        let remote = d
            .machine
            .as_ref()
            .filter(|machine| machine.machine_id != LOCAL_MACHINE_ID)
            .map(|machine| format!("on {}", machine.label));
        let focus_handle = self.focus_handle.clone();
        div()
            .id("add-project-modal")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(hsla(skin.p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(skin.fg()))
            .track_focus(&focus_handle)
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_tab))
            .capture_key_down(cx.listener(Self::on_key_down))
            .child(
                v_flex()
                    .size_full()
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .h(px(40.0))
                            .items_center()
                            .gap(px(8.0))
                            .px(px(16.0))
                            .border_b_1()
                            .border_color(hsla(skin.border_at(0.7)))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(skin.fg()))
                                    .child("Add Project"),
                            )
                            .children(
                                remote.map(|remote| {
                                    div().text_color(hsla(skin.muted())).child(remote)
                                }),
                            ),
                    )
                    .child(toolbar)
                    .when(!notices.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .flex_shrink_0()
                                .gap(px(6.0))
                                .px(px(12.0))
                                .pt(px(8.0))
                                .pb(px(8.0))
                                .border_b_1()
                                .border_color(hsla(skin.border_at(0.7)))
                                .children(notices),
                        )
                    })
                    .child(
                        h_flex()
                            .flex_1()
                            .min_h_0()
                            .child(sidebar)
                            .child(folders)
                            .children(side_panel.map(|panel| {
                                div()
                                    .flex_shrink_0()
                                    .h_full()
                                    .border_l_1()
                                    .border_color(hsla(skin.border_at(0.7)))
                                    .child(panel)
                            })),
                    )
                    .child(footer),
            )
            .children(menu)
    }
}

/// The line that says, before macOS asks, who will ask: browsing Documents, Desktop or Downloads
/// on this Mac makes macOS ask to let "Ghostex Server" (Ghostex's background service, which reads
/// and watches the project) open that folder, a name the user has never seen.
///
/// CDXC:Onboarding 2026-11-11 DECISION: User, on the first-run page (step 12), picked "Local folder first, and explain the folder prompt"; the picker opens on the folders (CDXC:AddProject 2026-11-21) and keeps this line.
fn folder_access_notice(d: &Derived, home: Option<&str>) -> Option<&'static str> {
    if !cfg!(target_os = "macos")
        || !d.is_browsing
        || d.machine_id.as_deref() != Some(LOCAL_MACHINE_ID)
    {
        return None;
    }
    let env_home = std::env::var("HOME").ok();
    let home = home.map(str::to_string).or(env_home)?;
    let folder = d
        .selected_folder
        .clone()
        .unwrap_or_else(|| expand_home(d.browse_directory_path.trim(), Some(&home)));
    ["Documents", "Desktop", "Downloads"]
        .iter()
        .any(|name| {
            let protected = format!("{}/{name}", home.trim_end_matches('/'));
            // macOS folders are case-insensitive, so `~/documents` is the same folder.
            folder_contains(&protected.to_lowercase(), &folder.to_lowercase())
        })
        .then_some("macOS will ask to let \"Ghostex Server\", Ghostex\u{2019}s own service, read this folder: allow it.")
}
