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
//!
//! CDXC:AgentLauncher 2026-11-15 DECISION:
//! User, of the project list's framed machine pills: "i want to show just icon of each machine for ex, i dont wanna show bunch of pills for diff machines. cloud can have cloud icon too", then picked B on the redesign page (Q3258 "machine columns"), Q3259 "letter" with "but put it in an apple icon bg with different color for each project pls", and Q3260 "faint". Every project row is one 30px line: the project's own icon (else its first letter on an app-icon tile in a colour of its own, kept for that project by hashing its repository key), the name, the `owner/repo` or "no remote" of a name another project shares, and at the end its places as logos without frames, each machine (and Cloud Box, and each box) in a column of its own so the logos line up down the list and a gap means the project isn't there. A machine wears the logo the sidebar gives it (Settings > Remote's computer icon, else its system's), Cloud Box the cloud; the place a pick takes is bright with a dot under it, the others dimmer, a machine that isn't connected faint, skipped by Left and Right and not clickable; under the pointer a logo shows the machine's full name (this computer's host name) or that it isn't connected. The top row names the picked place by its logo alone, and the key hints give way to "←→ Machine" inside the search field. This supersedes the 2026-11-04 two-line rows with chips under the name.
//! SEE-ALSO: packages/gx-core/src/unified_sidebar/choices.rs (the rows, the start, the machine), apps/desktop/src/app/gx_store/unified_new_session.rs (what the host passes in and does after a launch), apps/mobile/app/src/chat/native/composer/ProjectPickerSheet.tsx (the phone's list).
use super::*;
use ghostex_gx_core::unified_sidebar::{
    NewThreadStartRow, UnifiedProjectChoice, new_thread_start_heading, new_thread_start_rows,
};
use std::collections::BTreeMap;

/// The top row: an 8px gap above it, then 34px.
pub(crate) const NEW_THREAD_PICKER_PLACE_HEIGHT: f32 = 40.0;
/// A section heading in the project list (Recent, All projects).
const SECTION_HEIGHT: f32 = 24.0;
/// The key hints row under the search field, which the project list does without.
const HINTS_HEIGHT: f32 = 32.0;
/// The picker without its agent rows, the divider, the three bottom rows and the key hints: the
/// insets and the search field, which is what the project list sits under.
const PROJECT_LIST_CHROME_HEIGHT: f32 =
    NEW_THREAD_PICKER_CHROME_HEIGHT - 9.0 - 3.0 * 36.0 - HINTS_HEIGHT;
/// A project row: one line.
const PROJECT_ROW_HEIGHT: f32 = 30.0;
/// One place's column at a row's end: its logo, and the dot under the picked one.
const PLACE_CELL: f32 = 24.0;
const PLACE_LOGO: f32 = 14.0;

const PLACE_LABEL: &str = "In";
const PLACEHOLDER_PROJECTS: &str = "Search projects...";
const SECTION_RECENT: &str = "Recent";
const SECTION_ALL: &str = "All projects";
const NO_SESSIONS: &str = "No sessions";
const EMPTY_PROJECTS: &str = "No projects match.";
const HINT_PLACE: &str = "Machine";
#[cfg(target_os = "macos")]
const PLACE_KEYS: &str = "←→";
#[cfg(not(target_os = "macos"))]
const PLACE_KEYS: &str = "← →";
const OFFLINE_SUFFIX: &str = "isn't connected";

/// The app-icon tiles a project with no icon of its own wears, as (hue, saturation, lightness):
/// soft tones that read on the dark glass and on the light one, none of them black.
const TILE_TONES: [(f32, f32, f32); 11] = [
    (214.0, 0.55, 0.52),
    (238.0, 0.45, 0.60),
    (268.0, 0.42, 0.58),
    (322.0, 0.45, 0.58),
    (8.0, 0.55, 0.58),
    (26.0, 0.62, 0.54),
    (42.0, 0.62, 0.48),
    (145.0, 0.36, 0.44),
    (172.0, 0.42, 0.40),
    (195.0, 0.52, 0.46),
    (220.0, 0.16, 0.50),
];

/// One machine's logo and full name, as the sidebar draws it (`gx_store_computer_marks`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerMark {
    pub(crate) machine_id: String,
    /// Its logo's asset (`titlebar/brand-apple.svg`, ...).
    pub(crate) asset: &'static str,
    /// Its full name: this computer's host name, another machine's saved name.
    pub(crate) name: String,
}

/// The first "Start <agent> in" row of a list.
pub(super) fn first_start_row(rows: &[PickerRow]) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, PickerRow::StartIn(_)))
}
const ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";
const ICON_CHEVRON_UP: &str = "titlebar/chevron-up.svg";

/// The projects (gx-core's project list rows, each starting on its `selected` machine) and where
/// the picker starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NewThreadPickerPlace {
    pub(crate) projects: Vec<UnifiedProjectChoice>,
    /// The project the picker starts on.
    pub(crate) project: usize,
    /// Its machine chip.
    pub(crate) machine: usize,
    /// The New Thread hotkey as Settings › Hotkeys spells it, drawn on the row.
    pub(crate) hotkey: Option<String>,
    /// Every machine's logo and name, in the order the sidebar lists the machines, which is the
    /// order of their columns.
    pub(crate) marks: Vec<NewThreadPickerMark>,
    /// Each project's own icon as a `data:` URL, by repository key; a project without one wears
    /// its letter tile.
    pub(crate) icons: BTreeMap<String, String>,
}

/// Where a launch goes: a project of a repository on one machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerTarget {
    pub(crate) repo_key: String,
    pub(crate) machine_id: String,
    pub(crate) project_id: String,
}

/// A project's machines as the Run on row lists them, each with its logo and name from `marks`.
pub(super) fn run_on_machines(
    project: &UnifiedProjectChoice,
    marks: &[NewThreadPickerMark],
) -> Vec<RunOnMachine> {
    project
        .machines
        .iter()
        .map(|machine| {
            let mark = marks
                .iter()
                .find(|mark| mark.machine_id == machine.machine_id);
            RunOnMachine {
                machine_id: machine.machine_id.clone(),
                label: machine.label.clone(),
                connected: machine.connected,
                local: machine.machine_id == "local",
                icon: mark.map(|mark| mark.asset),
                name: mark.map(|mark| mark.name.clone()),
            }
        })
        .collect()
}

/// A project's tile colour, the same for it every time: FNV-1a over its repository key picks a tone.
fn tile_tone(key: &str) -> (f32, f32, f32) {
    let hash = key.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    TILE_TONES[hash as usize % TILE_TONES.len()]
}

/// The columns for `rows` (each row's places, how many of them lead as its machines, and whether
/// it shows them): see `place_columns`.
fn place_columns_of(
    rows: &[(Vec<RunOnChip>, usize, bool)],
    marks: &[NewThreadPickerMark],
) -> Vec<String> {
    if !rows.iter().any(|(_, _, shown)| *shown) {
        return Vec::new();
    }
    // (group, order, key): machines 0, Cloud Box 1, boxes 2.
    let mut columns: Vec<(u8, usize, String)> = Vec::new();
    for (chips, machines, _) in rows {
        for (position, chip) in chips.iter().enumerate() {
            if columns.iter().any(|(_, _, key)| *key == chip.key) {
                continue;
            }
            let next = columns.len();
            let column = if position < *machines {
                let order = marks
                    .iter()
                    .position(|mark| mark.machine_id == chip.key)
                    .map_or(marks.len() + next, |order| order + 1);
                (
                    0,
                    if chip.key == "local" { 0 } else { order },
                    chip.key.clone(),
                )
            } else if chip.key == CLOUD_BOX_RUN_LOCATION {
                (1, 0, chip.key.clone())
            } else {
                (2, next, chip.key.clone())
            };
            columns.push(column);
        }
    }
    columns.sort_by_key(|(group, order, _)| (*group, *order));
    columns.into_iter().map(|(_, _, key)| key).collect()
}

/// A logo's tooltip: the place's full name, or that the machine isn't connected.
fn place_tooltip(chip: &RunOnChip) -> String {
    if chip.selectable {
        chip.name.clone()
    } else {
        format!("{} {OFFLINE_SUFFIX}", chip.name)
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
    pub(super) fn place_project(&self) -> Option<&UnifiedProjectChoice> {
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
            .place
            .as_ref()
            .zip(self.place_project())
            .map(|(place, project)| run_on_machines(project, &place.marks))
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

    /// The search's own rows in the agent list: the projects gx-core's `new_thread_start_rows`
    /// finds, each started with the first agent (the last one used) in one Enter. They need an
    /// agent to start and aren't offered inside an agent's account list.
    pub(super) fn start_rows(&self) -> Vec<PickerRow> {
        if self.agents.is_empty() || self.scope.is_some() {
            return Vec::new();
        }
        self.start_row_places()
            .into_iter()
            .map(|row| PickerRow::StartIn(row.choice))
            .collect()
    }

    /// The "Start <agent> in" rows for what is typed (gx-core); `render_list` draws them all from
    /// one search.
    pub(super) fn start_row_places(&self) -> Vec<NewThreadStartRow> {
        self.place
            .as_ref()
            .map(|place| new_thread_start_rows(&place.projects, &self.query))
            .unwrap_or_default()
    }

    /// The gx-core row a `PickerRow::StartIn(index)` launches.
    fn start_row_place(&self, index: usize) -> Option<NewThreadStartRow> {
        self.start_row_places()
            .into_iter()
            .find(|row| row.choice == index)
    }

    /// 1 when `selected` is at or past the "Start <agent> in" heading.
    pub(super) fn start_heading_before(&self, rows: &[PickerRow], selected: usize) -> usize {
        usize::from(first_start_row(rows).is_some_and(|first| selected >= first))
    }

    /// The agent and place a "Start <agent> in" row launches.
    pub(super) fn start_in(&self, index: usize) -> Option<(String, NewThreadPickerTarget)> {
        let agent = self.agents.first()?;
        let project = self.place.as_ref()?.projects.get(index)?;
        let row = self.start_row_place(index)?;
        let machine = project.machines.get(row.machine)?;
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
            .child(new_thread_start_heading(&name).to_uppercase())
            .into_any_element()
    }

    /// `[G] Ghostex  Windows` with the agent at the right: a project the search found.
    /// `start` is the gx-core row for `index` from the list's one search (`start_row_places`).
    pub(super) fn render_start_row(
        &self,
        index: usize,
        start: Option<&NewThreadStartRow>,
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
        let machine_label = start.and_then(|row| row.place.clone());
        let agent = self.agents.first().cloned();
        self.row_shell(
            ElementId::Name(format!("new-thread-start-{index}").into()),
            PickerRow::StartIn(index),
            selected,
            cx,
        )
        .child(self.render_project_tile(project))
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
    /// its headings, under the top row and the search field.
    pub(super) fn project_list_height(&self) -> f32 {
        let rows = self.project_rows();
        let headings = if self.project_sections(&rows) {
            2.0
        } else {
            0.0
        };
        PROJECT_LIST_CHROME_HEIGHT
            + NEW_THREAD_PICKER_PLACE_HEIGHT
            + rows.len().clamp(1, NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32 * PROJECT_ROW_HEIGHT
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
        self.set_placeholder(self.agents_placeholder(), window, cx);
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
                .for_machines(run_on_machines(project, &place.marks), project.selected),
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

    /// The project list's place columns: every machine in the sidebar's order (one no mark names
    /// after them, as first met), then Cloud Box, then the boxes. Read from every project, not just
    /// the ones the search keeps, so the columns stay put while typing. Empty when no row has a
    /// place to choose and every project is only on this computer, which then draws no logos.
    fn place_columns(&self) -> Vec<String> {
        let Some(place) = self.place.as_ref() else {
            return Vec::new();
        };
        let rows: Vec<(Vec<RunOnChip>, usize, bool)> = (0..place.projects.len())
            .filter_map(|index| {
                let project = place.projects.get(index)?;
                let chips = self
                    .project_places(index)
                    .map(|places| places.chips())
                    .unwrap_or_default();
                let shown = Self::row_shows_places(&chips, project);
                Some((chips, project.machines.len().max(1), shown))
            })
            .collect();
        place_columns_of(&rows, &place.marks)
    }

    /// Whether a row draws its chips: there is a place to choose, or it isn't this computer.
    fn row_shows_places(chips: &[RunOnChip], project: &UnifiedProjectChoice) -> bool {
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
        let machine = project.selected;
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

    /// `In [P] project [logo] ⌘N ⌄`: the project the thread starts in and the logo of the place it
    /// starts at; a click opens the project list.
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
                        .child(self.render_project_tile(project))
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
                            let tooltip = place_tooltip(&chip);
                            row.child(
                                div()
                                    .id("ghostex-gpui-new-thread-picker-place-logo")
                                    .flex()
                                    .flex_shrink_0()
                                    .size(px(20.0))
                                    .items_center()
                                    .justify_center()
                                    .child(modal_icon(chip.icon, 13.0, c.selected_label))
                                    .tooltip(move |window, cx| {
                                        gpui_component::tooltip::Tooltip::new(tooltip.clone())
                                            .build(window, cx)
                                    }),
                            )
                        })
                        .when_some(hotkey, |row, hotkey| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .whitespace_nowrap()
                                    .text_size(px(11.5))
                                    .text_color(hsla(rgba_of(c.muted, 0.75)))
                                    .child(hotkey),
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

    /// A project's icon: its own when it has one, else its first letter on an app-icon tile in the
    /// project's own colour.
    fn render_project_tile(&self, project: &UnifiedProjectChoice) -> AnyElement {
        let image = self
            .place
            .as_ref()
            .and_then(|place| place.icons.get(&project.repo_key))
            .and_then(|url| crate::data_url_image::image(url, false));
        if let Some(image) = image {
            return gpui::img(image)
                .flex_shrink_0()
                .size(px(16.0))
                .into_any_element();
        }
        let (h, s, l) = tile_tone(&project.repo_key);
        let tone = |lightness: f32| gpui::Hsla {
            h: h / 360.0,
            s,
            l: lightness.clamp(0.0, 1.0),
            a: 1.0,
        };
        div()
            .flex()
            .flex_shrink_0()
            .size(px(16.0))
            .items_center()
            .justify_center()
            .rounded(px(4.5))
            .bg(gpui::linear_gradient(
                180.0,
                gpui::linear_color_stop(tone(l + 0.07), 0.0),
                gpui::linear_color_stop(tone(l - 0.05), 1.0),
            ))
            .text_size(px(10.0))
            .line_height(px(12.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(gpui::Hsla {
                h: 0.0,
                s: 0.0,
                l: 1.0,
                a: 0.94,
            })
            .child(initial(&project.label))
            .into_any_element()
    }

    /// One place's logo at a row's end, in its column: bright with a dot under it when it is the
    /// row's pick, dimmer when it isn't, faint and not clickable when its machine isn't connected.
    /// A click takes the project there.
    fn render_place_cell(
        &self,
        chip: &RunOnChip,
        picked: bool,
        project: usize,
        place: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        let selectable = chip.selectable;
        // A project only on a machine that isn't connected starts there, but its logo stays faint.
        let picked = picked && selectable;
        let color = if picked {
            c.selected_label
        } else if selectable {
            rgba_of(c.glyph, 0.62)
        } else {
            rgba_of(c.glyph, 0.24)
        };
        let tooltip = place_tooltip(chip);
        div()
            .id(ElementId::Name(
                format!("new-thread-place-{project}-{place}").into(),
            ))
            .relative()
            .flex()
            .flex_shrink_0()
            .size(px(PLACE_CELL))
            .justify_center()
            .pt(px((PLACE_CELL - PLACE_LOGO) / 2.0 - 1.0))
            .rounded(px(6.0))
            .hover(|cell| cell.bg(hsla(rgba_of(c.foreground, 0.09))))
            .child(modal_icon(chip.icon, PLACE_LOGO, color))
            .when(picked, |cell| {
                cell.child(
                    div()
                        .absolute()
                        .bottom(px(2.0))
                        .left(px((PLACE_CELL - 3.0) / 2.0))
                        .size(px(3.0))
                        .rounded_full()
                        .bg(hsla(c.selected_label)),
                )
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if selectable {
                    this.pick_project(project, place, window, cx);
                }
            }))
            .into_any_element()
    }

    /// `←→ Machine` at the search field's end while the project list is open and a row has a
    /// place to choose: the one key the list adds, in place of the key hints row.
    pub(super) fn render_project_search_hint(&self) -> Option<AnyElement> {
        if !self.choosing_project || !self.any_row_has_places() {
            return None;
        }
        let c = self.colors;
        Some(
            h_flex()
                .flex_shrink_0()
                .gap(px(5.0))
                .items_center()
                .whitespace_nowrap()
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(hsla(rgba_of(c.muted, 0.8)))
                .child(
                    div()
                        .text_color(hsla(rgba_of(c.foreground, 0.7)))
                        .child(PLACE_KEYS),
                )
                .child(HINT_PLACE)
                .into_any_element(),
        )
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

    /// The project list: Recent, then All projects, one line per project with its places' logos
    /// in their columns at the end.
    pub(super) fn render_project_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = self.colors;
        let rows = self.project_rows();
        let sections = self.project_sections(&rows);
        let columns = self.place_columns();
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
            let chips = self
                .project_places(index)
                .as_ref()
                .map(RunOnState::chips)
                .unwrap_or_default();
            let picked = self.row_place(index);
            let cells: Vec<AnyElement> = columns
                .iter()
                .map(|key| match chips.iter().position(|chip| chip.key == *key) {
                    Some(chip_index) => self.render_place_cell(
                        &chips[chip_index],
                        chip_index == picked,
                        index,
                        chip_index,
                        &mut *cx,
                    ),
                    None => div()
                        .flex_shrink_0()
                        .size(px(PLACE_CELL))
                        .into_any_element(),
                })
                .collect();
            children.push(
                self.row_shell(
                    ElementId::Name(format!("new-thread-project-{index}").into()),
                    *row,
                    selected,
                    cx,
                )
                .h(px(PROJECT_ROW_HEIGHT))
                .min_h(px(PROJECT_ROW_HEIGHT))
                .pr(px(4.0))
                .child(self.render_project_tile(project))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(6.0))
                        .items_center()
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(project.label.clone()),
                        )
                        .when_some(project.detail.clone(), |line, detail| {
                            line.child(
                                div()
                                    .flex_shrink_0()
                                    .whitespace_nowrap()
                                    .text_size(px(12.0))
                                    .text_color(hsla(rgba_of(c.muted, 0.8)))
                                    .child(detail),
                            )
                        })
                        .when(!project.has_sessions, |line| {
                            line.child(
                                div()
                                    .flex_shrink_0()
                                    .whitespace_nowrap()
                                    .text_size(px(11.5))
                                    .text_color(hsla(rgba_of(c.muted, 0.7)))
                                    .child(NO_SESSIONS),
                            )
                        }),
                )
                .when(!cells.is_empty(), |row| {
                    row.child(h_flex().flex_shrink_0().items_center().children(cells))
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
    fn a_row_lists_its_places_only_when_there_is_a_choice_or_it_is_elsewhere() {
        let chip = |label: &str| RunOnChip {
            key: label.to_lowercase(),
            label: label.to_string(),
            name: label.to_string(),
            icon: "",
            selectable: true,
            tooltip: None,
        };
        let project = |machine_id: &str| UnifiedProjectChoice {
            repo_key: "r".to_string(),
            label: "r".to_string(),
            has_sessions: true,
            recent: false,
            machines: vec![ghostex_gx_core::unified_sidebar::UnifiedChoiceMachine {
                machine_id: machine_id.to_string(),
                label: machine_id.to_string(),
                connected: true,
                project_id: "p".to_string(),
            }],
            selected: 0,
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
    fn the_project_list_is_the_picker_without_its_agent_rows_or_key_hints() {
        assert_eq!(PROJECT_LIST_CHROME_HEIGHT, 52.0);
    }

    fn place(key: &str, selectable: bool) -> RunOnChip {
        RunOnChip {
            key: key.to_string(),
            label: key.to_string(),
            name: format!("{key} name"),
            icon: "",
            selectable,
            tooltip: None,
        }
    }

    fn mark(machine_id: &str) -> NewThreadPickerMark {
        NewThreadPickerMark {
            machine_id: machine_id.to_string(),
            asset: "titlebar/brand-apple.svg",
            name: format!("{machine_id} name"),
        }
    }

    #[test]
    fn the_columns_are_this_computer_the_sidebars_machines_cloud_box_then_the_boxes() {
        let marks = [mark("local"), mark("windows"), mark("arch")];
        let rows = vec![
            // A project only on Arch comes first, yet Arch keeps its place after Windows.
            (vec![place("arch", true)], 1, true),
            (
                vec![
                    place("local", true),
                    place("agentbox:docker", true),
                    place(CLOUD_BOX_RUN_LOCATION, true),
                ],
                1,
                true,
            ),
            (
                vec![place("windows", false), place("studio", true)],
                2,
                true,
            ),
        ];
        assert_eq!(
            place_columns_of(&rows, &marks),
            [
                "local",
                "windows",
                "arch",
                "studio",
                CLOUD_BOX_RUN_LOCATION,
                "agentbox:docker"
            ]
        );
        // Every project only here with nothing to choose: no logos at all.
        assert!(place_columns_of(&[(vec![place("local", true)], 1, false)], &marks).is_empty());
    }

    #[test]
    fn a_logo_names_its_place_or_says_its_machine_isnt_connected() {
        assert_eq!(place_tooltip(&place("local", true)), "local name");
        assert_eq!(
            place_tooltip(&place("windows", false)),
            "windows name isn't connected"
        );
    }

    #[test]
    fn a_project_keeps_its_tile_colour() {
        let tone = tile_tone("github.com/maddada/openusage");
        assert_eq!(tone, tile_tone("github.com/maddada/openusage"));
        assert!(TILE_TONES.contains(&tone));
        // Different projects spread over the tones instead of sharing one.
        let tones: std::collections::BTreeSet<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|key| format!("{:?}", tile_tone(key)))
            .collect();
        assert!(tones.len() > 3);
        // Soft, never black: every tone is lighter than a third.
        assert!(TILE_TONES.iter().all(|(_, _, lightness)| *lightness > 0.33));
    }

    #[test]
    fn a_machine_wears_its_mark_and_name() {
        let project = UnifiedProjectChoice {
            repo_key: "r".to_string(),
            label: "r".to_string(),
            machines: vec![
                ghostex_gx_core::unified_sidebar::UnifiedChoiceMachine {
                    machine_id: "local".to_string(),
                    label: "Local".to_string(),
                    connected: true,
                    project_id: "p".to_string(),
                },
                ghostex_gx_core::unified_sidebar::UnifiedChoiceMachine {
                    machine_id: "box".to_string(),
                    label: "Box".to_string(),
                    connected: false,
                    project_id: "q".to_string(),
                },
            ],
            ..Default::default()
        };
        let machines = run_on_machines(&project, &[mark("local")]);
        assert_eq!(machines[0].icon, Some("titlebar/brand-apple.svg"));
        assert_eq!(machines[0].name.as_deref(), Some("local name"));
        assert!(machines[0].local);
        // A machine no mark names keeps the generic drawing and its label.
        assert_eq!(machines[1].icon, None);
        assert!(!machines[1].connected);
    }
}
