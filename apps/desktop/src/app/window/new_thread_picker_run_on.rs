//! The New Thread picker's Run on row: this computer, or an agentbox box gxserver reported ready.
//! A child of `new_thread_picker.rs` (included with `#[path]`, so the preview binary builds it
//! too), which is why it reads the picker's private state directly.
//!
//! CDXC:AgentBox 2026-10-01 DECISION:
//! User: "when spinning up a thread i should be able to pick to spin it up in one of these clouds". The picker carries a Run on row under its search field: This computer, then one chip per location `/api/agentbox status` reports ready (Docker, a cloud provider, or a registered SSH host by its alias, with "Your server <alias> over SSH" on hover), starting on Settings > Cloud Boxes' default location. Cmd+Left and Cmd+Right (Alt+Left and Alt+Right off macOS) or a click move between them; Tab stays the account list. A box location lists only the agents agentbox runs (Claude, Codex, OpenCode, Pi), drops the Browser and Terminal rows and the account list (a box signs in on its own), and launches with that `runLocation`. With agentbox installed but no box ready, the row offers a "Run in a cloud box…" link to Settings > Cloud Boxes.
//!
//! CDXC:AgentBox 2026-10-01 WHY:
//! (Superseded for a picker that knows its project's machines, see 2026-11-03 above.) The orchestrator decided, while the user was away, that the row and its 30px appear only when agentbox is installed and the project is on this computer; without agentbox, on Windows, for a remote project, or before a status answer, the picker is exactly the one the user sized (CDXC:AgentLauncher 2026-09-09 in new_thread_picker.rs), and discovery stays in Settings > Cloud Boxes and the sidebar's Select Agent menu.
//! SEE-ALSO: packages/gx-core/src/agentbox.rs (which locations are ready, which agents a box runs), apps/desktop/src/app/gx_store/agentbox.rs (the status read), packages/gx-core/src/sidebar_menu/run_in_box.rs (the sidebar launcher's twin).
//!
//! CDXC:AgentLauncher 2026-11-03 DECISION:
//! User (Q1180, "btw I don't like this "Run on" row :\ it's left aligned while the rest are filling the row"; the page's Q1181 "per-project" and its offline-chip note, unobjected): the machine a thread starts on is picked on this row. It starts with a chip per machine that has the picked project (this computer's chip first when it has it), starting on the last machine used for that project, then Cloud Box and the boxes, which only run here, so they show only while the project is on this computer. A machine that isn't connected keeps its chip, dimmed, and can't be picked; Cmd+Left and Cmd+Right step over it. The chips share the row's whole width, as the rows above and below fill theirs, and scroll only when there are too many to fit. The row shows whenever the picker knows the project's machines, so it is there on every machine; without them (the preview binary) it keeps the This computer chip and its agentbox-only rule below. This supersedes the WHY below, under which the row showed only with agentbox installed and the project on this computer.
//!
//! CDXC:AgentLauncher 2026-11-03 DECISION:
//! User, of the row above the key hints in the real app: "how can we make this area look better. it's not good. let's not show the machines here, show the machines inside the project picking area please instead". With its projects known the picker draws no Run on row: the places (`RunOnState::chips`: the machines, then Cloud Box and the boxes for a project on this computer) are chips on each row of the project list, and the top row names the picked place beside the project. Cloud Box isn't a machine but it is a place a thread starts in, so it is a chip on that list too, after this computer's machines. Cmd+Left and Cmd+Right still step through the places with the list closed. The row below stays only for a picker without projects (the preview binary). This supersedes the fill-the-row look above for the app.
//!
//! CDXC:ClaudeCloud 2026-10-29 DECISION:
//! User: "also no need for this machine/cloud since I can just pick cloud claude there in agents list". Claude cloud is no longer a place in this row (supersedes the 2026-10-23 decision in packages/gx-core/src/claude_cloud.rs that put it here): the user starts a cloud session by picking "Claude (cloud)" in the agent list. Without agentbox the row and its Location key hint are gone, as before 2026-10-23; with agentbox it holds This computer and the boxes.
use super::*;
use gpui_component::input::{MoveEnd, MoveHome};

/// The row: an 8px gap under the search field and 22px chips.
pub(crate) const NEW_THREAD_PICKER_RUN_ON_HEIGHT: f32 = 30.0;

const RUN_ON_LABEL: &str = "Run on";
/// The Cloud Box chip's run location: the chat opens with the tray's Cloud Box segment on and the
/// box starts with its first message.
pub(crate) const CLOUD_BOX_RUN_LOCATION: &str = "cloudBox";
const CLOUD_BOX_LABEL: &str = "Cloud Box";
const CLOUD_BOX_TOOLTIP: &str =
    "Runs on a cloud box. The box starts when you send the first message.";
const ICON_CLOUD_BOX: &str = "titlebar/cloud.svg";
const THIS_COMPUTER: &str = "This computer";
const SET_UP_LINK: &str = "Run in a cloud box…";
const ICON_THIS_COMPUTER: &str = "titlebar/device-desktop.svg";
const ICON_REMOTE_MACHINE: &str = "titlebar/server.svg";
/// A chip's narrowest width while the chips share the row; past that the row scrolls.
const CHIP_MIN_WIDTH: f32 = 76.0;
#[cfg(target_os = "macos")]
const KEYS: &str = "⌘←→";
#[cfg(not(target_os = "macos"))]
const KEYS: &str = "Alt ←→";
const HINT_LOCATION: &str = "Location";
/// A chip's label is cut short with an ellipsis past this width; the row scrolls when the chips
/// still do not fit.
const CHIP_LABEL_MAX_WIDTH: f32 = 120.0;

/// One box location the picker can launch in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerLocation {
    /// The `runLocation` a launch carries (`agentbox:docker`, `agentbox:docker:<alias>`, ...).
    pub(crate) run_location: String,
    pub(crate) label: String,
    /// `local`, `cloud` or `remoteDocker`: picks the chip's glyph.
    pub(crate) kind: String,
    /// The hover text (`Your server <alias> over SSH` for an SSH host).
    pub(crate) tooltip: Option<String>,
}

impl NewThreadPickerLocation {
    fn icon(&self) -> &'static str {
        match self.kind.as_str() {
            "local" => "titlebar/box.svg",
            "remoteDocker" => "titlebar/server.svg",
            _ => "titlebar/cloud.svg",
        }
    }
}

/// What the host knows about boxes on this computer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum NewThreadPickerBoxes {
    /// Not read yet, gxserver could not answer, agentbox is not installed, or the project is on
    /// another machine: no Run on row.
    #[default]
    Unknown,
    /// agentbox cannot run here (native Windows): no Run on row.
    Unsupported,
    /// agentbox is missing or no box is ready: This computer and the Settings link.
    NotSetUp,
    /// The locations a box can start in right now.
    Ready(Vec<NewThreadPickerLocation>),
}

impl NewThreadPickerBoxes {
    /// Whether the picker draws the Run on row: agentbox is installed here (a box is ready, or
    /// none yet).
    pub(crate) fn shows_run_on(&self) -> bool {
        matches!(self, Self::NotSetUp | Self::Ready(_))
    }

    fn locations(&self) -> &[NewThreadPickerLocation] {
        match self {
            Self::Ready(locations) => locations,
            _ => &[],
        }
    }
}

/// One machine chip of the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunOnMachine {
    pub(crate) label: String,
    pub(crate) connected: bool,
    /// This computer, where boxes run.
    pub(crate) local: bool,
}

/// One place a thread can start: a machine, Cloud Box or a box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunOnChip {
    pub(crate) label: String,
    pub(crate) icon: &'static str,
    pub(crate) selectable: bool,
    pub(crate) tooltip: Option<String>,
}

/// The row's state: the project's machines, the offered boxes, the default location, and which
/// chip is picked.
#[derive(Clone, Debug, Default)]
pub(crate) struct RunOnState {
    /// The project's machines; empty draws the single This computer chip.
    machines: Vec<RunOnMachine>,
    /// The picked machine chip, while no box is picked.
    machine: usize,
    boxes: NewThreadPickerBoxes,
    /// Settings' default location (`local` or a `runLocation`), applied until the user picks.
    preferred: String,
    /// The picked box's `runLocation`; `None` is This computer.
    selected: Option<String>,
    /// The user picked a chip during this open, so a late status answer keeps their pick.
    touched: bool,
    /// Cloud Boxes is on and the project is on this computer: the Cloud Box chip follows This
    /// computer (D3).
    cloud_box: bool,
}

impl RunOnState {
    pub(crate) fn new(boxes: NewThreadPickerBoxes, preferred: String) -> Self {
        let mut state = Self {
            machines: Vec::new(),
            machine: 0,
            boxes,
            preferred,
            selected: None,
            touched: false,
            cloud_box: false,
        };
        state.apply_preferred();
        state
    }

    /// Settings' default box applies only while the picked machine is this computer, where the
    /// boxes run.
    fn apply_preferred(&mut self) {
        let preferred = self.preferred.as_str();
        self.selected = self
            .machine_is_local()
            .then(|| {
                self.boxes
                    .locations()
                    .iter()
                    .find(|location| location.run_location == preferred)
                    .map(|location| location.run_location.clone())
            })
            .flatten();
    }

    /// The project's machines, starting on `machine` (or the first connected one when that one
    /// can't be reached); a box pick is dropped when the project isn't on this computer.
    pub(crate) fn set_machines(&mut self, machines: Vec<RunOnMachine>, machine: usize) {
        self.machines = machines;
        self.machine = machine.min(self.machines.len().saturating_sub(1));
        if !self.machine_selectable(self.machine)
            && let Some(first) =
                (0..self.machines.len()).find(|index| self.machine_selectable(*index))
        {
            self.machine = first;
        }
        if !self.local_offered() {
            self.selected = None;
        } else if !self.touched {
            self.apply_preferred();
        }
    }

    /// The places in chip order: the machines (This computer alone without them), Cloud Box, then
    /// the boxes; the project list draws them on each project's row.
    pub(crate) fn chips(&self) -> Vec<RunOnChip> {
        let mut chips: Vec<RunOnChip> = if self.machines.is_empty() {
            vec![RunOnChip {
                label: THIS_COMPUTER.to_string(),
                icon: ICON_THIS_COMPUTER,
                selectable: true,
                tooltip: Some(ghostex_gx_core::THIS_COMPUTER_DESCRIPTION.to_string()),
            }]
        } else {
            self.machines
                .iter()
                .map(|machine| RunOnChip {
                    label: machine.label.clone(),
                    icon: if machine.local {
                        ICON_THIS_COMPUTER
                    } else {
                        ICON_REMOTE_MACHINE
                    },
                    selectable: machine.connected,
                    tooltip: if !machine.connected {
                        Some(format!("{} isn't connected", machine.label))
                    } else if machine.local {
                        Some(ghostex_gx_core::THIS_COMPUTER_DESCRIPTION.to_string())
                    } else {
                        None
                    },
                })
                .collect()
        };
        if self.cloud_box_offered() {
            chips.push(RunOnChip {
                label: CLOUD_BOX_LABEL.to_string(),
                icon: ICON_CLOUD_BOX,
                selectable: true,
                tooltip: Some(CLOUD_BOX_TOOLTIP.to_string()),
            });
        }
        for location in &self.boxes.locations()[..self.box_count()] {
            chips.push(RunOnChip {
                label: location.label.clone(),
                icon: location.icon(),
                selectable: true,
                tooltip: location.tooltip.clone(),
            });
        }
        chips
    }

    /// The same row for another project: its machines, starting on `machine`.
    pub(crate) fn for_machines(&self, machines: Vec<RunOnMachine>, machine: usize) -> Self {
        let mut state = self.clone();
        state.touched = false;
        state.set_machines(machines, machine);
        state
    }

    /// The next place that can be picked from `from`, stopping at the ends.
    pub(crate) fn step(&self, from: usize, forward: bool) -> usize {
        let next = if forward {
            (from + 1..self.chip_count()).find(|index| self.selectable(*index))
        } else {
            (0..from).rev().find(|index| self.selectable(*index))
        };
        next.unwrap_or(from)
    }

    fn machine_count(&self) -> usize {
        self.machines.len().max(1)
    }

    fn machine_selectable(&self, index: usize) -> bool {
        self.machines
            .get(index)
            .is_none_or(|machine| machine.connected)
    }

    fn machine_is_local(&self) -> bool {
        self.machines
            .get(self.machine)
            .is_none_or(|machine| machine.local)
    }

    /// Boxes and Cloud Box run on this computer, so they are offered while the project is here.
    fn local_offered(&self) -> bool {
        self.machines.is_empty() || self.machines.iter().any(|machine| machine.local)
    }

    /// The machine the thread starts on: the picked chip, or this computer for a box.
    pub(crate) fn machine_index(&self) -> usize {
        if self.selected.is_some() {
            return self
                .machines
                .iter()
                .position(|machine| machine.local)
                .unwrap_or(self.machine);
        }
        self.machine
    }

    /// A fresh status: the user's pick stays while it is still offered, otherwise the default.
    pub(crate) fn set_boxes(&mut self, boxes: NewThreadPickerBoxes) {
        self.boxes = boxes;
        let still_offered = self.selected.as_deref().is_none_or(|selected| {
            self.boxes
                .locations()
                .iter()
                .any(|location| location.run_location == selected)
        });
        if !self.touched || !still_offered {
            self.touched = self.touched && still_offered;
            self.apply_preferred();
        }
        if !self.local_offered() {
            self.selected = None;
        }
    }

    /// Offers the Cloud Box chip (Cloud Boxes on, project on this computer).
    pub(crate) fn with_cloud_box(mut self, offered: bool) -> Self {
        self.cloud_box = offered;
        self
    }

    /// The picked box's `runLocation` ([`CLOUD_BOX_RUN_LOCATION`] for the Cloud Box chip); `None`
    /// runs on this computer.
    pub(crate) fn run_location(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// An agentbox box is picked (its agents and no account list); the Cloud Box chip starts the
    /// chat here and keeps the full list.
    pub(crate) fn in_box(&self) -> bool {
        self.selected
            .as_deref()
            .is_some_and(|selected| selected != CLOUD_BOX_RUN_LOCATION)
    }

    pub(crate) fn shown(&self) -> bool {
        !self.machines.is_empty() || self.boxes.shows_run_on() || self.cloud_box
    }

    fn cloud_box_offered(&self) -> bool {
        self.cloud_box && self.local_offered()
    }

    fn box_count(&self) -> usize {
        if self.local_offered() {
            self.boxes.locations().len()
        } else {
            0
        }
    }

    /// The boxes after the machines and the Cloud Box chip.
    fn box_offset(&self) -> usize {
        self.machine_count() + usize::from(self.cloud_box_offered())
    }

    /// Chip index: the machines (This computer alone without them), then Cloud Box when offered,
    /// then the boxes in order.
    pub(crate) fn selected_index(&self) -> usize {
        match self.selected.as_deref() {
            Some(CLOUD_BOX_RUN_LOCATION) if self.cloud_box_offered() => self.machine_count(),
            selected => selected
                .and_then(|selected| {
                    self.boxes
                        .locations()
                        .iter()
                        .position(|location| location.run_location == selected)
                })
                .map_or(self.machine, |index| index + self.box_offset()),
        }
    }

    /// Whether the chip at `index` can be picked: a machine that isn't connected can't.
    pub(crate) fn selectable(&self, index: usize) -> bool {
        index >= self.machine_count() || self.machine_selectable(index)
    }

    pub(crate) fn select_index(&mut self, index: usize) {
        if !self.selectable(index) {
            return;
        }
        self.touched = true;
        if index < self.machine_count() {
            self.machine = index;
            self.selected = None;
            return;
        }
        if self.cloud_box_offered() && index == self.machine_count() {
            self.selected = Some(CLOUD_BOX_RUN_LOCATION.to_string());
            return;
        }
        self.selected = index
            .checked_sub(self.box_offset())
            .and_then(|index| self.boxes.locations().get(index))
            .map(|location| location.run_location.clone());
    }

    pub(crate) fn chip_count(&self) -> usize {
        self.box_offset() + self.box_count()
    }
}

/// The agents agentbox runs, by their icon (the family) or their id.
pub(crate) fn runs_in_box(agent: &NewThreadPickerAgent) -> bool {
    matches!(agent.family(), "claude" | "codex" | "opencode" | "pi")
}

impl GpuiNewThreadPickerWindow {
    /// The host's newest read of `/api/agentbox status`.
    pub(crate) fn set_run_locations(
        &mut self,
        boxes: NewThreadPickerBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_in_box = self.run_on.in_box();
        let was_shown = self.run_on.shown();
        self.run_on.set_boxes(boxes);
        if was_in_box != self.run_on.in_box() {
            self.run_location_changed(window, cx);
        }
        if was_shown != self.run_on.shown() {
            self.fit_window_height(window);
        }
        self.reveal_selected_run_location();
        cx.notify();
    }

    /// Preview hook: picks the chip at `index` (0 is This computer).
    #[allow(dead_code)]
    pub(crate) fn preview_select_run_location(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_run_location(index, window, cx);
    }

    fn pick_run_location(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let index = index.min(self.run_on.chip_count() - 1);
        let was = self.run_on.selected_index();
        self.run_on.select_index(index);
        if was != self.run_on.selected_index() {
            self.run_location_changed(window, cx);
        }
        self.reveal_selected_run_location();
        cx.notify();
    }

    /// Scrolls the chips so the picked one is on screen when they do not all fit.
    pub(super) fn reveal_selected_run_location(&self) {
        self.run_on_scroll
            .scroll_to_item(self.run_on.selected_index());
    }

    /// `⌘←→ Location` in the key-hint row, while there is more than one place to pick.
    pub(super) fn run_on_key_hint(&self) -> Option<impl IntoElement> {
        (self.run_on.shown()
            && self.run_on.chip_count() > 1
            && self.scope.is_none()
            && !self.choosing_project
            && self.place.is_none())
        .then(|| self.render_hint(&[KEYS], HINT_LOCATION))
    }

    /// Cmd+Left and Cmd+Right (Alt off macOS): the previous or next location, wrapping.
    pub(super) fn cycle_run_location(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.run_on.chip_count() as isize;
        if count < 2 {
            return;
        }
        let start = self.run_on.selected_index() as isize;
        let next = (1..count)
            .map(|step| (start + delta * step).rem_euclid(count) as usize)
            .find(|index| self.run_on.selectable(*index));
        if let Some(next) = next {
            self.pick_run_location(next, window, cx);
        }
    }

    /// A box has no account list and fewer rows, so the list is rebuilt around the agent that was
    /// highlighted.
    pub(super) fn run_location_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let highlighted = self.scope.or(match self.selected_row() {
            Some(PickerRow::Agent(index)) => Some(index),
            _ => None,
        });
        if self.scope.is_some() && self.run_on.in_box() {
            self.leave_scope(window, cx);
        }
        let rows = self.rows();
        self.selected = highlighted
            .and_then(|index| rows.iter().position(|row| *row == PickerRow::Agent(index)))
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.scroll_child_index(&rows, self.selected));
    }

    /// The Cmd+Left half of the search field's `MoveHome` on macOS; Ctrl+A (the same action)
    /// and the Home key still move the caret.
    pub(super) fn on_move_home(
        &mut self,
        _: &MoveHome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cfg!(target_os = "macos") && window.modifiers().platform && self.run_on.chip_count() > 1
        {
            cx.stop_propagation();
            self.cycle_run_location(-1, window, cx);
            return;
        }
        cx.propagate();
    }

    /// The Cmd+Right half of `MoveEnd` on macOS; Ctrl+E and End still move the caret.
    pub(super) fn on_move_end(&mut self, _: &MoveEnd, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(target_os = "macos") && window.modifiers().platform && self.run_on.chip_count() > 1
        {
            cx.stop_propagation();
            self.cycle_run_location(1, window, cx);
            return;
        }
        cx.propagate();
    }

    /// Alt+Left and Alt+Right, which the search field does not bind off macOS.
    pub(super) fn on_run_on_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        if cfg!(target_os = "macos")
            || !keystroke.modifiers.alt
            || keystroke.modifiers.control
            || keystroke.modifiers.shift
        {
            return;
        }
        let delta = match keystroke.key.as_str() {
            "left" => -1,
            "right" => 1,
            _ => return,
        };
        if self.run_on.chip_count() > 1 {
            cx.stop_propagation();
            self.cycle_run_location(delta, window, cx);
        }
    }

    fn render_run_on_chip(
        &self,
        index: usize,
        icon: &'static str,
        label: String,
        tooltip: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        let selected = self.run_on.selected_index() == index;
        let selectable = self.run_on.selectable(index);
        h_flex()
            .id(ElementId::Name(format!("new-thread-run-on-{index}").into()))
            .flex_1()
            .flex_basis(px(0.0))
            .min_w(px(CHIP_MIN_WIDTH))
            .h(px(22.0))
            .items_center()
            .justify_center()
            .when(!selectable, |chip| chip.opacity(0.45))
            .gap(px(5.0))
            .px(px(7.0))
            .rounded(px(4.0))
            .border_1()
            .text_size(px(12.0))
            .line_height(px(16.0))
            .when(selected, |chip| {
                chip.bg(hsla(c.selected_background))
                    .border_color(hsla(rgba_of(c.foreground, 0.28)))
                    .text_color(hsla(c.selected_label))
            })
            .when(!selected, |chip| {
                chip.border_color(hsla(rgba_of(c.foreground, 0.10)))
                    .text_color(hsla(c.muted))
            })
            .when(!selected && selectable, |chip| {
                chip.hover(|chip| chip.bg(hsla(rgba_of(c.foreground, 0.06))))
            })
            .child(modal_icon(
                icon,
                13.0,
                if selected { c.selected_label } else { c.glyph },
            ))
            .child(
                div()
                    .min_w_0()
                    .max_w(px(CHIP_LABEL_MAX_WIDTH))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
            )
            .when_some(tooltip, |chip, tooltip| {
                chip.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.run_on.selectable(index) {
                    this.pick_run_location(index, window, cx);
                }
                this.input.update(cx, |input, cx| input.focus(window, cx));
            }))
            .into_any_element()
    }

    /// `Run on [This computer] [Docker] [Hetzner] ...`, or the Settings link when no box is set
    /// up. Its keys are in the key-hint row, so the chips have the whole width.
    pub(super) fn render_run_on(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors;
        // Asked on every draw: the first frame has no viewport width yet, so a request made only
        // when the pick changes can be spent before the chips are measured.
        self.reveal_selected_run_location();
        let mut chips: Vec<AnyElement> = if self.run_on.machines.is_empty() {
            vec![self.render_run_on_chip(
                0,
                ICON_THIS_COMPUTER,
                THIS_COMPUTER.to_string(),
                Some(ghostex_gx_core::THIS_COMPUTER_DESCRIPTION.to_string()),
                cx,
            )]
        } else {
            self.run_on
                .machines
                .iter()
                .enumerate()
                .map(|(index, machine)| {
                    let tooltip = if !machine.connected {
                        Some(format!("{} isn't connected", machine.label))
                    } else if machine.local {
                        Some(ghostex_gx_core::THIS_COMPUTER_DESCRIPTION.to_string())
                    } else {
                        (machine.label.chars().count() > 12).then(|| machine.label.clone())
                    };
                    self.render_run_on_chip(
                        index,
                        if machine.local {
                            ICON_THIS_COMPUTER
                        } else {
                            ICON_REMOTE_MACHINE
                        },
                        machine.label.clone(),
                        tooltip,
                        cx,
                    )
                })
                .collect()
        };
        if self.run_on.cloud_box_offered() {
            chips.push(self.render_run_on_chip(
                self.run_on.machine_count(),
                ICON_CLOUD_BOX,
                CLOUD_BOX_LABEL.to_string(),
                Some(CLOUD_BOX_TOOLTIP.to_string()),
                cx,
            ));
        }
        let box_offset = self.run_on.box_offset();
        let boxes = &self.run_on.boxes.locations()[..self.run_on.box_count()];
        for (offset, location) in boxes.iter().enumerate() {
            // A label the chip cuts short is readable on hover.
            let tooltip = location
                .tooltip
                .clone()
                .or_else(|| (location.label.chars().count() > 16).then(|| location.label.clone()));
            chips.push(self.render_run_on_chip(
                offset + box_offset,
                location.icon(),
                location.label.clone(),
                tooltip,
                cx,
            ));
        }
        let set_up = matches!(self.run_on.boxes, NewThreadPickerBoxes::NotSetUp)
            && self.run_on.local_offered();
        h_flex()
            .id("ghostex-gpui-new-thread-picker-run-on")
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .h(px(NEW_THREAD_PICKER_RUN_ON_HEIGHT))
            .pt(px(8.0))
            .px(px(10.0))
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(hsla(rgba_of(c.muted, 0.8)))
                    .child(RUN_ON_LABEL),
            )
            .child(
                h_flex()
                    .id("ghostex-gpui-new-thread-picker-run-on-chips")
                    .flex_1()
                    .min_w_0()
                    .gap(px(5.0))
                    .items_center()
                    .overflow_x_scroll()
                    .track_scroll(&self.run_on_scroll)
                    .children(chips)
                    .when(set_up, |row| {
                        row.child(
                            div()
                                .id("ghostex-gpui-new-thread-picker-run-on-setup")
                                .flex_shrink_0()
                                .ml(px(3.0))
                                .text_size(px(12.0))
                                .whitespace_nowrap()
                                .text_color(hsla(rgba_of(c.muted, 0.9)))
                                .hover(|link| link.text_color(hsla(c.foreground)))
                                .child(SET_UP_LINK)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.finish(
                                        NewThreadPickerCommand::OpenCloudBoxesSettings,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docker() -> NewThreadPickerLocation {
        NewThreadPickerLocation {
            run_location: "agentbox:docker".to_string(),
            label: "Docker".to_string(),
            kind: "local".to_string(),
            tooltip: None,
        }
    }

    #[test]
    fn the_row_shows_only_with_agentbox_installed() {
        for boxes in [
            NewThreadPickerBoxes::Unknown,
            NewThreadPickerBoxes::Unsupported,
        ] {
            assert!(!RunOnState::new(boxes, "local".to_string()).shown());
        }
        let not_set_up = RunOnState::new(NewThreadPickerBoxes::NotSetUp, "local".to_string());
        assert!(not_set_up.shown());
        assert_eq!(not_set_up.chip_count(), 1);
    }

    fn machine(label: &str, connected: bool, local: bool) -> RunOnMachine {
        RunOnMachine {
            label: label.to_string(),
            connected,
            local,
        }
    }

    #[test]
    fn the_projects_machines_lead_and_boxes_follow_only_on_this_computer() {
        let mut state = RunOnState::new(
            NewThreadPickerBoxes::Ready(vec![docker()]),
            "local".to_string(),
        );
        state.set_machines(
            vec![machine("Mac", true, true), machine("Server A", true, false)],
            1,
        );
        assert!(state.shown());
        assert_eq!(state.chip_count(), 3);
        assert_eq!(state.selected_index(), 1);
        assert_eq!(state.machine_index(), 1);
        // A box runs on this computer.
        state.select_index(2);
        assert_eq!(state.run_location(), Some("agentbox:docker"));
        assert_eq!(state.machine_index(), 0);
        // A project only on another machine offers no box.
        state.set_machines(vec![machine("Server A", true, false)], 0);
        assert_eq!(state.chip_count(), 1);
        assert_eq!(state.run_location(), None);
    }

    #[test]
    fn the_places_are_the_machines_then_cloud_box_then_the_boxes() {
        let state = RunOnState::new(
            NewThreadPickerBoxes::Ready(vec![docker()]),
            "local".to_string(),
        )
        .with_cloud_box(true)
        .for_machines(
            vec![machine("Mac", true, true), machine("Studio", false, false)],
            0,
        );
        let labels: Vec<(String, bool)> = state
            .chips()
            .into_iter()
            .map(|chip| (chip.label, chip.selectable))
            .collect();
        assert_eq!(
            labels,
            [
                ("Mac".to_string(), true),
                ("Studio".to_string(), false),
                ("Cloud Box".to_string(), true),
                ("Docker".to_string(), true),
            ]
        );
        // Steps skip the machine that can't be reached and stop at the ends.
        assert_eq!(state.step(0, true), 2);
        assert_eq!(state.step(3, true), 3);
        assert_eq!(state.step(2, false), 0);
        // A project only elsewhere has no Cloud Box or box.
        let remote = state.for_machines(vec![machine("Studio", true, false)], 0);
        assert_eq!(remote.chips().len(), 1);
    }

    #[test]
    fn a_machine_that_is_not_connected_cannot_be_picked() {
        let mut state = RunOnState::new(NewThreadPickerBoxes::Unknown, "local".to_string());
        state.set_machines(
            vec![machine("Mac", true, true), machine("Studio", false, false)],
            1,
        );
        // It was the start, but it can't be reached.
        assert_eq!(state.machine_index(), 0);
        state.select_index(1);
        assert_eq!(state.machine_index(), 0);
        assert!(!state.selectable(1));
    }

    #[test]
    fn the_chips_are_this_computer_then_the_boxes() {
        let mut state = RunOnState::new(
            NewThreadPickerBoxes::Ready(vec![docker()]),
            "agentbox:docker".to_string(),
        );
        assert_eq!(state.chip_count(), 2);
        assert_eq!(state.selected_index(), 1);
        assert_eq!(state.run_location(), Some("agentbox:docker"));
        state.select_index(0);
        assert_eq!(state.run_location(), None);
        assert!(!state.in_box());
    }
}
