//! The New Thread picker's top row and its project list: which project the thread starts in, and
//! the list the row (or the New Thread hotkey pressed again) opens. The machine is picked on the
//! Run on row (new_thread_picker_run_on.rs). A child of `new_thread_picker.rs` (included with
//! `#[path]`, so the preview binary builds it too), which is why it reads the picker's private
//! state directly.
//!
//! CDXC:AgentLauncher 2026-11-03 DECISION:
//! User (Q1163 "we need A and also need C", Q1167, Q1183 "cmd-n-again"): the picker names the project it starts in on a row above the search field, and the project is picked there: clicking the row, or pressing the New Thread hotkey again while the picker is open, turns the list into the projects (Recent first, then the rest A to Z, a project with no open session saying "No sessions"), the search field searching them; Enter or a click picks one and goes back to the agents, and the hotkey again or Escape goes back without changing it. A second press of the hotkey no longer closes the picker. The project's machine is chosen on the Run on row, starting on the last machine used for it. The rows come from gx-core (`unified_project_choices`), so the tray and the phone list the same projects.
//! SEE-ALSO: packages/gx-core/src/unified_sidebar/choices.rs (the rows, the start, the machine), apps/desktop/src/app/gx_store/unified_new_session.rs (what the host passes in and does after a launch), apps/mobile/app/src/chat/native/composer/ProjectPickerSheet.tsx (the phone's list).
use super::*;

/// The top row: an 8px gap above it, then 34px.
pub(crate) const NEW_THREAD_PICKER_PLACE_HEIGHT: f32 = 40.0;
/// A section heading in the project list (Recent, All projects).
const SECTION_HEIGHT: f32 = 24.0;
/// The picker without its agent rows, the divider and the three bottom rows: the insets, the search
/// field and the key hints, which is what the project list sits under.
const PROJECT_LIST_CHROME_HEIGHT: f32 = NEW_THREAD_PICKER_CHROME_HEIGHT - 9.0 - 3.0 * 36.0;

const PLACE_LABEL: &str = "In";
const PLACEHOLDER_PROJECTS: &str = "Search projects...";
const SECTION_RECENT: &str = "Recent";
const SECTION_ALL: &str = "All projects";
const NO_SESSIONS: &str = "No sessions";
const EMPTY_PROJECTS: &str = "No projects match.";
const HINT_CHOOSE: &str = "Choose";
const ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";
const ICON_CHEVRON_UP: &str = "titlebar/chevron-up.svg";

/// One machine that has the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerMachine {
    /// `local` for this computer.
    pub(crate) machine_id: String,
    pub(crate) label: String,
    /// A thread can start there now; one that can't keeps its chip, dimmed.
    pub(crate) connected: bool,
    /// The project on that machine a launch there goes to.
    pub(crate) project_id: String,
}

/// One row of the project list: a repository, on every machine that has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerProject {
    pub(crate) repo_key: String,
    pub(crate) label: String,
    pub(crate) has_sessions: bool,
    pub(crate) recent: bool,
    pub(crate) machines: Vec<NewThreadPickerMachine>,
    /// The machine chip a pick of this project starts on.
    pub(crate) start_machine: usize,
}

/// The projects and where the picker starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NewThreadPickerPlace {
    pub(crate) projects: Vec<NewThreadPickerProject>,
    /// The project the picker starts on.
    pub(crate) project: usize,
    /// Its machine chip.
    pub(crate) machine: usize,
    /// The New Thread hotkey as Settings › Hotkeys spells it, drawn on the row.
    pub(crate) hotkey: Option<String>,
}

/// Where a launch goes: a project of a repository on one machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerTarget {
    pub(crate) repo_key: String,
    pub(crate) machine_id: String,
    pub(crate) project_id: String,
}

impl NewThreadPickerProject {
    pub(super) fn run_on_machines(&self) -> Vec<RunOnMachine> {
        self.machines
            .iter()
            .map(|machine| RunOnMachine {
                label: machine.label.clone(),
                connected: machine.connected,
                local: machine.machine_id == "local",
            })
            .collect()
    }
}

/// The letter tile of a project, as the sidebar draws one for a project with no icon.
fn initial(label: &str) -> String {
    label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "·".to_string())
}

impl GpuiNewThreadPickerWindow {
    /// The project the picker starts the thread in.
    pub(super) fn place_project(&self) -> Option<&NewThreadPickerProject> {
        let place = self.place.as_ref()?;
        place.projects.get(place.project)
    }

    /// Where a launch goes: the project, on the Run on row's machine (this computer for a box).
    pub(super) fn launch_target(&self) -> Option<NewThreadPickerTarget> {
        let project = self.place_project()?;
        let machine = project.machines.get(self.run_on.machine_index())?;
        Some(NewThreadPickerTarget {
            repo_key: project.repo_key.clone(),
            machine_id: machine.machine_id.clone(),
            project_id: machine.project_id.clone(),
        })
    }

    /// The Run on row follows the project: its machines, starting on `machine`.
    pub(super) fn apply_place_machines(&mut self, machine: usize) {
        let machines = self
            .place_project()
            .map(NewThreadPickerProject::run_on_machines)
            .unwrap_or_default();
        self.run_on.set_machines(machines, machine);
    }

    /// The project list's rows: every project whose name or machine holds what was typed.
    pub(super) fn project_rows(&self) -> Vec<PickerRow> {
        let query = self.normalized_query();
        let Some(place) = self.place.as_ref() else {
            return Vec::new();
        };
        place
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| {
                matches_query(&project.label, &query)
                    || project
                        .machines
                        .iter()
                        .any(|machine| matches_query(&machine.label, &query) && !query.is_empty())
            })
            .map(|(index, _)| PickerRow::Project(index))
            .collect()
    }

    /// The Recent and All projects headings, drawn only while nothing is typed and both have rows.
    fn project_sections(&self, rows: &[PickerRow]) -> bool {
        let Some(place) = self.place.as_ref() else {
            return false;
        };
        let recent = |row: &PickerRow| match row {
            PickerRow::Project(index) => place.projects.get(*index).is_some_and(|p| p.recent),
            _ => false,
        };
        self.query.trim().is_empty() && rows.iter().any(recent) && !rows.iter().all(recent)
    }

    /// The child index of a project row inside the scroll container, past the headings above it.
    pub(super) fn project_scroll_child_index(&self, rows: &[PickerRow], selected: usize) -> usize {
        if !self.project_sections(rows) {
            return selected;
        }
        let recent = rows
            .iter()
            .filter(|row| match row {
                PickerRow::Project(index) => self
                    .place
                    .as_ref()
                    .and_then(|place| place.projects.get(*index))
                    .is_some_and(|project| project.recent),
                _ => false,
            })
            .count();
        selected + if selected < recent { 1 } else { 2 }
    }

    /// The frame's height while the project list is open: its rows up to the visible maximum and
    /// its headings, under the top row, the search field and the key hints.
    pub(super) fn project_list_height(&self) -> f32 {
        let rows = self.project_rows();
        let headings = if self.project_sections(&rows) {
            2.0
        } else {
            0.0
        };
        PROJECT_LIST_CHROME_HEIGHT
            + NEW_THREAD_PICKER_PLACE_HEIGHT
            + rows.len().clamp(1, NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32
                * NEW_THREAD_PICKER_ROW_HEIGHT
            + headings * SECTION_HEIGHT
    }

    /// The top row or the New Thread hotkey: opens the project list, or closes it. `false` when
    /// the picker has no projects to list, so the hotkey closes the picker as it used to.
    pub(crate) fn toggle_project_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.place.is_none() {
            return false;
        }
        if self.choosing_project {
            self.leave_project_list(window, cx);
        } else {
            self.enter_project_list(window, cx);
        }
        true
    }

    fn enter_project_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope.is_some() {
            self.leave_scope(window, cx);
        }
        self.choosing_project = true;
        self.clear_query(window, cx);
        self.set_placeholder(PLACEHOLDER_PROJECTS, window, cx);
        let current = self.place.as_ref().map(|place| place.project);
        self.selected = self
            .rows()
            .iter()
            .position(|row| Some(*row) == current.map(PickerRow::Project))
            .unwrap_or(0);
        self.scroll.scroll_to_item(self.selected);
        self.fit_window_height(window);
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn leave_project_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choosing_project = false;
        self.clear_query(window, cx);
        self.set_placeholder(PLACEHOLDER_AGENTS, window, cx);
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        self.fit_window_height(window);
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// A project picked from the list: the thread starts there, on its starting machine.
    pub(super) fn pick_project(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(place) = self.place.as_mut() else {
            return;
        };
        let Some(project) = place.projects.get(index) else {
            return;
        };
        let machine = project.start_machine;
        place.project = index;
        place.machine = machine;
        let was_in_box = self.run_on.in_box();
        self.apply_place_machines(machine);
        self.leave_project_list(window, cx);
        if was_in_box != self.run_on.in_box() {
            self.run_location_changed(window, cx);
        }
        self.reveal_selected_run_location();
    }

    /// `In [P] project ⌘N ⌄`: the project the thread starts in; a click opens the project list.
    pub(super) fn render_place(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.colors;
        let project = self.place_project()?;
        let hotkey = self
            .place
            .as_ref()
            .and_then(|place| place.hotkey.clone())
            .filter(|hotkey| !hotkey.is_empty());
        let open = self.choosing_project;
        Some(
            div()
                .flex_shrink_0()
                .pt(px(6.0))
                .px(px(6.0))
                .child(
                    h_flex()
                        .id("ghostex-gpui-new-thread-picker-place")
                        .role(gpui::Role::Button)
                        .aria_label(format!("Project: {}. Change project", project.label))
                        .h(px(NEW_THREAD_PICKER_PLACE_HEIGHT - 6.0))
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(10.0))
                        .rounded(px(6.0))
                        .border_1()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .when(open, |row| {
                            row.bg(hsla(c.selected_background))
                                .border_color(hsla(c.selected_ring))
                        })
                        .when(!open, |row| {
                            row.bg(hsla(rgba_of(c.foreground, 0.04)))
                                .border_color(hsla(rgba_of(c.foreground, 0.08)))
                                .hover(|row| row.bg(hsla(rgba_of(c.foreground, 0.08))))
                        })
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(hsla(c.muted))
                                .child(PLACE_LABEL),
                        )
                        .child(self.render_project_tile(&project.label))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(hsla(c.foreground))
                                .child(project.label.clone()),
                        )
                        .when_some(hotkey, |row, hotkey| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0))
                                    .text_color(hsla(rgba_of(c.muted, 0.8)))
                                    .child(self.render_kbd_text(hotkey)),
                            )
                        })
                        .child(modal_icon(
                            if open {
                                ICON_CHEVRON_UP
                            } else {
                                ICON_CHEVRON_DOWN
                            },
                            13.0,
                            c.glyph,
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_project_list(window, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    fn render_project_tile(&self, label: &str) -> impl IntoElement {
        let c = self.colors;
        div()
            .flex()
            .flex_shrink_0()
            .size(px(16.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(hsla(rgba_of(c.muted, 0.16)))
            .text_size(px(9.5))
            .line_height(px(12.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(hsla(c.muted))
            .child(initial(label))
    }

    /// A hotkey drawn as one key cap.
    fn render_kbd_text(&self, label: String) -> impl IntoElement {
        let c = &self.colors;
        div()
            .flex()
            .h(px(16.0))
            .min_w(px(16.0))
            .px(px(4.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(hsla(rgba_of(c.foreground, 0.08)))
            .border_1()
            .border_color(hsla(rgba_of(c.foreground, 0.14)))
            .text_size(px(10.0))
            .line_height(px(14.0))
            .child(label)
    }

    /// The key hints while the project list is open.
    pub(super) fn render_project_hints(&self) -> impl IntoElement {
        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(16.0))
            .pt(px(9.0))
            .pb(px(7.0))
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(hsla(rgba_of(self.colors.muted, 0.8)))
            .child(self.render_hint(&["↑", "↓"], HINT_MOVE))
            .child(self.render_hint(&["↵"], HINT_CHOOSE))
            .child(self.render_hint(&["esc"], HINT_BACK))
    }

    fn render_section(&self, label: &'static str) -> AnyElement {
        div()
            .flex_shrink_0()
            .h(px(SECTION_HEIGHT))
            .px(px(10.0))
            .pt(px(8.0))
            .text_size(px(10.5))
            .line_height(px(14.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(hsla(rgba_of(self.colors.muted, 0.85)))
            .child(label.to_uppercase())
            .into_any_element()
    }

    /// The project list: Recent, then All projects, one row per project.
    pub(super) fn render_project_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = self.colors;
        let rows = self.project_rows();
        let sections = self.project_sections(&rows);
        let Some(place) = self.place.as_ref() else {
            return Vec::new();
        };
        let mut children: Vec<AnyElement> = Vec::new();
        let mut in_recent = None;
        for (position, row) in rows.iter().enumerate() {
            let PickerRow::Project(index) = *row else {
                continue;
            };
            let Some(project) = place.projects.get(index) else {
                continue;
            };
            if sections && in_recent != Some(project.recent) {
                in_recent = Some(project.recent);
                children.push(self.render_section(if project.recent {
                    SECTION_RECENT
                } else {
                    SECTION_ALL
                }));
            }
            let selected = position == self.selected;
            children.push(
                self.row_shell(
                    ElementId::Name(format!("new-thread-project-{index}").into()),
                    *row,
                    selected,
                    cx,
                )
                .child(self.render_project_tile(&project.label))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(project.label.clone()),
                )
                .when(!project.has_sessions, |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.5))
                            .text_color(hsla(rgba_of(c.muted, 0.85)))
                            .child(NO_SESSIONS),
                    )
                })
                .into_any_element(),
            );
        }
        if rows.is_empty() {
            children.push(self.render_empty(EMPTY_PROJECTS));
        }
        children
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_shows_the_projects_first_letter() {
        assert_eq!(initial("ghostex"), "G");
        assert_eq!(initial("_tools"), "T");
        assert_eq!(initial(""), "·");
    }

    #[test]
    fn the_project_list_is_the_picker_without_its_agent_rows() {
        assert_eq!(PROJECT_LIST_CHROME_HEIGHT, 84.0);
    }
}
