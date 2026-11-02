//! The New Thread picker's Run on row: this computer, or an agentbox box gxserver reported ready.
//! A child of `new_thread_picker.rs` (included with `#[path]`, so the preview binary builds it
//! too), which is why it reads the picker's private state directly.
//!
//! CDXC:AgentBox 2026-10-01 DECISION:
//! User: "when spinning up a thread i should be able to pick to spin it up in one of these clouds". The picker carries a Run on row under its search field: This computer, then one chip per location `/api/agentbox status` reports ready (Docker, a cloud provider, or a registered SSH host by its alias, with "Your server <alias> over SSH" on hover), starting on Settings > Cloud Boxes' default location. Cmd+Left and Cmd+Right (Alt+Left and Alt+Right off macOS) or a click move between them; Tab stays the account list. A box location lists only the agents agentbox runs (Claude, Codex, OpenCode, Pi), drops the Browser and Terminal rows and the account list (a box signs in on its own), and launches with that `runLocation`. With agentbox installed but no box ready, the row offers a "Run in a cloud box…" link to Settings > Cloud Boxes.
//!
//! CDXC:AgentBox 2026-10-01 WHY:
//! The orchestrator decided, while the user was away, that the row and its 30px appear only when agentbox is installed and the project is on this computer; without agentbox, on Windows, for a remote project, or before a status answer, the picker is exactly the one the user sized (CDXC:AgentLauncher 2026-09-09 in new_thread_picker.rs), and discovery stays in Settings > Cloud Boxes and the sidebar's Select Agent menu.
//! SEE-ALSO: packages/gx-core/src/agentbox.rs (which locations are ready, which agents a box runs), apps/desktop/src/app/gx_store/agentbox.rs (the status read), packages/gx-core/src/sidebar_menu/run_in_box.rs (the sidebar launcher's twin).
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

/// The row's state: the offered boxes, the default location, and which chip is picked.
#[derive(Clone, Debug, Default)]
pub(crate) struct RunOnState {
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
            boxes,
            preferred,
            selected: None,
            touched: false,
            cloud_box: false,
        };
        state.apply_preferred();
        state
    }

    fn apply_preferred(&mut self) {
        let preferred = self.preferred.as_str();
        self.selected = self
            .boxes
            .locations()
            .iter()
            .find(|location| location.run_location == preferred)
            .map(|location| location.run_location.clone());
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
        self.boxes.shows_run_on() || self.cloud_box
    }

    /// The boxes after This computer and the Cloud Box chip.
    fn box_offset(&self) -> usize {
        1 + usize::from(self.cloud_box)
    }

    /// Chip index: 0 is This computer, then Cloud Box when offered, then the boxes in order.
    fn selected_index(&self) -> usize {
        match self.selected.as_deref() {
            Some(CLOUD_BOX_RUN_LOCATION) if self.cloud_box => 1,
            selected => selected
                .and_then(|selected| {
                    self.boxes
                        .locations()
                        .iter()
                        .position(|location| location.run_location == selected)
                })
                .map_or(0, |index| index + self.box_offset()),
        }
    }

    fn select_index(&mut self, index: usize) {
        self.touched = true;
        if self.cloud_box && index == 1 {
            self.selected = Some(CLOUD_BOX_RUN_LOCATION.to_string());
            return;
        }
        self.selected = index
            .checked_sub(self.box_offset())
            .filter(|_| index > 0)
            .and_then(|index| self.boxes.locations().get(index))
            .map(|location| location.run_location.clone());
    }

    fn chip_count(&self) -> usize {
        self.box_offset() + self.boxes.locations().len()
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
        (self.run_on.shown() && self.run_on.chip_count() > 1 && self.scope.is_none())
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
        let next = (self.run_on.selected_index() as isize + delta).rem_euclid(count) as usize;
        self.pick_run_location(next, window, cx);
    }

    /// A box has no account list and fewer rows, so the list is rebuilt around the agent that was
    /// highlighted.
    fn run_location_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        h_flex()
            .id(ElementId::Name(format!("new-thread-run-on-{index}").into()))
            .flex_shrink_0()
            .h(px(22.0))
            .items_center()
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
                    .hover(|chip| chip.bg(hsla(rgba_of(c.foreground, 0.06))))
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
                this.pick_run_location(index, window, cx);
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
        let mut chips: Vec<AnyElement> = vec![self.render_run_on_chip(
            0,
            ICON_THIS_COMPUTER,
            THIS_COMPUTER.to_string(),
            Some(ghostex_gx_core::THIS_COMPUTER_DESCRIPTION.to_string()),
            cx,
        )];
        if self.run_on.cloud_box {
            chips.push(self.render_run_on_chip(
                1,
                ICON_CLOUD_BOX,
                CLOUD_BOX_LABEL.to_string(),
                Some(CLOUD_BOX_TOOLTIP.to_string()),
                cx,
            ));
        }
        let box_offset = self.run_on.box_offset();
        for (offset, location) in self.run_on.boxes.locations().iter().enumerate() {
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
        let set_up = matches!(self.run_on.boxes, NewThreadPickerBoxes::NotSetUp);
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
