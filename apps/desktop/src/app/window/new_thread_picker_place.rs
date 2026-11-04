//! The New Thread picker's top row and its project list: which project and place the thread
//! starts in, and the list the row (or the New Thread hotkey pressed again) opens, where each
//! project's machines (and Cloud Box and boxes on this computer) are chips on its row. A child of `new_thread_picker.rs` (included with
//! `#[path]`, so the preview binary builds it too), which is why it reads the picker's private
//! state directly.
//!
//! CDXC:AgentLauncher 2026-11-03 DECISION:
//! User (Q1163 "we need A and also need C", Q1167, Q1183 "cmd-n-again"): the picker names the project it starts in on a row above the search field, and the project is picked there: clicking the row, or pressing the New Thread hotkey again while the picker is open, turns the list into the projects (Recent first, then the rest A to Z, a project with no open session saying "No sessions"), the search field searching them; Enter or a click picks one and goes back to the agents, and the hotkey again or Escape goes back without changing it. A second press of the hotkey no longer closes the picker. The project's machine is chosen on the Run on row, starting on the last machine used for it. The rows come from gx-core (`unified_project_choices`), so the tray and the phone list the same projects.
//!
//! CDXC:AgentLauncher 2026-11-03 DECISION:
//! User: "show the machines inside the project picking area please instead" and "fix pressing down arrow here when i open this not going to the top project in this list". A project row shows its places as small chips at its right (`RunOnState::chips`), the one it starts on (the last used for it) filled, a machine that isn't connected dimmed; Enter or a click on the row takes the project there, Left and Right move the highlighted row's pick, and a click on a chip takes that place. The list opens with no row highlighted, so the first Down highlights its top row (Up its last), and Enter before that keeps the project the picker started on; typing highlights the best match. A row lists its chips only when there is a choice or the place isn't this computer, so a project only here stays a plain row. The top row names the picked place beside the project the same way.
//!
//! CDXC:AgentLauncher 2026-11-04 DECISION:
//! User (Q1224, randomsand: with a project on 3 computers "the computer chips push the project name out of view"; "project name on top, the computers below it"): a project row that lists its computers is two lines, the name (and "No sessions") on top and the chips below it, wrapping when there are many, with the `owner/repo` of a project whose name another project shares in front of them. A project only on this computer stays one line.
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
const HINT_PLACE: &str = "Machine";
/// A project row with its computers (or its `owner/repo`) on a second line is this much taller.
const TALL_ROW_EXTRA: f32 = 22.0;
/// The most projects the agent list's search shows.
const START_ROWS_MAX: usize = 5;

/// The first "Start <agent> in" row of a list.
pub(super) fn first_start_row(rows: &[PickerRow]) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, PickerRow::StartIn(_)))
}
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
    /// What tells it from another project with the same name (`owner/repo`, "no remote").
    pub(crate) detail: Option<String>,
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
        let Some(place) = self.place.as_ref() else {
            return Vec::new();
        };
        (0..place.projects.len())
            .filter(|index| self.project_match(*index).is_some())
            .map(PickerRow::Project)
            .collect()
    }

    /// A project against the search (gx-core `project_query_match`, the tray's and the phone's
    /// rule): `None` when it doesn't match, else the machine a typed word named.
    fn project_match(&self, index: usize) -> Option<Option<usize>> {
        let project = self.place.as_ref()?.projects.get(index)?;
        let machines: Vec<(&str, bool)> = project
            .machines
            .iter()
            .map(|machine| (machine.label.as_str(), machine.connected))
            .collect();
        ghostex_gx_core::unified_sidebar::project_query_match(
            &project.label,
            project.detail.as_deref(),
            &machines,
            &self.query,
        )
    }

    /// The search's own rows in the agent list: up to five projects it found, each started with the
    /// first agent (the last one used) in one Enter.
    ///
    /// CDXC:AgentLauncher 2026-11-04 DECISION:
    /// User (Q1224, randomsand: use the picker's search bar to find the project and the computer, and give "what to run" a default so a search plus Enter starts a session): while something is typed, the agent list ends with "Start <agent> in", the projects the search finds (a computer's name in it picks that computer, `project_query_match`), each on the computer it starts on. The agent is the list's first, the one you used last; Enter on such a row starts it there at once. An agent the search finds still comes first, so typing an agent's name works as before, and the project row at the top still opens the full list.
    pub(super) fn start_rows(&self) -> Vec<PickerRow> {
        if self.query.trim().is_empty() || self.agents.is_empty() || self.scope.is_some() {
            return Vec::new();
        }
        let Some(place) = self.place.as_ref() else {
            return Vec::new();
        };
        (0..place.projects.len())
            .filter(|index| self.project_match(*index).is_some())
            .take(START_ROWS_MAX)
            .map(PickerRow::StartIn)
            .collect()
    }

    /// 1 when `selected` is at or past the "Start <agent> in" heading.
    pub(super) fn start_heading_before(&self, rows: &[PickerRow], selected: usize) -> usize {
        usize::from(first_start_row(rows).is_some_and(|first| selected >= first))
    }

    /// The agent and place a "Start <agent> in" row launches.
    pub(super) fn start_in(&self, index: usize) -> Option<(String, NewThreadPickerTarget)> {
        let agent = self.agents.first()?;
        let project = self.place.as_ref()?.projects.get(index)?;
        let machine = self
            .project_match(index)
            .flatten()
            .unwrap_or(project.start_machine);
        let machine = project.machines.get(machine)?;
        Some((
            agent.agent_id.clone(),
            NewThreadPickerTarget {
                repo_key: project.repo_key.clone(),
                machine_id: machine.machine_id.clone(),
                project_id: machine.project_id.clone(),
            },
        ))
    }

    pub(super) fn render_start_heading(&self) -> AnyElement {
        let name = self
            .agents
            .first()
            .map(|agent| agent.name.clone())
            .unwrap_or_default();
        div()
            .flex_shrink_0()
            .h(px(SECTION_HEIGHT))
            .px(px(10.0))
            .pt(px(8.0))
            .text_size(px(10.5))
            .line_height(px(14.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(hsla(rgba_of(self.colors.muted, 0.85)))
            .child(format!("START {} IN", name.to_uppercase()))
            .into_any_element()
    }

    /// `[G] Ghostex  Windows` with the agent at the right: a project the search found.
    pub(super) fn render_start_row(
        &self,
        index: usize,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        let Some(project) = self
            .place
            .as_ref()
            .and_then(|place| place.projects.get(index))
        else {
            return div().into_any_element();
        };
        let machine = self
            .project_match(index)
            .flatten()
            .unwrap_or(project.start_machine);
        let machine_label = project
            .machines
            .get(machine)
            .filter(|_| {
                project.machines.len() > 1
                    || project
                        .machines
                        .iter()
                        .any(|machine| machine.machine_id != "local")
            })
            .map(|machine| machine.label.clone());
        // Then what tells two same-named projects apart.
        let machine_label = match (machine_label, project.detail.clone()) {
            (Some(machine), Some(detail)) => Some(format!("{machine} · {detail}")),
            (machine, detail) => machine.or(detail),
        };
        let agent = self.agents.first().cloned();
        self.row_shell(
            ElementId::Name(format!("new-thread-start-{index}").into()),
            PickerRow::StartIn(index),
            selected,
            cx,
        )
        .child(self.render_project_tile(&project.label))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(6.0))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(project.label.clone()),
                )
                .when_some(machine_label, |line, label| {
                    line.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.0))
                            .text_color(hsla(c.muted))
                            .child(label),
                    )
                }),
        )
        .when_some(agent, |row, agent| {
            row.child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(5.0))
                    .items_center()
                    .text_size(px(12.0))
                    .text_color(hsla(c.muted))
                    .child(self.render_agent_icon(&agent, 12.0, 0.0))
                    .child(agent.name.clone()),
            )
        })
        .into_any_element()
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
        let tall = rows
            .iter()
            .take(NEW_THREAD_PICKER_MAX_AGENT_ROWS)
            .filter(|row| matches!(row, PickerRow::Project(index) if self.row_is_tall(*index)))
            .count();
        PROJECT_LIST_CHROME_HEIGHT
            + NEW_THREAD_PICKER_PLACE_HEIGHT
            + rows.len().clamp(1, NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32
                * NEW_THREAD_PICKER_ROW_HEIGHT
            + tall as f32 * TALL_ROW_EXTRA
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
        self.project_cursor = false;
        self.row_places.clear();
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        self.fit_window_height(window);
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn leave_project_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choosing_project = false;
        self.project_cursor = false;
        self.clear_query(window, cx);
        self.set_placeholder(PLACEHOLDER_AGENTS, window, cx);
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        self.fit_window_height(window);
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// The first Down (or Up) after the list opens: its top (or last) row, nothing moved past.
    pub(super) fn first_project_step(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        if !self.choosing_project || self.project_cursor {
            return false;
        }
        let rows = self.rows();
        if rows.is_empty() {
            return true;
        }
        self.project_cursor = true;
        self.selected = if delta > 0 { 0 } else { rows.len() - 1 };
        self.scroll
            .scroll_to_item(self.project_scroll_child_index(&rows, self.selected));
        cx.notify();
        true
    }

    /// A project's places as its row offers them: the picked one for the project the picker is on,
    /// else where it starts.
    fn project_places(&self, index: usize) -> Option<RunOnState> {
        let place = self.place.as_ref()?;
        let project = place.projects.get(index)?;
        if index == place.project {
            return Some(self.run_on.clone());
        }
        Some(
            self.run_on
                .for_machines(project.run_on_machines(), project.start_machine),
        )
    }

    /// The place a pick of the row takes: the one Left or Right chose on it, else its start.
    pub(super) fn row_place(&self, index: usize) -> usize {
        self.row_places.get(&index).copied().unwrap_or_else(|| {
            // A computer named in the search; machines lead the chips, so its index is its chip's.
            if let Some(machine) = self.project_match(index).flatten() {
                return machine;
            }
            self.project_places(index)
                .map_or(0, |places| places.selected_index())
        })
    }

    /// Whether a project row is two lines: its chips, or its `detail`, under the name.
    fn row_is_tall(&self, index: usize) -> bool {
        let Some(project) = self
            .place
            .as_ref()
            .and_then(|place| place.projects.get(index))
        else {
            return false;
        };
        let chips = self
            .project_places(index)
            .map(|places| places.chips())
            .unwrap_or_default();
        project.detail.is_some() || Self::row_shows_places(&chips, project)
    }

    /// Whether a row draws its chips: there is a place to choose, or it isn't this computer.
    fn row_shows_places(chips: &[RunOnChip], project: &NewThreadPickerProject) -> bool {
        chips.len() > 1
            || project
                .machines
                .iter()
                .any(|machine| machine.machine_id != "local")
    }

    /// Left or Right on the highlighted project row: its next place that can be picked.
    pub(super) fn step_row_place(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        if !self.choosing_project || !self.project_cursor {
            return false;
        }
        let Some(PickerRow::Project(index)) = self.selected_row() else {
            return false;
        };
        let Some(places) = self.project_places(index) else {
            return false;
        };
        if places.chip_count() < 2 {
            return false;
        }
        let next = places.step(self.row_place(index), forward);
        self.row_places.insert(index, next);
        cx.notify();
        true
    }

    /// A project picked from the list: the thread starts there, in the place `chip` (its row's
    /// chip order).
    pub(super) fn pick_project(
        &mut self,
        index: usize,
        chip: usize,
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
        let same_project = place.project == index;
        place.project = index;
        place.machine = machine;
        let was_in_box = self.run_on.in_box();
        if !same_project {
            self.apply_place_machines(machine);
        }
        self.run_on.select_index(chip);
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
        // The place beside the project, while there is one to choose or it isn't this computer.
        let chips = self.run_on.chips();
        let place_chip = Self::row_shows_places(&chips, project)
            .then(|| chips.get(self.run_on.selected_index()).cloned())
            .flatten();
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
                        .when_some(place_chip, |row, chip| {
                            row.child(self.render_place_chip(&chip, true, None))
                        })
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

    /// A place chip: a machine, Cloud Box or a box, filled when it is the row's pick. `click`
    /// takes the project there.
    fn render_place_chip(
        &self,
        chip: &RunOnChip,
        picked: bool,
        click: Option<(usize, usize, &mut Context<Self>)>,
    ) -> AnyElement {
        let c = self.colors;
        let base = h_flex()
            .flex_shrink_0()
            .h(px(20.0))
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .border_1()
            .text_size(px(11.5))
            .line_height(px(14.0))
            .when(picked, |chip| {
                chip.bg(hsla(rgba_of(c.foreground, 0.10)))
                    .border_color(hsla(rgba_of(c.foreground, 0.22)))
                    .text_color(hsla(c.foreground))
            })
            .when(!picked, |chip| {
                chip.border_color(hsla(rgba_of(c.foreground, 0.08)))
                    .text_color(hsla(c.muted))
            })
            .when(!chip.selectable, |chip| chip.opacity(0.45))
            .child(modal_icon(
                chip.icon,
                11.0,
                if picked { c.foreground } else { c.glyph },
            ))
            .child(
                div()
                    .max_w(px(110.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(chip.label.clone()),
            );
        let Some((project, place, cx)) = click else {
            return base.into_any_element();
        };
        let selectable = chip.selectable;
        let tooltip = chip.tooltip.clone();
        base.id(ElementId::Name(
            format!("new-thread-place-{project}-{place}").into(),
        ))
        .when(selectable && !picked, |chip| {
            chip.hover(|chip| chip.bg(hsla(rgba_of(c.foreground, 0.06))))
        })
        .when_some(tooltip, |chip, tooltip| {
            chip.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            if selectable {
                this.pick_project(project, place, window, cx);
            }
        }))
        .into_any_element()
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
            .when(self.any_row_has_places(), |hints| {
                hints.child(self.render_hint(&["←", "→"], HINT_PLACE))
            })
            .child(self.render_hint(&["↵"], HINT_CHOOSE))
            .child(self.render_hint(&["esc"], HINT_BACK))
    }

    fn any_row_has_places(&self) -> bool {
        self.place.as_ref().is_some_and(|place| {
            (0..place.projects.len()).any(|index| {
                self.project_places(index)
                    .is_some_and(|places| places.chip_count() > 1)
            })
        })
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
            let selected = self.project_cursor && position == self.selected;
            let tall = self.row_is_tall(index);
            let places = self.project_places(index);
            let chips = places.as_ref().map(RunOnState::chips).unwrap_or_default();
            let picked = self.row_place(index);
            let chip_elements: Vec<AnyElement> = if Self::row_shows_places(&chips, project) {
                chips
                    .iter()
                    .enumerate()
                    .map(|(chip_index, chip)| {
                        self.render_place_chip(
                            chip,
                            chip_index == picked,
                            Some((index, chip_index, &mut *cx)),
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            };
            children.push(
                self.row_shell(
                    ElementId::Name(format!("new-thread-project-{index}").into()),
                    *row,
                    selected,
                    cx,
                )
                .child(self.render_project_tile(&project.label))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .py(px(if tall { 6.0 } else { 0.0 }))
                        .gap(px(4.0))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(project.label.clone()),
                                )
                                .when(!project.has_sessions, |line| {
                                    line.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(11.5))
                                            .text_color(hsla(rgba_of(c.muted, 0.85)))
                                            .child(NO_SESSIONS),
                                    )
                                }),
                        )
                        // The computers under the name, so three of them never squeeze it.
                        .when(tall, |column| {
                            column.child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .flex_wrap()
                                    .gap(px(4.0))
                                    .items_center()
                                    .when_some(project.detail.clone(), |line, detail| {
                                        line.child(
                                            div()
                                                .flex_shrink_0()
                                                .mr(px(2.0))
                                                .text_size(px(11.5))
                                                .text_color(hsla(rgba_of(c.muted, 0.85)))
                                                .child(detail),
                                        )
                                    })
                                    .children(chip_elements),
                            )
                        }),
                )
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
    fn a_row_lists_its_places_only_when_there_is_a_choice_or_it_is_elsewhere() {
        let chip = |label: &str| RunOnChip {
            label: label.to_string(),
            icon: "",
            selectable: true,
            tooltip: None,
        };
        let project = |machine_id: &str| NewThreadPickerProject {
            repo_key: "r".to_string(),
            label: "r".to_string(),
            has_sessions: true,
            recent: false,
            machines: vec![NewThreadPickerMachine {
                machine_id: machine_id.to_string(),
                label: machine_id.to_string(),
                connected: true,
                project_id: "p".to_string(),
            }],
            start_machine: 0,
            detail: None,
        };
        let here = project("local");
        assert!(!GpuiNewThreadPickerWindow::row_shows_places(
            &[chip("Local")],
            &here
        ));
        assert!(GpuiNewThreadPickerWindow::row_shows_places(
            &[chip("Local"), chip("Cloud Box")],
            &here
        ));
        assert!(GpuiNewThreadPickerWindow::row_shows_places(
            &[chip("Windows")],
            &project("windows")
        ));
    }

    #[test]
    fn the_project_list_is_the_picker_without_its_agent_rows() {
        assert_eq!(PROJECT_LIST_CHROME_HEIGHT, 84.0);
    }
}
