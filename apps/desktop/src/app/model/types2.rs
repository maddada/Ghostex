// C1 wave-3 extraction: a chunk (2/6, in original file order) of the remaining plain value-type enums/structs/small helper fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


pub(crate) fn command_pane_tab_pending_click_after_mouse_up_out(
    pending_click: Option<CommandPanePendingTabClick>,
    target: CommandPanePendingTabClick,
) -> Option<CommandPanePendingTabClick> {
    /*
    CDXC:GPUICommandTabSelection 2026-06-26-05:22:
    Native command-tab selection is a same-gesture mouse-up commit. A left mouse-up outside the tab must cancel the armed left-click token so a stale future mouse-up on that tab cannot select or enter command Focus mode.

    CDXC:GPUICommandTabSelection 2026-06-26-05:23:
    Mouse-up-out cancellation is exact to the command tab whose current gesture is ending. Leave any nonmatching pending token intact so only that tab's later same-tab mouse-up is prevented from selecting or focusing without a fresh mouse-down.
    */
    if pending_click == Some(target) {
        None
    } else {
        pending_click
    }
}


pub(crate) fn workspace_tab_left_mouse_up_selects(
    pending_click: Option<WorkspacePendingTabClick>,
    target: WorkspacePendingTabClick,
    workspace_tab_drag_active: bool,
) -> bool {
    /*
    CDXC:GPUIWorkspaceTabs 2026-06-26-06:34:
    Native Agents pane tabs arm selection on mouse-down and commit it on mouse-up only if the same tab still owns the gesture and no tab drag began. GPUI must keep that as runtime-only click state so dragging a tab does not first select/focus it or wake/materialize any placeholder.
    */
    pending_click == Some(target) && !workspace_tab_drag_active
}


pub(crate) fn workspace_tab_left_mouse_up_focuses(
    click_count: usize,
    pending_click: Option<WorkspacePendingTabClick>,
    target: WorkspacePendingTabClick,
    workspace_tab_drag_active: bool,
) -> bool {
    /*
    CDXC:GPUIWorkspaceTabs 2026-06-26-06:34:
    Native pane-tab double-click sends a Focus request from mouse-up after the same click/drag gates as normal selection. The model helper decides whether Focus mode can actually toggle, but stale or drag-active gestures must not route into Focus.
    */
    click_count >= 2
        && workspace_tab_left_mouse_up_selects(pending_click, target, workspace_tab_drag_active)
}


pub(crate) fn workspace_tab_pending_click_after_mouse_up_out(
    pending_click: Option<WorkspacePendingTabClick>,
    target: WorkspacePendingTabClick,
) -> Option<WorkspacePendingTabClick> {
    /*
    CDXC:GPUIWorkspaceTabs 2026-06-26-06:34:
    Mouse-up outside a native Agents pane tab cancels only that armed tab gesture. Preserve unrelated pending tab tokens for their own mouse-up path, and do not use broad root hit-test routing or synthetic input cleanup.
    */
    if pending_click == Some(target) {
        None
    } else {
        pending_click
    }
}


pub(crate) fn command_pane_tab_separator_visible(has_following_command_tab: bool) -> bool {
    /*
    CDXC:GPUICommandTabSeparators 2026-06-25-14:17:
    Native command tab buttons draw the trailing separator only when another command tab follows. The final tab in either expanded titlebar or collapsed strip must not get separator chrome.
    */
    has_following_command_tab
}


pub(crate) fn command_pane_sticky_active_tab_edge_for_scroll_handle(
    scroll_handle: &ScrollHandle,
    active_index: usize,
) -> Option<CommandPaneStickyActiveTabEdge> {
    /*
    CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
    Match native command overflow visibility from actual scroll geometry: hide the proxy unless the tab strip overflows and the active command tab has less than the native usable visible width.
    */
    command_pane_sticky_active_tab_edge(
        scroll_handle.bounds(),
        scroll_handle.bounds_for_item(active_index)?,
        scroll_handle.offset().x,
        scroll_handle.max_offset().x,
    )
}


pub(crate) fn command_pane_sticky_active_tab_edge(
    viewport_bounds: Bounds<Pixels>,
    active_tab_bounds: Bounds<Pixels>,
    scroll_offset_x: Pixels,
    max_scroll_x: Pixels,
) -> Option<CommandPaneStickyActiveTabEdge> {
    if !command_pane_tab_scroll_geometry_ready(viewport_bounds, active_tab_bounds, max_scroll_x) {
        return None;
    }

    if command_pane_active_tab_visible_width(viewport_bounds, active_tab_bounds, scroll_offset_x)
        >= command_pane_active_tab_minimum_usable_visible_width(active_tab_bounds)
    {
        return None;
    }

    if active_tab_bounds.left() + scroll_offset_x < viewport_bounds.left() {
        Some(CommandPaneStickyActiveTabEdge::Leading)
    } else {
        Some(CommandPaneStickyActiveTabEdge::Trailing)
    }
}


pub(crate) fn command_pane_tab_scroll_geometry_ready(
    viewport_bounds: Bounds<Pixels>,
    active_tab_bounds: Bounds<Pixels>,
    max_scroll_x: Pixels,
) -> bool {
    viewport_bounds.size.width >= px(COMMAND_PANE_STICKY_ACTIVE_TAB_BUTTON_SIZE)
        && active_tab_bounds.size.width > px(0.0)
        && max_scroll_x > px(0.0)
}


pub(crate) fn command_pane_active_tab_visible_width(
    viewport_bounds: Bounds<Pixels>,
    active_tab_bounds: Bounds<Pixels>,
    scroll_offset_x: Pixels,
) -> Pixels {
    let visible_left = if active_tab_bounds.left() + scroll_offset_x > viewport_bounds.left() {
        active_tab_bounds.left() + scroll_offset_x
    } else {
        viewport_bounds.left()
    };
    let visible_right = if active_tab_bounds.right() + scroll_offset_x < viewport_bounds.right() {
        active_tab_bounds.right() + scroll_offset_x
    } else {
        viewport_bounds.right()
    };
    if visible_right > visible_left {
        visible_right - visible_left
    } else {
        px(0.0)
    }
}


pub(crate) fn command_pane_active_tab_minimum_usable_visible_width(
    active_tab_bounds: Bounds<Pixels>,
) -> Pixels {
    if active_tab_bounds.size.width < px(COMMAND_PANE_ACTIVE_TAB_REVEAL_MINIMUM_VISIBLE_WIDTH) {
        active_tab_bounds.size.width
    } else {
        px(COMMAND_PANE_ACTIVE_TAB_REVEAL_MINIMUM_VISIBLE_WIDTH)
    }
}


pub(crate) fn command_pane_active_tab_reveal_scroll_offset_x(
    viewport_bounds: Bounds<Pixels>,
    active_tab_bounds: Bounds<Pixels>,
    current_offset_x: Pixels,
    max_scroll_x: Pixels,
) -> Option<Pixels> {
    /*
    CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
    Native command tab activation preserves scroll position when the selected tab is already usable, and otherwise reveals the active tab with a 12px margin instead of snapping the strip more than needed.
    */
    if !command_pane_tab_scroll_geometry_ready(viewport_bounds, active_tab_bounds, max_scroll_x) {
        return None;
    }

    if command_pane_active_tab_visible_width(viewport_bounds, active_tab_bounds, current_offset_x)
        >= command_pane_active_tab_minimum_usable_visible_width(active_tab_bounds)
    {
        return Some(current_offset_x);
    }

    let target_offset = if active_tab_bounds.left() + current_offset_x < viewport_bounds.left() {
        viewport_bounds.left() + px(COMMAND_PANE_ACTIVE_TAB_REVEAL_SCROLL_MARGIN)
            - active_tab_bounds.left()
    } else {
        viewport_bounds.right()
            - px(COMMAND_PANE_ACTIVE_TAB_REVEAL_SCROLL_MARGIN)
            - active_tab_bounds.right()
    };
    Some(command_pane_clamped_tab_scroll_offset_x(
        target_offset,
        max_scroll_x,
    ))
}


pub(crate) fn command_pane_clamped_tab_scroll_offset_x(
    target_offset_x: Pixels,
    max_scroll_x: Pixels,
) -> Pixels {
    let min_offset = px(0.0) - max_scroll_x;
    if target_offset_x < min_offset {
        min_offset
    } else if target_offset_x > px(0.0) {
        px(0.0)
    } else {
        target_offset_x
    }
}


pub(crate) fn command_pane_reveal_active_tab_with_native_margin(
    scroll_handle: &ScrollHandle,
    active_index: usize,
) {
    let Some(active_tab_bounds) = scroll_handle.bounds_for_item(active_index) else {
        scroll_handle.scroll_to_item(active_index);
        return;
    };
    let current_offset = scroll_handle.offset();
    let Some(next_x) = command_pane_active_tab_reveal_scroll_offset_x(
        scroll_handle.bounds(),
        active_tab_bounds,
        current_offset.x,
        scroll_handle.max_offset().x,
    ) else {
        scroll_handle.scroll_to_item(active_index);
        return;
    };
    if next_x != current_offset.x {
        scroll_handle.set_offset(gpui::point(next_x, current_offset.y));
    }
}


pub(crate) fn command_pane_tab_wheel_scroll_delta_x(
    delta: ScrollDelta,
    line_height: Pixels,
) -> Option<Pixels> {
    /*
    CDXC:GPUICommandTabScrolling 2026-06-25-13:45:
    Match native command tab wheel routing: horizontal gestures move tabs directly, precise vertical gestures are not remapped, and non-precision vertical wheel ticks are amplified before becoming horizontal tab movement.
    */
    let pixel_delta = delta.pixel_delta(line_height);
    let vertical_gesture = pixel_delta.y.abs() >= pixel_delta.x.abs();
    if !vertical_gesture {
        return Some(pixel_delta.x);
    }
    if delta.precise() {
        return None;
    }
    Some(command_pane_amplified_vertical_wheel_tab_delta(
        pixel_delta.y,
    ))
}


pub(crate) fn command_pane_amplified_vertical_wheel_tab_delta(delta_y: Pixels) -> Pixels {
    let scaled_delta = delta_y * COMMAND_PANE_VERTICAL_WHEEL_TAB_SCROLL_MULTIPLIER;
    if scaled_delta == px(0.0) {
        return scaled_delta;
    }
    let minimum_delta = px(COMMAND_PANE_MINIMUM_DISCRETE_VERTICAL_WHEEL_TAB_SCROLL_DELTA);
    if scaled_delta.abs() >= minimum_delta {
        scaled_delta
    } else if scaled_delta < px(0.0) {
        px(0.0) - minimum_delta
    } else {
        minimum_delta
    }
}


pub(crate) fn command_pane_handle_tab_strip_scroll_wheel(
    scroll_handle: &ScrollHandle,
    delta: ScrollDelta,
    line_height: Pixels,
) -> bool {
    let max_scroll_x = scroll_handle.max_offset().x;
    if max_scroll_x <= px(0.0) {
        return false;
    }
    let Some(delta_x) = command_pane_tab_wheel_scroll_delta_x(delta, line_height) else {
        return false;
    };
    if delta_x == px(0.0) {
        return false;
    }
    let current_offset = scroll_handle.offset();
    let next_x = command_pane_clamped_tab_scroll_offset_x(current_offset.x + delta_x, max_scroll_x);
    if next_x == current_offset.x {
        return false;
    }
    scroll_handle.set_offset(gpui::point(next_x, current_offset.y));
    true
}


pub(crate) fn command_pane_centered_active_tab_scroll_offset_x(
    viewport_bounds: Bounds<Pixels>,
    active_tab_bounds: Bounds<Pixels>,
    max_scroll_x: Pixels,
) -> Pixels {
    /*
    CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
    Clicking native Show Active Tab centers the real active tab when scroll bounds allow, then clamps at the strip ends. Keep GPUI on explicit scroll-offset math instead of the minimal `scroll_to_item` reveal path.
    */
    if viewport_bounds.size.width <= px(0.0)
        || active_tab_bounds.size.width <= px(0.0)
        || max_scroll_x <= px(0.0)
    {
        return px(0.0);
    }
    let centered_offset = viewport_bounds.center().x - active_tab_bounds.center().x;
    command_pane_clamped_tab_scroll_offset_x(centered_offset, max_scroll_x)
}


pub(crate) fn command_pane_center_active_tab_in_scroll_handle(
    scroll_handle: &ScrollHandle,
    active_index: usize,
) -> bool {
    let Some(active_tab_bounds) = scroll_handle.bounds_for_item(active_index) else {
        return false;
    };
    let current_offset = scroll_handle.offset();
    let next_x = command_pane_centered_active_tab_scroll_offset_x(
        scroll_handle.bounds(),
        active_tab_bounds,
        scroll_handle.max_offset().x,
    );
    if next_x == current_offset.x {
        return false;
    }
    scroll_handle.set_offset(gpui::point(next_x, current_offset.y));
    true
}


pub(crate) fn command_pane_sticky_active_tab_icon_path(edge: CommandPaneStickyActiveTabEdge) -> &'static str {
    match edge {
        CommandPaneStickyActiveTabEdge::Leading => COMMAND_ICON_CHEVRON_LEFT,
        CommandPaneStickyActiveTabEdge::Trailing => COMMAND_ICON_CHEVRON_RIGHT,
    }
}


pub(crate) fn command_pane_sticky_active_tab_tooltip() -> &'static str {
    "Show Active Tab"
}


pub(crate) fn command_pane_sticky_active_tab_trailing_inset(
    expanded_chrome: bool,
    tab_add_visible: bool,
) -> f32 {
    /*
    CDXC:GPUICommandTabOverflow 2026-06-25-18:51:
    Native Show Active Tab chrome overlays the tab viewport edge and does not consume tab-run layout width. Keep the trailing proxy before the inline New Terminal button and fixed command-panel actions so the overlay covers clipped tabs, not controls.
    */
    command_pane_fixed_panel_control_width(expanded_chrome)
        + if tab_add_visible {
            COMMAND_PANE_TAB_ADD_BUTTON_GAP + COMMAND_PANE_TAB_BAR_HEIGHT
        } else {
            0.0
        }
}


pub(crate) fn command_pane_empty_titlebar_double_click_creates_new_terminal(click_count: usize) -> bool {
    /*
    CDXC:GPUICommandTabDoubleClick 2026-06-25-13:50:
    Native command titlebars create New Terminal only for double-clicks on empty tab chrome. Single clicks and real tab/control hits must keep their normal focus, selection, drag, and action behavior.
    */
    click_count >= 2
}


pub(crate) fn command_pane_fixed_panel_control_count(expanded_chrome: bool) -> usize {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:26:
    Native command-panel fixed chrome has one visibility action in all states, plus Pin/Unpin only while visible. New Terminal stays inline with tabs, and visible panels do not add a second close/minimize button.
    */
    let mut count = 1;
    if command_pane_new_command_control_placement()
        == CommandPaneNewCommandControlPlacement::FixedActionCluster
    {
        count += 1;
    }
    if command_pane_panel_mode_controls_visible(expanded_chrome) {
        count += 1;
    }
    count
}


pub(crate) fn command_pane_fixed_panel_control_width(expanded_chrome: bool) -> f32 {
    command_pane_fixed_panel_control_count(expanded_chrome) as f32
        * COMMAND_PANE_CONTROL_BUTTON_SIZE
        + command_pane_control_trailing_padding(expanded_chrome)
}


pub(crate) fn command_pane_inline_tab_add_visible_for_chrome_width(
    chrome_width: f32,
    expanded_chrome: bool,
) -> bool {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-18:46:
    Native computes command tab-add visibility from the tab area left after fixed panel actions. Hide GPUI's inline plus under the same threshold so narrow command groups keep usable tab/double-click chrome instead of pinning a New Terminal button over it.
    */
    let tab_area_width =
        (chrome_width - command_pane_fixed_panel_control_width(expanded_chrome)).max(0.0);
    tab_area_width
        >= COMMAND_PANE_MINIMUM_VISIBLE_TAB_VIEWPORT_WIDTH_WITH_DOUBLE_CLICK_TARGET
            + COMMAND_PANE_TAB_ADD_BUTTON_GAP
            + COMMAND_PANE_TAB_BAR_HEIGHT
}


pub(crate) fn command_pane_panel_pin_label(mode: CommandPaneMode) -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:19:
    macOS command-panel chrome labels the mode toggle as Pin/Unpin Commands Panel, not Float/Pin Command Pane. Keep the label tied to the native action vocabulary while preserving the existing pinned/floating state mutation.
    */
    match mode {
        CommandPaneMode::Pinned => "Unpin Commands Panel",
        CommandPaneMode::Floating | CommandPaneMode::Collapsed => "Pin Commands Panel",
    }
}


pub(crate) fn command_pane_panel_minimize_label() -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:19:
    Native `closeCommandsPanel` hides/minimizes the command panel while preserving command sessions. Surface that action as Minimize Commands Panel so GPUI copy matches macOS and does not imply terminal/session deletion.
    */
    "Minimize Commands Panel"
}


pub(crate) fn command_pane_panel_expand_menu_label() -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:23:
    Native collapsed command-panel chrome exposes `.expandCommandsPanel` as Expand Commands Panel. Use that plural panel wording in GPUI collapsed-strip menus and controls instead of the older command-pane phrase.
    */
    "Expand Commands Panel"
}


// CPRAILDBG: temporary diagnostic logging for the command-pane resize-rail
// drag investigation. Remove before handoff.
pub(crate) fn cpraildbg(message: &str) {
    if !shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
        || !support_logs::scenario_id_enabled("native.pane.tabs")
    {
        return;
    }
    use std::io::Write as _;
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/cpraildbg.log")
    else {
        return;
    };
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let _ = writeln!(file, "[{millis}] {message}");
}


/*
CDXC:GPUICommandPaneSide 2026-08-16:
One drag state serves both docks: the bottom rail tracks pointer Y against the
panel height, the right divider tracks pointer X against the panel width. The
side is captured at mouse-down so a Settings save mid-drag cannot re-interpret
the stored start position on the other axis.
*/
#[derive(Clone, Copy)]
pub(crate) struct CommandPaneResizeDragState {
    pub(crate) side: GpuiCommandPaneSide,
    pub(crate) start_position: f32,
    pub(crate) start_extent: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct SplitResizeMetrics {
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct WorkspaceSplitResizeDragState {
    pub(crate) split_id: WorkspaceSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) start_position: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct CommandPaneSplitResizeDragState {
    pub(crate) split_id: CommandPaneSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) start_position: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct BrowserSplitResizeDragState {
    pub(crate) split_id: BrowserSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) start_position: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorCompanionResizeDragState {
    pub(crate) start_x: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorCompanionSplitResizeDragState {
    pub(crate) start_y: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


/*
CDXC:GPUICommandTabStatus 2026-06-27-05:07:
Command-pane tab status indicators now have live Action parity plus safe restored metadata: Action command tabs own run ids, session-state file stamping, status-file polling, run-start Working, idle completion, completion feedback, and exit cleanup, while non-Action/restored tabs remain enum/boolean status only. Status must not be inferred from shell titles, output, paths, command text, env, logs, or persisted shell JSON. Persistence keeps only the bounded Action selector needed to reclaim the same tab after restart; run ids, status-file paths, command text, stdout/stderr, terminal content, countdown labels, paths, tokens, and private titles remain runtime-only.

CDXC:GPUICommandPaneTabs 2026-06-25-11:24:
The command-pane model still keeps an idle enum for persistence and sidebar indicators, but command tab chrome should match macOS by rendering indicators only for working, attention, and delayed-send states. Idle command tabs show no status indicator.

CDXC:GPUICommandTabStatus 2026-06-25-13:18:
Command-tab status chrome is a trailing native slot, not a leading title prefix: the tab title reserves trailing space for active status, the indicator hides while hover close chrome is visible, and Delayed Send uses clock glyph chrome instead of a rounded dot.

CDXC:GPUICommandTabSleep 2026-06-25-14:27:
Command-pane Sleep is a renderer lifecycle flag, not tab deletion. Persist only the sleeping boolean with safe command metadata so tabs remain in their command group while mounted command terminal body slots are withheld until explicit wake.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandTerminalActivity {
    Idle,
    Working,
    Attention,
}


impl Default for CommandTerminalActivity {
    fn default() -> Self {
        Self::Idle
    }
}


impl CommandTerminalActivity {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "working" => Some(Self::Working),
            "attention" => Some(Self::Attention),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandTerminalTabStatus {
    Idle,
    Working,
    Attention,
    DelayedSend,
}


impl CommandTerminalTabStatus {
    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
            Self::DelayedSend => "delayed-send",
        }
    }
}


pub(crate) fn command_terminal_tab_status(
    activity: CommandTerminalActivity,
    delayed_send_active: bool,
) -> CommandTerminalTabStatus {
    if delayed_send_active {
        CommandTerminalTabStatus::DelayedSend
    } else {
        match activity {
            CommandTerminalActivity::Idle => CommandTerminalTabStatus::Idle,
            CommandTerminalActivity::Working => CommandTerminalTabStatus::Working,
            CommandTerminalActivity::Attention => CommandTerminalTabStatus::Attention,
        }
    }
}


pub(crate) struct CommandTerminalSession {
    pub(crate) id: CommandSessionId,
    pub(crate) title: String,
    pub(crate) gxserver_session_key: Option<GpuiLocalWorkspaceSessionKey>,
    /// Runtime-only zmx persistence session name from gxserver attach
    /// metadata; see `TerminalSession::zmx_session_name`.
    pub(crate) zmx_session_name: Option<String>,
    pub(crate) activity: CommandTerminalActivity,
    pub(crate) delayed_send_active: bool,
    pub(crate) delayed_send_timer_owned: bool,
    pub(crate) close_after_done_armed: bool,
    pub(crate) is_sleeping: bool,
    pub(crate) action_close_terminal_on_exit: bool,
    pub(crate) action_command_id: Option<String>,
    pub(crate) action_play_completion_sound: bool,
    pub(crate) action_run_id: Option<String>,
    pub(crate) action_status_file_path: Option<PathBuf>,
}


impl CommandTerminalSession {
    pub(crate) fn placeholder(id: CommandSessionId, title: String) -> Self {
        Self {
            id,
            title,
            gxserver_session_key: None,
            zmx_session_name: None,
            activity: CommandTerminalActivity::Idle,
            delayed_send_active: false,
            delayed_send_timer_owned: false,
            close_after_done_armed: false,
            is_sleeping: false,
            action_close_terminal_on_exit: false,
            action_command_id: None,
            action_play_completion_sound: false,
            action_run_id: None,
            action_status_file_path: None,
        }
    }

    pub(crate) fn with_activity(mut self, activity: CommandTerminalActivity) -> Self {
        self.activity = activity;
        self
    }

    pub(crate) fn with_delayed_send_active(mut self, delayed_send_active: bool) -> Self {
        self.delayed_send_active = delayed_send_active;
        self.delayed_send_timer_owned = false;
        self
    }

    pub(crate) fn set_delayed_send_active(&mut self, delayed_send_active: bool, timer_owned: bool) {
        self.delayed_send_active = delayed_send_active;
        self.delayed_send_timer_owned = delayed_send_active && timer_owned;
    }

    pub(crate) fn with_sleeping(mut self, is_sleeping: bool) -> Self {
        self.is_sleeping = is_sleeping;
        self
    }

    pub(crate) fn with_close_after_done_armed(mut self, close_after_done_armed: bool) -> Self {
        self.close_after_done_armed = close_after_done_armed;
        self
    }

    pub(crate) fn with_gxserver_session_key(mut self, key: Option<GpuiLocalWorkspaceSessionKey>) -> Self {
        self.gxserver_session_key = key;
        self
    }

    pub(crate) fn with_action_command_id(mut self, action_command_id: String) -> Self {
        self.action_command_id = Some(action_command_id);
        self
    }

    pub(crate) fn tab_status(&self) -> CommandTerminalTabStatus {
        /*
        CDXC:GPUICommandDelayedSend 2026-06-25-18:07:
        Native command-pane tab sync publishes Delayed Send countdown labels before it skips sleeping sessions for working/attention activity. Preserve the timer clock on sleeping command tabs, while sleeping tabs without a timer still render idle parked chrome.
        */
        if self.delayed_send_active {
            return CommandTerminalTabStatus::DelayedSend;
        }
        if self.is_sleeping {
            return CommandTerminalTabStatus::Idle;
        }
        command_terminal_tab_status(self.activity, false)
    }

    pub(crate) fn sidebar_hud_indicator_status(&self) -> &'static str {
        /*
        CDXC:GPUICommandSessionHud 2026-06-27-06:30:
        Sidebar command-session HUD indicators mirror native terminal lifecycle, not Action completion feedback. GPUI has no persisted lifecycle error state for live local command tabs, so awake command tabs export running and sleeping tabs export idle without deriving error from Attention or exposing terminal-private data.
        */
        if self.is_sleeping { "idle" } else { "running" }
    }
}


#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuiCommandDelayedSendTimer {
    pub(crate) deadline_at: SystemTime,
    pub(crate) generation: u64,
}


#[derive(Clone, Debug)]
pub(crate) struct GpuiAgentsSendWhenStoppedWatcher {
    pub(crate) generation: u64,
    pub(crate) non_working_since: Option<Instant>,
    pub(crate) scope: GpuiAgentsSendWhenStoppedScope,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAgentsSendWhenStoppedScope {
    Session,
    Project(String),
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiAgentsDelayedSendTarget {
    GpuiEngine {
        session_id: TerminalSessionId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    },
    AgentsNative(AgentsTerminalBodyMountSlotId),
    ProjectEditorCompanionNative(ProjectEditorCompanionTerminalBodyMountSlotId),
    #[cfg(target_os = "macos")]
    AgentsParkedNative(AgentsTerminalRuntimeSessionId),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandDelayedSendRestoreTimer {
    pub(crate) session_id: CommandSessionId,
    pub(crate) remaining_ms: u64,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiAgentsDelayedSendRestoreIntent {
    pub(crate) session_id: TerminalSessionId,
    pub(crate) trigger: GpuiAgentsDelayedSendRestoreTrigger,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAgentsDelayedSendRestoreTrigger {
    Timer { remaining_ms: u64 },
    WhenAgentFinishesWorking,
    WhenAllAgentsFinishWorking { project_id: String },
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandStartupActivityRestoreIntent {
    pub(crate) session_id: CommandSessionId,
    pub(crate) activity: CommandTerminalActivity,
}


impl GpuiCommandDelayedSendTimer {
    pub(crate) fn remaining_ms(self, now: SystemTime) -> u64 {
        self.deadline_at
            .duration_since(now)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }
}


#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuiCommandCloseAfterDoneTimer {
    pub(crate) deadline_at: SystemTime,
    pub(crate) generation: u64,
}


impl GpuiCommandCloseAfterDoneTimer {
    pub(crate) fn remaining_ms(self, now: SystemTime) -> u64 {
        self.deadline_at
            .duration_since(now)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }
}


pub(crate) fn gpui_command_delayed_send_duration_from_millis(delay_ms: u64) -> Option<Duration> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-15:11:
    The shared Delayed Send modal posts only a session id and delay duration. GPUI must keep the same whole-minute, one-minute-minimum, 24-day JavaScript-timer-compatible bounds as native before arming a command-pane timer.
    */
    if !(COMMAND_PANE_DELAYED_SEND_MIN_DELAY_MS..=COMMAND_PANE_DELAYED_SEND_MAX_DELAY_MS)
        .contains(&delay_ms)
        || delay_ms % COMMAND_PANE_DELAYED_SEND_MIN_DELAY_MS != 0
    {
        return None;
    }
    Some(Duration::from_millis(delay_ms))
}


pub(crate) fn gpui_command_delayed_send_countdown_label(remaining_ms: u64) -> String {
    let total_seconds = remaining_ms.saturating_add(999) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}


pub(crate) fn gpui_agents_send_when_stopped_remaining_label(
    watcher: &GpuiAgentsSendWhenStoppedWatcher,
    is_working: bool,
    now: Instant,
) -> String {
    if is_working {
        return match &watcher.scope {
            GpuiAgentsSendWhenStoppedScope::Session => "Waiting for agent".to_string(),
            GpuiAgentsSendWhenStoppedScope::Project(_) => "Waiting for agents".to_string(),
        };
    }
    let remaining = watcher
        .non_working_since
        .map(|started_at| {
            GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION
                .saturating_sub(now.saturating_duration_since(started_at))
        })
        .unwrap_or(GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION);
    gpui_command_delayed_send_countdown_label(remaining.as_millis().min(u128::from(u64::MAX)) as u64)
}


pub(crate) fn gpui_command_delayed_send_body_badge_label(
    timer: Option<GpuiCommandDelayedSendTimer>,
    now: SystemTime,
) -> Option<String> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-15:42:
    The command body badge projects only a countdown string from a runtime timer. It must not inspect command text, titles, terminal content, paths, shell-state JSON, or persisted delayed-send placeholders.
    */
    timer.map(|timer| gpui_command_delayed_send_countdown_label(timer.remaining_ms(now)))
}


pub(crate) fn gpui_command_delayed_send_duration_label(duration: Duration) -> String {
    let total_seconds = duration.as_secs().max(1);
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}


pub(crate) fn gpui_command_delayed_send_restore_remaining_ms(value: &serde_json::Value) -> Option<u64> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-16:41:
    Restored GPUI command Delayed Send timers should match macOS by resuming from a saved remaining-duration checkpoint, not by spending countdown time while the app is closed. Accept only bounded numeric milliseconds and keep command text, titles, terminal content, paths, runtime ids, and stdout/stderr out of the restart contract.
    */
    let remaining_ms = value.as_u64()?;
    (remaining_ms > 0 && remaining_ms <= COMMAND_PANE_DELAYED_SEND_MAX_DELAY_MS)
        .then_some(remaining_ms)
}


pub(crate) fn gpui_command_delayed_send_restore_duration(remaining_ms: u64) -> Duration {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-16:41:
    macOS gives restored Delayed Send timers a 2s fire grace even when the saved checkpoint is nearly expired. GPUI should use the same startup grace so a restored command tab has time to mount before the pending Return key can fire.
    */
    Duration::from_millis(remaining_ms.max(COMMAND_PANE_DELAYED_SEND_RESTORE_FIRE_GRACE_MS))
}


pub(crate) fn gpui_command_session_id_from_modal_value(value: &serde_json::Value) -> Option<CommandSessionId> {
    gpui_command_session_id_from_external_id(value.as_str()?)
}


pub(crate) fn gpui_command_session_rename_title_from_modal_value(value: &serde_json::Value) -> Option<String> {
    /*
    CDXC:GPUICommandPaneRename 2026-06-25-16:33:
    The shared Rename Session modal already applies the normal sidebar rename normalization before posting. Revalidate the GPUI boundary by accepting only non-empty, non-control text and collapsing whitespace so direct bridge messages cannot store multiline terminal content as command-tab chrome.
    */
    let title = value
        .as_str()?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()
        && title.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !title.chars().any(char::is_control))
    .then_some(title)
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneAppModalReturnFocusTarget {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


pub(crate) fn command_pane_apply_app_modal_return_focus_target(
    command_pane: &mut CommandPaneModel,
    target: CommandPaneAppModalReturnFocusTarget,
) -> bool {
    /*
    CDXC:GPUIAppModalReturnFocus 2026-06-25-22:13:
    Modal dismissal may restore only the exact command group/session captured at open time. Reject stale sessions and mismatched group ids instead of falling back to the focused group or first visible group, because fallback would send typing to the wrong command terminal after a Rename or Delayed Send dialog closes.
    */
    if command_pane.session(target.session_id).is_none()
        || command_pane_group_for_session(command_pane, target.session_id) != Some(target.group_id)
    {
        return false;
    }
    command_pane.select_session_in_group(target.group_id, target.session_id)
}


pub(crate) fn restore_command_pane_app_modal_return_focus(
    command_pane: &mut CommandPaneModel,
    target: CommandPaneAppModalReturnFocusTarget,
) -> bool {
    /*
    CDXC:GPUICommandModalFocus 2026-06-25-22:12:
    Native child app modals restore keyboard focus to the command terminal captured at open time. GPUI keeps only process-local numeric command group/session ids for that internal handoff, selects that exact tab on close, and consumes stale targets without falling back to another command group.
    */
    command_pane_apply_app_modal_return_focus_target(command_pane, target)
}


pub(crate) fn command_pane_mounted_slot_for_session(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<CommandTerminalBodyMountSlotId> {
    command_pane
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .find(|slot_id| slot_id.session_id == session_id)
}


pub(crate) fn command_pane_group_for_session(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<CommandPaneGroupId> {
    command_pane
        .flat_tab_ids()
        .into_iter()
        .find_map(|(group_id, candidate)| (candidate == session_id).then_some(group_id))
}


pub(crate) fn gpui_app_modal_command_return_focus_target_for_session(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<CommandPaneAppModalReturnFocusTarget> {
    let group_id = command_pane_group_for_session(command_pane, session_id)?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some(CommandPaneAppModalReturnFocusTarget {
            group_id,
            session_id,
        })
}


pub(crate) fn gpui_app_modal_sidebar_command_live_command_tab(
    command_pane: &CommandPaneModel,
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUIAppModalCommandBridge 2026-06-25-22:54:
    Direct command-session sidebar commands such as toggleCloseAfterDone must carry an explicit external `G{u64}` sessionId that still belongs to a live command tab group. Reject raw numeric JSON ids, legacy numeric strings, lowercase/malformed ids, stale tabs, and orphan stored sessions without falling back to the focused command group.

    CDXC:GPUIAppModalCommandBridge 2026-06-25-23:04:
    scheduleDelayedSend and cancelDelayedSend submissions use this same live-tab resolver before arming or clearing a timer. Stale stored command-session rows, including orphan rows that still carry timer-owned Delayed Send flags, must not arm/cancel another timer or fall back to the focused command group.
    */
    let session_id = command
        .get("sessionId")
        .and_then(gpui_command_session_id_from_modal_value)?;
    let group_id = command_pane_group_for_session(command_pane, session_id)?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some((group_id, session_id))
}


pub(crate) fn gpui_app_modal_requires_live_requested_command_session(modal: GpuiAppModalKind) -> bool {
    matches!(
        modal,
        GpuiAppModalKind::RenameSession | GpuiAppModalKind::DelayedSend
    )
}


pub(crate) fn gpui_app_modal_requested_live_command_session_id(
    modal: GpuiAppModalKind,
    open_message: &serde_json::Value,
    command_pane: &CommandPaneModel,
) -> Option<CommandSessionId> {
    /*
    CDXC:GPUIAppModalCommandBridge 2026-06-25-22:24:
    Rename Session and Delayed Send app-modal opens are command-tab requests. They must carry an external `G{u64}` sessionId for a live CommandPaneModel session; stale, malformed, legacy numeric, or missing ids no-op instead of opening a modal for another command tab or inventing a fallback target.
    */
    if !gpui_app_modal_requires_live_requested_command_session(modal) {
        return None;
    }
    let session_id = open_message
        .get("sessionId")
        .and_then(gpui_command_session_id_from_modal_value)?;
    gpui_app_modal_command_return_focus_target_for_session(command_pane, session_id)
        .map(|target| target.session_id)
}


pub(crate) fn gpui_app_modal_has_required_live_command_session(
    modal: GpuiAppModalKind,
    open_message: &serde_json::Value,
    command_pane: &CommandPaneModel,
) -> bool {
    !gpui_app_modal_requires_live_requested_command_session(modal)
        || gpui_app_modal_requested_live_command_session_id(modal, open_message, command_pane)
            .is_some()
}


pub(crate) fn gpui_app_modal_command_return_focus_target(
    modal: GpuiAppModalKind,
    open_message: &serde_json::Value,
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<CommandPaneAppModalReturnFocusTarget> {
    /*
    CDXC:GPUIAppModalReturnFocus 2026-06-25-22:13:
    App-modal dismissal should remember only a command-pane terminal target. Rename Session and Delayed Send parse the requested external `G{u64}` bridge id into an internal numeric command session id and reject stale or malformed ids; unrelated app modals may return to the currently shell-focused expanded command group. Keep the target runtime-only and bounded to command group/session ids, with no titles, paths, command text, terminal output, URLs, or modal payload details.
    */
    if matches!(
        modal,
        GpuiAppModalKind::RenameSession | GpuiAppModalKind::DelayedSend
    ) {
        let session_id =
            gpui_app_modal_requested_live_command_session_id(modal, open_message, command_pane)?;
        return gpui_app_modal_command_return_focus_target_for_session(command_pane, session_id);
    }

    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (_group_id, session_id) = command_pane.focused_group_active_session_id()?;
    gpui_app_modal_command_return_focus_target_for_session(command_pane, session_id)
}


pub(crate) fn gpui_app_modal_command_return_focus_target_for_active_modal(
    existing: Option<CommandPaneAppModalReturnFocusTarget>,
    incoming: Option<CommandPaneAppModalReturnFocusTarget>,
) -> Option<CommandPaneAppModalReturnFocusTarget> {
    /*
    CDXC:GPUIAppModalReturnFocus 2026-06-25-22:32:
    Native `rememberAppModalReturnFocusTarget` returns early after the first command terminal is captured for an active app-modal window. GPUI existing-host modal updates must preserve that first target so duplicate or nested modal opens do not retarget dismissal focus away from the terminal that opened the original modal.
    */
    existing.or(incoming)
}


pub(crate) fn gpui_command_close_after_done_session_marked_done(session: &CommandTerminalSession) -> bool {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-15:24:
    GPUI command Close After Done mirrors native's terminal-scoped watcher for command-pane Actions: Attention is done/error, and action-owned tabs become done once their live run is no longer Working. Generic idle command placeholders without an Action identity are not treated as done.
    */
    if session.is_sleeping {
        return false;
    }
    if session.activity == CommandTerminalActivity::Attention {
        return true;
    }
    session.action_command_id.is_some()
        && session.action_run_id.is_none()
        && session.activity != CommandTerminalActivity::Working
}


pub(crate) fn gpui_command_close_after_done_runtime_timer_member(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<(CommandPaneGroupId, &CommandTerminalSession)> {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-22:39:
    Runtime Close After Done countdown membership is command-tab membership, not stored-session presence. Resolve the live group with `command_pane_group_for_session`; orphan stored sessions may keep their armed boolean but cannot refresh, prune, or fire runtime timers.
    */
    let group_id = command_pane_group_for_session(command_pane, session_id)?;
    command_pane
        .session(session_id)
        .map(|session| (group_id, session))
}


pub(crate) fn gpui_command_close_after_done_timer_should_count_down(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> bool {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-27-01:37:
    A Close After Done runtime deadline is valid only while the armed command tab is live, awake, and still Done. Sleeping or newly Working tabs keep the armed intent but must stop spending the three-minute countdown until a later refresh finds them awake and Done again.
    */
    gpui_command_close_after_done_runtime_timer_member(command_pane, session_id).is_some_and(
        |(_group_id, session)| {
            session.close_after_done_armed
                && gpui_command_close_after_done_session_marked_done(session)
        },
    )
}


pub(crate) fn gpui_command_close_after_done_stale_runtime_timer_session_ids(
    command_pane: &CommandPaneModel,
    close_after_done_timers: &HashMap<CommandSessionId, GpuiCommandCloseAfterDoneTimer>,
) -> Vec<CommandSessionId> {
    close_after_done_timers
        .keys()
        .copied()
        .filter(|session_id| {
            !gpui_command_close_after_done_timer_should_count_down(command_pane, *session_id)
        })
        .collect()
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandCloseAfterDoneToggleTarget {
    ArmLiveSession,
    ClearStoredSession,
    NoOp,
}


pub(crate) fn gpui_command_close_after_done_toggle_target(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> GpuiCommandCloseAfterDoneToggleTarget {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-22:54:
    Direct Close After Done toggles may arm only sessions that are still attached to a command tab group. Already-armed stored sessions, including stale orphans, may still clear so persisted booleans can be cleaned without reattaching, falling back, or arming an unrelated tab.
    */
    let Some(session) = command_pane.session(session_id) else {
        return GpuiCommandCloseAfterDoneToggleTarget::NoOp;
    };
    if session.close_after_done_armed {
        return GpuiCommandCloseAfterDoneToggleTarget::ClearStoredSession;
    }
    if command_pane_group_for_session(command_pane, session_id).is_some() {
        GpuiCommandCloseAfterDoneToggleTarget::ArmLiveSession
    } else {
        GpuiCommandCloseAfterDoneToggleTarget::NoOp
    }
}


pub(crate) fn command_session_is_reusable_for_action(session: &CommandTerminalSession) -> bool {
    /*
    CDXC:GPUICommandPaneActions 2026-08-08:
    A sleeping command tab is still the existing pane owned by its Action. An
    Action launch already re-selects that exact tab, marks it awake, and sends
    startup text through the existing gxserver attach path, so excluding parked
    tabs here discarded the reusable owner and allocated a duplicate pane.
    Working tabs remain non-reusable so concurrent runs keep separate owners.
    */
    session.activity == CommandTerminalActivity::Idle
}


pub(crate) fn focused_command_pane_close_after_done_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-16:52:
    Native routes Close After Done for command terminals by focused session id, not by mounted terminal surface. GPUI should resolve the expanded shell-focused command tab even when it is sleeping so users can arm or cancel the safe persisted intent without waking or mounting the terminal.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some((group_id, session_id))
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneActionSessionSelectionKind {
    Created,
    Reused,
    ReusedActive,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneActionSessionSelection {
    pub(crate) kind: CommandPaneActionSessionSelectionKind,
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneActionRunCompletedTab {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarCommandRunState {
    Error,
    Running,
    Success,
}


impl GpuiSidebarCommandRunState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            GpuiSidebarCommandRunState::Error => "error",
            GpuiSidebarCommandRunState::Running => "running",
            GpuiSidebarCommandRunState::Success => "success",
        }
    }
}


#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSidebarCommandRunFeedbackState {
    pub(crate) active_run_ids: Vec<String>,
    pub(crate) status: Option<GpuiSidebarCommandRunState>,
}


impl GpuiSidebarCommandRunFeedbackState {
    pub(crate) fn apply_run_state(&mut self, run_id: &str, state: GpuiSidebarCommandRunState) {
        match state {
            GpuiSidebarCommandRunState::Running => {
                if !self
                    .active_run_ids
                    .iter()
                    .any(|active_run_id| active_run_id == run_id)
                {
                    self.active_run_ids.push(run_id.to_string());
                }
                self.status = Some(GpuiSidebarCommandRunState::Running);
            }
            GpuiSidebarCommandRunState::Success | GpuiSidebarCommandRunState::Error => {
                self.active_run_ids
                    .retain(|active_run_id| active_run_id != run_id);
                self.status = Some(if self.active_run_ids.is_empty() {
                    state
                } else {
                    GpuiSidebarCommandRunState::Running
                });
            }
        }
    }

    pub(crate) fn titlebar_action_run_mode_for_click(
        &self,
        action: &GpuiTitlebarAction,
    ) -> GpuiTitlebarActionRunMode {
        /*
        CDXC:GPUITitlebarActions 2026-06-27-09:26:
        GPUI Rust titlebar Actions do not run through the React command-palette click handler, so they need the same sanitized feedback rule locally: a close-on-exit terminal Action reruns in Debug only after its previous run ended in error and no newer run is active. Store only run ids plus coarse state; never derive Debug from command text, URLs, paths, env, output, status files, terminal content, or logs.
        */
        if action.run_mode != GpuiTitlebarActionRunMode::Default {
            return action.run_mode;
        }
        if action.action_type == GpuiTitlebarActionType::Terminal
            && action.close_terminal_on_exit
            && self.status == Some(GpuiSidebarCommandRunState::Error)
            && self.active_run_ids.is_empty()
        {
            GpuiTitlebarActionRunMode::Debug
        } else {
            GpuiTitlebarActionRunMode::Default
        }
    }
}


pub(crate) fn gpui_titlebar_action_run_mode_for_click(
    action: &GpuiTitlebarAction,
    feedback: Option<&GpuiSidebarCommandRunFeedbackState>,
) -> GpuiTitlebarActionRunMode {
    if action.run_mode != GpuiTitlebarActionRunMode::Default {
        return action.run_mode;
    }
    feedback
        .map(|feedback| feedback.titlebar_action_run_mode_for_click(action))
        .unwrap_or(GpuiTitlebarActionRunMode::Default)
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneActionRunCompletion {
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command_id: String,
    pub(crate) completed_tab: Option<CommandPaneActionRunCompletedTab>,
    pub(crate) exit_code: i32,
    pub(crate) play_completion_sound: bool,
    pub(crate) run_id: String,
}


impl CommandPaneActionRunCompletion {
    pub(crate) fn run_state(&self) -> GpuiSidebarCommandRunState {
        if self.exit_code == 0 {
            GpuiSidebarCommandRunState::Success
        } else {
            GpuiSidebarCommandRunState::Error
        }
    }

    pub(crate) fn should_play_completion_sound(&self) -> bool {
        self.exit_code != 0 || self.play_completion_sound
    }
}


pub(crate) fn gpui_command_pane_action_runtime_close_terminal_on_exit(
    _requested_close_terminal_on_exit: bool,
) -> bool {
    /*
    CDXC:GPUICommandPaneActions 2026-06-26-04:59:
    Native `runNativeSidebarCommand` forces command-pane Action close-on-exit off so each Action keeps a reusable command tab after completion. GPUI may still parse and save legacy `closeTerminalOnExit` fields, but every command-pane Action runtime boundary must normalize the requested value to false and must not close the completed Action tab.
    */
    false
}


pub(crate) fn gpui_command_pane_default_action_should_focus_command_pane() -> bool {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-01:45:
    Default terminal Actions mirror native `runNativeSidebarCommand` with `createCommandTerminal(..., { focusAfterCreate: false })`: select/open the Action command tab and publish running state without moving shell typing focus into CommandPane. Do not add fallback focus behavior here; focus remains explicit through command-pane controls, tab clicks, direct context actions, F12/open-panel routes, command hotkeys, and Debug Actions.

    CDXC:GPUICommandPaneActions 2026-06-27-01:45:
    The default Action focus policy is privacy-neutral and state-only: it must not inspect command text, cwd, project paths, terminal output, URLs, titles, env vars, or user content to decide focus. Native parity is unconditional for default terminal Actions.
    */
    false
}


#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandPaneActionRunRefresh {
    pub(crate) changed: bool,
    pub(crate) completions: Vec<CommandPaneActionRunCompletion>,
}


#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandTerminalProcessExitCleanup {
    pub(crate) changed: bool,
    pub(crate) completions: Vec<CommandPaneActionRunCompletion>,
}


/*
CDXC:GPUICommandPaneTabs 2026-06-22-17:20:
Command-pane tab chrome is focus-invariant: command group focus and shell focus may drive keyboard ownership and group borders, but per-tab brightness derives only from semantic command status and active membership inside the command tab group. CommandPaneModel.focused_group and shell focus are intentionally excluded.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandTabChromeSignature {
    pub(crate) tab_status: CommandTerminalTabStatus,
    pub(crate) active_in_tab_group: bool,
}


pub(crate) fn command_tab_chrome_signature(
    tab_group: &CommandPaneTabGroup,
    session_id: CommandSessionId,
    tab_status: CommandTerminalTabStatus,
) -> CommandTabChromeSignature {
    CommandTabChromeSignature {
        tab_status,
        active_in_tab_group: tab_group.active_session_id() == Some(session_id),
    }
}


pub(crate) fn command_pane_group_has_first_responder_border(
    shell_focus: ShellFocusTarget,
    focused_group: CommandPaneGroupId,
    group_id: CommandPaneGroupId,
) -> bool {
    /*
    CDXC:GPUICommandPaneFocus 2026-06-27-03:03:
    `native-command-panel-focus-source.test.ts` covers the AppKit source path that repaints command-pane borders after programmatic first-responder focus. GPUI command borders must treat shell command focus plus the focused command group as the responder chrome source; selected-group state alone is not focused chrome.
    */
    shell_focus == ShellFocusTarget::CommandPane && focused_group == group_id
}


pub(crate) fn command_pane_group_border_color(
    mode: CommandPaneMode,
    shell_focus: ShellFocusTarget,
    focused_group: CommandPaneGroupId,
    group_id: CommandPaneGroupId,
) -> Hsla {
    /*
    CDXC:GPUICommandPaneFocus 2026-06-25-17:57:
    Native command-pane focus borders are first-responder chrome, not only selected command-group state. Show the focused border only when the command pane owns shell focus and the group is the command model's focused group, so returning focus to Agents, Browser, or project-editor surfaces clears command focus chrome without changing command tab selection.

    CDXC:GPUICommandPaneFocus 2026-06-25-18:02:
    Native pinned command panels hide inactive command borders while floating command panels keep the inactive command outline. Keep the GPUI border frame stable but make pinned inactive command groups transparent so the visual state follows AppKit without resizing panes.
    */
    if command_pane_group_has_first_responder_border(shell_focus, focused_group, group_id) {
        command_pane_focused_border_color()
    } else if mode == CommandPaneMode::Pinned {
        command_pane_hidden_border_color()
    } else {
        command_pane_border_color()
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneGroupBorderWidth {
    Focused,
    Inactive,
}


impl CommandPaneGroupBorderWidth {
    pub(crate) fn px(self) -> u8 {
        match self {
            CommandPaneGroupBorderWidth::Focused => COMMAND_PANE_GROUP_FOCUSED_BORDER_WIDTH,
            CommandPaneGroupBorderWidth::Inactive => COMMAND_PANE_GROUP_INACTIVE_BORDER_WIDTH,
        }
    }
}


pub(crate) fn command_pane_group_border_width(
    shell_focus: ShellFocusTarget,
    focused_group: CommandPaneGroupId,
    group_id: CommandPaneGroupId,
) -> CommandPaneGroupBorderWidth {
    /*
    CDXC:GPUICommandPaneFocus 2026-06-27-04:35:
    Native command-pane chrome uses a 1px first-responder border and a 2px inactive command border. GPUI must keep the inactive width even when pinned inactive groups use transparent color, so hidden inactive command groups remain layout-stable while focused chrome stays thinner than attention/command borders.
    */
    if command_pane_group_has_first_responder_border(shell_focus, focused_group, group_id) {
        CommandPaneGroupBorderWidth::Focused
    } else {
        CommandPaneGroupBorderWidth::Inactive
    }
}


#[derive(Clone)]
pub(crate) struct DraggedCommandTab {
    pub(crate) source_group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) title: String,
    pub(crate) tab_status: CommandTerminalTabStatus,
}


pub(crate) struct CommandTabDragPreview {
    pub(crate) title: String,
    pub(crate) tab_status: CommandTerminalTabStatus,
}


#[derive(Clone, Copy)]
pub(crate) struct CommandPaneTab {
    pub(crate) session_id: CommandSessionId,
}


pub(crate) struct CommandPaneLeaf {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) tab_group: CommandPaneTabGroup,
}


pub(crate) struct CommandPaneSplit {
    pub(crate) id: CommandPaneSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) ratio: f32,
    pub(crate) first: Box<CommandPaneNode>,
    pub(crate) second: Box<CommandPaneNode>,
}


pub(crate) enum CommandPaneNode {
    Split(CommandPaneSplit),
    Leaf(CommandPaneLeaf),
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaneDropTarget {
    TabStrip(usize),
    PaneBody(WorkspaceDropZone),
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandPaneDropFeedback {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) target: CommandPaneDropTarget,
}


/*
CDXC:GPUIWorkspaceTabs 2026-06-26-06:34:
Native Agents pane tabs select on same-gesture mouse-up, not on mouse-down. Keep this pending tab click as runtime-only pane/session identity so drag start, mouse-up-out, and double-click Focus can share AppKit's gesture ownership without adding overlays, broad hit-test routing, persistence, terminal lifecycle changes, or synthetic input paths.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspacePendingTabClick {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}


/*
CDXC:GPUICommandTabSelection 2026-06-25-19:14:
Native AppKit command tabs arm a potential tab selection on left mouse-down, but commit selection only on the matching mouse-up while the gesture stayed a click. Keep GPUI's pending state as a runtime-only tab id token so a command-tab drag start can cancel selection without overlays, root hit-test routing, synthetic coordinates, persistence, or logging.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPanePendingTabClick {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) expand_on_click: bool,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabState {
    Loaded,
    AddressOnly,
}


impl BrowserTabState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "loaded" => Some(Self::Loaded),
            "address-only" => Some(Self::AddressOnly),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::AddressOnly => "address-only",
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabChromeStatus {
    LoadedSurface,
    RestoredPlaceholder,
    AddressOnly,
}


impl BrowserTabChromeStatus {
    /*
    CDXC:GPUIBrowserTabs 2026-06-22-16:48:
    Browser tab chrome must distinguish a loaded tab backed by a live CEF surface from a restored loaded placeholder that has no materialized surface. Derive this render-only status from BrowserTabState plus runtime browser_surfaces membership so shell-state persistence and favicon privacy stay unchanged.
    */
    pub(crate) fn from_state(state: BrowserTabState, has_cef_surface: bool) -> Self {
        match (state, has_cef_surface) {
            (BrowserTabState::Loaded, true) => Self::LoadedSurface,
            (BrowserTabState::Loaded, false) => Self::RestoredPlaceholder,
            (BrowserTabState::AddressOnly, _) => Self::AddressOnly,
        }
    }

    pub(crate) fn allows_runtime_favicon(self) -> bool {
        self == Self::LoadedSurface
    }
}


/*
CDXC:GPUIBrowserTabs 2026-06-22-17:13:
Browser tab chrome is focus-invariant: pane focus may drive toolbar ownership and CEF surface sync, but tab-bar brightness derives only from a tab's shell state, runtime surface presence, and active membership inside its own Browser tab group. BrowserTabModel.focused_pane is intentionally excluded.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserTabChromeSignature {
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
    pub(crate) active_in_tab_group: bool,
}


pub(crate) fn browser_tab_chrome_signature(
    tab_group: &BrowserTabGroup,
    tab_id: BrowserTabId,
    state: BrowserTabState,
    has_cef_surface: bool,
) -> BrowserTabChromeSignature {
    BrowserTabChromeSignature {
        state,
        chrome_status: BrowserTabChromeStatus::from_state(state, has_cef_surface),
        active_in_tab_group: tab_group.active_tab_id() == Some(tab_id),
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserRuntimeLifecycleInput {
    pub(crate) active_mode: TitlebarMode,
    pub(crate) browser_awake: bool,
    pub(crate) browser_tab_drag_active: bool,
    pub(crate) command_tab_drag_active: bool,
    pub(crate) workspace_tab_drag_active: bool,
}


impl BrowserRuntimeLifecycleInput {
    /*
    CDXC:GPUIWorkspaceTabDragVisibility 2026-07-03:
    Workspace/Agents tab drags join the same hide-during-drag gate as Browser and command tab drags so the GPUI drag ghost and drop feedback can never sit under a native CEF child view. Hide-and-hold only; no CEF teardown, recreation, or overlay layering.
    */
    pub(crate) fn allows_cef_child_views(self) -> bool {
        !self.browser_tab_drag_active
            && !self.command_tab_drag_active
            && !self.workspace_tab_drag_active
            && self.active_mode == TitlebarMode::Browser
            && self.browser_awake
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserRuntimeSurfacePolicy {
    Visible,
    HiddenHold,
    RestoredPlaceholder,
}


impl BrowserRuntimeSurfacePolicy {
    /*
    CDXC:GPUIBrowserLifecycle 2026-06-23-11:32:
    Phase 8 Browser runtime lifecycle is hide-and-hold at the native CEF child-view boundary. Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hide existing CEF views while retaining tab-owned CEF entities and shell tab metadata; visibility must not teardown, recreate, or materialize restored loaded tabs.

    CDXC:GPUIBrowserLifecycle 2026-06-23-14:30:
    Hide-and-hold is the only current Browser runtime lifecycle decision. HiddenHold means an existing tab-owned CEF child view is not shown; it must not be expanded into CEF teardown, CEF suspend, CEF recreation, restored-tab materialization, shell-state writes, or popup content transfer.
    */
    pub(crate) fn for_tab(
        lifecycle: BrowserRuntimeLifecycleInput,
        tab_state: Option<BrowserTabState>,
        rendered_active_loaded: bool,
        has_cef_surface: bool,
    ) -> Self {
        if !lifecycle.allows_cef_child_views() {
            return Self::HiddenHold;
        }

        match (tab_state, rendered_active_loaded, has_cef_surface) {
            (Some(BrowserTabState::Loaded), true, true) => Self::Visible,
            (Some(BrowserTabState::Loaded), true, false) => Self::RestoredPlaceholder,
            _ => Self::HiddenHold,
        }
    }

    pub(crate) fn shows_cef_child_view(self) -> bool {
        matches!(self, Self::Visible)
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserPopupTargetPolicy {
    OpenLoadedTab,
    IgnoreWithoutTransfer,
}


impl BrowserPopupTargetPolicy {
    /*
    CDXC:GPUIBrowserRuntimePolicy 2026-06-23-12:48:
    Phase 8 popup handling is intentionally source-only until a compatible blank-popup content-transfer contract exists. Only non-empty CEF target URLs may create loaded Browser tabs; empty script-created popups are handled as no-ops with no address-only tab, CEF surface, persistence, notification, page-content copy, or fallback transfer path.

    CDXC:GPUIBrowserRuntimePolicy 2026-06-23-14:30:
    Blank or whitespace script-created popup targets remain blocked at the shell boundary. Empty targets must no-op without address-only fallback, Browser notification, CEF surface creation, shell-state write, page-content copy, or any synthetic content-transfer path; non-empty targets are trimmed only as target identifiers for loaded-tab creation.
    */
    pub(crate) fn for_target_url(target_url: &str) -> Self {
        if target_url.trim().is_empty() {
            Self::IgnoreWithoutTransfer
        } else {
            Self::OpenLoadedTab
        }
    }

    pub(crate) fn opens_loaded_tab(self) -> bool {
        matches!(self, Self::OpenLoadedTab)
    }
}


pub(crate) fn browser_loaded_popup_target_url(requested_url: &str) -> Option<String> {
    BrowserPopupTargetPolicy::for_target_url(requested_url)
        .opens_loaded_tab()
        .then(|| requested_url.trim().to_string())
}


pub(crate) fn browser_runtime_visible_surface_tab_ids(
    lifecycle: BrowserRuntimeLifecycleInput,
    browser_tabs: &BrowserTabModel,
    surface_tab_ids: impl IntoIterator<Item = BrowserTabId>,
) -> HashSet<BrowserTabId> {
    /*
    CDXC:GPUIBrowserLifecycle 2026-06-23-14:30:
    The visibility pass is a pure hide-and-hold filter over already-owned CEF surfaces. It must not create missing restored-tab surfaces, suspend or tear down hidden CEF entities, infer popup content transfer, or mutate Browser shell state.
    */
    let rendered_active_loaded_tab_ids = browser_tabs.rendered_active_loaded_tab_ids();
    surface_tab_ids
        .into_iter()
        .filter(|tab_id| {
            BrowserRuntimeSurfacePolicy::for_tab(
                lifecycle,
                browser_tabs.tab(*tab_id).map(|tab| tab.state),
                rendered_active_loaded_tab_ids.contains(tab_id),
                true,
            )
            .shows_cef_child_view()
        })
        .collect()
}


#[derive(Clone)]
pub(crate) struct BrowserNavigationHistory {
    pub(crate) entries: Vec<String>,
    pub(crate) current_index: Option<usize>,
}


pub(crate) struct BrowserHistoryRow {
    pub(crate) index: usize,
    pub(crate) url: String,
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BrowserFaviconImage {
    pub(crate) image: Arc<Image>,
}


#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BrowserFaviconFetchSource {
    pub(crate) url: String,
}


#[derive(Clone)]
pub(crate) struct DraggedBrowserTab {
    pub(crate) source_pane_id: BrowserPaneId,
    pub(crate) tab_id: BrowserTabId,
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: String,
    pub(crate) runtime_favicon_url: Option<String>,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
}


pub(crate) struct BrowserTabDragPreview {
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: String,
    pub(crate) runtime_favicon_url: Option<String>,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
}


pub(crate) struct BrowserProfileModel {
    pub(crate) profiles: Vec<BrowserProfileId>,
    pub(crate) active_profile: BrowserProfileId,
    pub(crate) next_profile_id: u64,
}


impl BrowserProfileModel {
    pub(crate) fn shell_default() -> Self {
        /*
        CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
        GPUI Browser profile parity is shell-owned and generated-only for this slice: keep a built-in Default profile plus app-generated Profile N ids, persist only those stable ids and the active id, and avoid user-entered names, profile paths, cookies, credentials, history, page titles, URLs, command text, or local paths.
        */
        Self {
            profiles: vec![BrowserProfileId::default_profile()],
            active_profile: BrowserProfileId::default_profile(),
            next_profile_id: BROWSER_PROFILE_FIRST_GENERATED_ID,
        }
    }

    pub(crate) fn contains_profile(&self, profile_id: BrowserProfileId) -> bool {
        self.profiles.contains(&profile_id)
    }

    pub(crate) fn active_profile_id(&self) -> BrowserProfileId {
        if self.contains_profile(self.active_profile) {
            self.active_profile
        } else {
            BrowserProfileId::default_profile()
        }
    }

    pub(crate) fn profile_ids(&self) -> impl Iterator<Item = BrowserProfileId> + '_ {
        self.profiles.iter().copied()
    }

    pub(crate) fn select_profile(&mut self, profile_id: BrowserProfileId) -> bool {
        if !self.contains_profile(profile_id) || self.active_profile_id() == profile_id {
            return false;
        }

        self.active_profile = profile_id;
        true
    }

    pub(crate) fn create_generated_profile(&mut self) -> Option<BrowserProfileId> {
        if self.profiles.len() >= BROWSER_PROFILE_MAX_PROFILES {
            return None;
        }

        let mut next_id = self.next_profile_id.max(BROWSER_PROFILE_FIRST_GENERATED_ID);
        while self.contains_profile(BrowserProfileId(next_id)) {
            if next_id == u64::MAX {
                return None;
            }
            next_id = next_id.saturating_add(1);
        }

        let profile_id = BrowserProfileId(next_id);
        self.profiles.push(profile_id);
        self.active_profile = profile_id;
        self.next_profile_id = next_id.saturating_add(1);
        Some(profile_id)
    }
}


#[derive(Clone)]
pub(crate) struct BrowserTab {
    /*
    CDXC:GPUIBrowserMetadata 2026-06-22-07:23:
    Browser tab titles have two tiers: `title` is the URL-derived fallback that can be regenerated from sanitized shell state, while `runtime_page_title` comes from CEF DisplayHandler callbacks.

    CDXC:GPUIBrowserTabTitleCache 2026-07-12:
    The last displayed title is persisted into shell state as a bounded `cachedTitle` and restored into `runtime_page_title`, so the sidebar and tab strip keep the pre-restart label until the page reports a fresh document title.

    CDXC:GPUIBrowserFavicons 2026-06-22-09:11:
    Browser favicon metadata is runtime-only like page titles, but even stricter for persistence: keep only a safe HTTP(S) origin marker or capped decoded data:image bytes for visible tab chrome, clear favicon state on navigation/address-only transitions, and never serialize raw favicon URLs or image cache data into shell state.

    CDXC:GPUIBrowserFavicons 2026-06-22-10:41:
    Browser tab icons can render capped runtime favicon image bytes from safe data:image URLs without persisting favicon bytes or raw URLs. Clear decoded images and HTTP(S) fetch sources wherever favicon URL metadata clears, and use the URL-only shell marker as fallback.

    CDXC:GPUIBrowserFavicons 2026-06-22-11:05:
    Browser tab icons may fetch safe HTTP(S) favicons only through a favicon-specific runtime asset source. Keep raw CEF favicon URLs out of shell state and logging, store only a scheme+authority marker on BrowserTab, cap URL length, redirects, body bytes, formats, and decode dimensions, and fall back to the marker or generic icon on every failure.

    CDXC:GPUIBrowserHistory 2026-06-22-10:09:
    GPUI Browser tabs need their own compact navigation history so the toolbar History menu can show rows for the focused tab without borrowing another tab's CEF internals. Keep history keyed by BrowserTabId and show history through OS-owned NativeMenus, not Back/Forward dropdown toggles, in-layout panels, or overlay hit regions.

    CDXC:GPUIBrowserHistoryPrivacy 2026-06-22-10:09:
    Browser history persistence is limited to sanitized loaded URLs and a current index. Do not store page titles, labels, favicon URLs, query strings, fragments, credentials, cookies, tokens, local paths, command text, stdout/stderr, or user-owned content; rebuild invalid or missing history from the tab's sanitized loaded URL.

    CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
    Each GPUI Browser tab carries its selected generated profile id. Changing a tab's profile recreates only that tab's CEF surface with the selected request context, and shell-state persistence stores only this safe numeric id so different tabs keep different profiles across restart without persisting profile names, paths, cookies, credentials, history, or user-entered browser data.
    */
    pub(crate) id: BrowserTabId,
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: String,
    pub(crate) runtime_page_title: Option<String>,
    pub(crate) runtime_favicon_url: Option<String>,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) runtime_is_loading: bool,
    pub(crate) runtime_can_go_back: bool,
    pub(crate) runtime_can_go_forward: bool,
    pub(crate) url: String,
    pub(crate) state: BrowserTabState,
    pub(crate) navigation_history: BrowserNavigationHistory,
}


pub(crate) struct BrowserBodyPlaceholder {
    pub(crate) state: BrowserTabState,
    pub(crate) safe_title: Option<String>,
    pub(crate) safe_url: Option<String>,
    pub(crate) has_cef_surface: bool,
}


impl BrowserBodyPlaceholder {
    pub(crate) fn blank() -> Self {
        Self {
            state: BrowserTabState::AddressOnly,
            safe_title: None,
            safe_url: None,
            has_cef_surface: false,
        }
    }

    pub(crate) fn from_tab(tab: &BrowserTab, has_cef_surface: bool) -> Self {
        let sanitized_url = if tab.state == BrowserTabState::Loaded {
            sanitize_browser_tab_url_for_state(&tab.url)
        } else {
            None
        };

        Self {
            state: tab.state,
            safe_title: sanitized_url.as_deref().map(browser_tab_title_for_url),
            safe_url: sanitized_url
                .as_deref()
                .and_then(browser_placeholder_safe_origin_url),
            has_cef_surface,
        }
    }
}


#[derive(Clone)]
pub(crate) struct BrowserTabModel {
    pub(crate) tabs: Vec<BrowserTab>,
    pub(crate) root: BrowserNode,
    pub(crate) focused_pane: BrowserPaneId,
    pub(crate) active_tab: BrowserTabId,
    pub(crate) next_pane_id: u64,
    pub(crate) next_split_id: u64,
    pub(crate) next_tab_id: u64,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabDropTarget {
    TabStrip(usize),
    PaneBody(WorkspaceDropZone),
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserDropFeedback {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) target: BrowserTabDropTarget,
}


#[derive(Clone)]
pub(crate) struct BrowserPaneTab {
    pub(crate) tab_id: BrowserTabId,
}


#[derive(Clone)]
pub(crate) struct BrowserTabGroup {
    pub(crate) tabs: Vec<BrowserPaneTab>,
    pub(crate) active_tab: BrowserTabId,
}


#[derive(Clone)]
pub(crate) struct BrowserLeaf {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) tab_group: BrowserTabGroup,
}


#[derive(Clone)]
pub(crate) struct BrowserSplit {
    pub(crate) id: BrowserSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) ratio: f32,
    pub(crate) first: Box<BrowserNode>,
    pub(crate) second: Box<BrowserNode>,
}


#[derive(Clone)]
pub(crate) enum BrowserNode {
    Split(BrowserSplit),
    Leaf(BrowserLeaf),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellFocusTarget {
    /*
    CDXC:GPUIKeyboardFocus 2026-06-22-22:59:
    The GPUI shell needs explicit surface ownership before full keyboard/mouse delivery exists. Track which shell surface owns keyboard actions so tab cycling, Cmd-W close, command-pane F12 focus, project-editor companion close, and runtime Agents Ghostty surface focus mirror the macOS workspace without adding native hit-test routing or runtime teardown.

    CDXC:GPUIKeyboardFocus 2026-06-22-07:54:
    The command pane must remember the last valid non-command workspace/editor focus before it takes keyboard ownership, then restore that focus when command chrome or the final command placeholder hides the pane. Store only the same enum/id focus metadata already used by shell-state persistence.

    CDXC:GPUIKeyboardFocus 2026-06-27-09:42:
    Hiding or collapsing Commands from the Browser workarea must restore the exact focused Browser pane, not only the coarse BrowserSurface. Persist BrowserPane only for TitlebarMode::Browser; Agents-mode browser sessions remain unavailable until GPUI has an Agents workspace browser-session model.
    */
    AgentsPane(WorkspacePaneId),
    CommandPane,
    BrowserSurface,
    BrowserPane(BrowserPaneId),
    ProjectEditorSurface(TitlebarMode),
    ProjectEditorCompanion(TitlebarMode),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FirstResponderTerminalSurface {
    Agents(TerminalSessionId),
    Command(CommandSessionId),
    ProjectEditorCompanion(TerminalSessionId),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FirstResponderCefSurface {
    Sidebar,
    BrowserTab(BrowserTabId),
    ProjectWorkarea(ProjectWorkareaCefSurfaceSlotKey),
    ProjectEditorCompanion,
    TitlebarTips,
    AppModal,
    SessionChat(TerminalSessionId),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FirstResponderTarget {
    TerminalSurface(FirstResponderTerminalSurface),
    CefSurface(FirstResponderCefSurface),
    GpuiWindow,
    Other,
    None,
}


/*
CDXC:GPUIKeyboardRouter 2026-07-24:
Keyboard ownership is window-scoped and exact. AppKit, CEF, native Ghostty,
and composited GPUI terminals do not share one responder system, so the native
event boundary records the owner that existed when a key was pressed and
routes only global application commands, confirmed Ghostex hotkeys, or the Tab
lifecycle that AppKit removes before GPUI can observe it. Ordinary text, IME,
and surface-local shortcuts remain on their native surface paths. A losing
composited terminal may clear ownership only when it still owns the window,
preventing split-pane focus event ordering from turning the focused-terminal
state false.
*/
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiKeyboardOwner {
    CompositedTerminal(GpuiEngineTerminalEventTarget),
    FirstResponder(FirstResponderTarget),
}


#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCapturedKeyRoute {
    CompositedTerminalTab {
        owner: GpuiEngineTerminalEventTarget,
        shift: bool,
    },
    CompositedTerminalBulkText {
        owner: GpuiEngineTerminalEventTarget,
    },
    ApplicationCommand {
        command: GpuiApplicationKeyboardCommand,
        owner: GpuiKeyboardOwner,
    },
    GhostexHotkey {
        action_id: String,
        owner: GpuiKeyboardOwner,
    },
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiApplicationKeyboardCommand {
    Hide,
    HideOthers,
    MinimizeWindow,
    Quit,
}


#[cfg(target_os = "macos")]
impl GpuiApplicationKeyboardCommand {
    pub(crate) fn log_id(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::HideOthers => "hideOthers",
            Self::MinimizeWindow => "minimizeWindow",
            Self::Quit => "quit",
        }
    }
}


#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
pub(crate) enum GpuiNativeKeyboardDispatch {
    CompositedTerminalTab {
        owner: GpuiEngineTerminalEventTarget,
        action: ghostty_vt::VtKeyAction,
        shift: bool,
    },
    CompositedTerminalBulkText {
        owner: GpuiEngineTerminalEventTarget,
        text: String,
    },
    ApplicationCommand(GpuiApplicationKeyboardCommand),
    GhostexHotkey {
        action_id: String,
        owner: GpuiKeyboardOwner,
    },
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspacePaneBorderState {
    Neutral,
    Focused,
    Attention,
}


#[derive(Clone, Copy)]
pub(crate) struct SidebarFocusBorderHandoff {
    pub(crate) held_pane_id: WorkspacePaneId,
    pub(crate) target_session_id: Option<TerminalSessionId>,
    pub(crate) started_at: Instant,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPanePaletteOpenDecision {
    OpenAndFocus,
    FocusVisible,
}


pub(crate) fn command_pane_palette_open_decision(
    command_pane_expanded: bool,
    _shell_focus: ShellFocusTarget,
) -> CommandPanePaletteOpenDecision {
    /*
    CDXC:GPUICommandPalette 2026-06-26-07:15:
    Open Commands Panel and F12 share one open/focus contract. Hidden panels open through the normal default-height path, while visible panels only focus the command pane even if command focus is already active.
    */
    if command_pane_expanded {
        CommandPanePaletteOpenDecision::FocusVisible
    } else {
        CommandPanePaletteOpenDecision::OpenAndFocus
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceFocusDirection {
    Left,
    Right,
    Up,
    Down,
}


impl WorkspaceFocusDirection {
    pub(crate) fn from_command_palette_directional_focus_action_id(action_id: &str) -> Option<Self> {
        /*
        CDXC:GPUICommandPalette 2026-06-26-07:33:
        Shared command-palette directional focus rows post `focusUp`, `focusRight`, `focusDown`, and `focusLeft` through `runGhostexHotkeyAction`. Keep this mapper exact so the handler can dispatch to the existing workspace directional-focus route without inventing separate focus semantics or absorbing unrelated hotkey ids.
        */
        match action_id {
            "focusLeft" => Some(Self::Left),
            "focusRight" => Some(Self::Right),
            "focusUp" => Some(Self::Up),
            "focusDown" => Some(Self::Down),
            _ => None,
        }
    }

    pub(crate) fn uses_previous_order_fallback(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }
}


#[derive(Clone, Copy)]
pub(crate) enum FocusedTerminalSplitDirection {
    Right,
    Down,
}


pub(crate) fn command_pane_focused_split_axis(direction: FocusedTerminalSplitDirection) -> WorkspaceSplitAxis {
    /*
    CDXC:GPUIFocusedSplits 2026-06-25-16:05:
    macOS command-panel split hotkeys log the requested direction but force command panes to horizontal split placement. Mirror that rule in GPUI so Cmd+Shift+D inside the command pane does not create a vertical command split.
    */
    match direction {
        FocusedTerminalSplitDirection::Right | FocusedTerminalSplitDirection::Down => {
            WorkspaceSplitAxis::Horizontal
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpatialFocusTarget {
    AgentsPane(WorkspacePaneId),
    BrowserPane(BrowserPaneId),
    ProjectEditorSurface(TitlebarMode),
    ProjectEditorCompanion(TitlebarMode),
    CommandPane,
    CommandPaneGroup(CommandPaneGroupId),
}


#[derive(Clone, Copy)]
pub(crate) struct FocusCandidate {
    pub(crate) target: SpatialFocusTarget,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) order: usize,
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorFocusBounds {
    pub(crate) mode: TitlebarMode,
    pub(crate) bounds: Bounds<Pixels>,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpatialFocusOutcome {
    Focused,
    NoTarget,
    BoundsUnavailable,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectEditorLifecycleState {
    Awake,
    Sleeping,
}


impl ProjectEditorLifecycleState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "awake" => Some(Self::Awake),
            "sleeping" => Some(Self::Sleeping),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Awake => "awake",
            Self::Sleeping => "sleeping",
        }
    }
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorModeLifecycle {
    pub(crate) state: ProjectEditorLifecycleState,
    pub(crate) recency: u64,
}


#[derive(Default)]
pub(crate) struct ProjectEditorAutoSleepEpochs {
    pub(crate) source: u64,
    pub(crate) browser: u64,
    pub(crate) kanban: u64,
    pub(crate) automate: u64,
    pub(crate) manage: u64,
}


impl ProjectEditorAutoSleepEpochs {
    pub(crate) fn epoch(&self, mode: TitlebarMode) -> Option<u64> {
        match mode {
            TitlebarMode::Source => Some(self.source),
            TitlebarMode::Browser => Some(self.browser),
            TitlebarMode::Kanban => Some(self.kanban),
            TitlebarMode::Automate => Some(self.automate),
            TitlebarMode::Manage => Some(self.manage),
            TitlebarMode::Agents => None,
        }
    }

    pub(crate) fn bump(&mut self, mode: TitlebarMode) -> Option<u64> {
        let epoch = match mode {
            TitlebarMode::Source => &mut self.source,
            TitlebarMode::Browser => &mut self.browser,
            TitlebarMode::Kanban => &mut self.kanban,
            TitlebarMode::Automate => &mut self.automate,
            TitlebarMode::Manage => &mut self.manage,
            TitlebarMode::Agents => return None,
        };
        *epoch = epoch.wrapping_add(1);
        Some(*epoch)
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectEditorAutoSleepPolicySnapshot {
    pub(crate) source: Option<Duration>,
    pub(crate) browser: Option<Duration>,
    pub(crate) kanban: Option<Duration>,
    pub(crate) automate: Option<Duration>,
    pub(crate) manage: Option<Duration>,
}


impl ProjectEditorAutoSleepPolicySnapshot {
    pub(crate) fn read_current() -> Self {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        Self::from_shared_settings(&settings)
    }

    pub(crate) fn from_shared_settings(settings: &shared_settings::SharedSidebarSettingsSnapshot) -> Self {
        Self {
            source: project_editor_auto_sleep_duration(TitlebarMode::Source, settings),
            browser: project_editor_auto_sleep_duration(TitlebarMode::Browser, settings),
            kanban: project_editor_auto_sleep_duration(TitlebarMode::Kanban, settings),
            automate: project_editor_auto_sleep_duration(TitlebarMode::Automate, settings),
            manage: project_editor_auto_sleep_duration(TitlebarMode::Manage, settings),
        }
    }

    pub(crate) fn duration_for_mode(self, mode: TitlebarMode) -> Option<Duration> {
        match mode {
            TitlebarMode::Source => self.source,
            TitlebarMode::Browser => self.browser,
            TitlebarMode::Kanban => self.kanban,
            TitlebarMode::Automate => self.automate,
            TitlebarMode::Manage => self.manage,
            TitlebarMode::Agents => None,
        }
    }
}


/*
CDXC:GPUIProjectViewMemory 2026-08-07:
The workarea a project was last shown in — and how that project's companion
pane was arranged — is project-owned state, exactly like its Agents split
topology. Keyed by the same canonical workspace project key, so a remote
project's view memory is machine-scoped and never collides with a same-named
local project. Companion slot occupants are the project's own shell session
ids, the identical vocabulary the parked workspace model already stores, so
they are meaningful only alongside that model and are validated against it on
restore.
*/
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct GpuiProjectViewState {
    pub(crate) active_mode: TitlebarMode,
    pub(crate) companion_visible: bool,
    pub(crate) companion_split_enabled: bool,
    pub(crate) companion_width_ratio: f32,
    pub(crate) companion_split_ratio: f32,
    pub(crate) companion_top_session_id: Option<TerminalSessionId>,
    pub(crate) companion_bottom_session_id: Option<TerminalSessionId>,
    pub(crate) companion_focused_slot: ProjectEditorCompanionTerminalSlot,
}


pub(crate) struct ProjectEditorShellModel {
    pub(crate) left_companion_visible: bool,
    pub(crate) left_companion_width_ratio: f32,
    pub(crate) left_companion_split_enabled: bool,
    pub(crate) left_companion_split_ratio: f32,
    pub(crate) source_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) browser_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) kanban_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) automate_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) manage_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) next_lifecycle_recency: u64,
}


impl ProjectEditorShellModel {
    pub(crate) fn shell_default() -> Self {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        Source, Browser, Kanban, Automate, and Docs are project-editor workspace modes in the GPUI parity shell. They replace the normal Agents workspace while active, keep command-pane wrapping outside the editor area, and reserve an in-memory left companion pane with a real divider region while project routing stays deferred.

        CDXC:GPUIProjectEditor 2026-06-22-06:24:
        The project-editor companion should start near the macOS default ratio intent instead of a small fixed pixel width. Store the shell default as a 32% editor-area ratio and apply practical companion/editor minimums at render and resize time.

        CDXC:GPUIProjectEditorLifecycle 2026-06-22-08:29:
        Source, Browser, Kanban, Automate, and Docs need independent shell-level awake/sleeping state while their real surfaces remain runtime-owned. Persist only enum-like lifecycle values and recency counters; runtime auto-sleep epochs live on the GPUI app so timer tokens never enter shell state and no source content, paths, raw page titles, command text, tokens, or secrets are stored.

        CDXC:GPUIManageLifecycle 2026-06-23-14:08:
        Docs sleep/wake must preserve the selected project/workarea runtime identity while hiding or restoring only shell-owned surface state. Sleeping, waking, and load-failed Docs states must not clear or synthesize CEF/file-bridge readiness, perform file I/O, persist project facts, or create fallback surfaces.

        CDXC:GPUIManageLifecycle 2026-06-23-14:48:
        Docs sleep/wake must preserve companion layout and command-pane shell state at the same shell boundary as Source and Kanban. Lifecycle toggles may not synthesize readiness, mount CEF or file bridges, reset shell-owned layout, persist private project/workarea facts, or create fallback surfaces.

        CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
        Automate participates in project-editor shell lifecycle, focus, persistence, companion layout, and the direct workarea CEF slot. Shell lifecycle may wake or sleep the mode only; it must not synthesize surface ids, issue fallback URLs, persist private page facts, or mount hidden CEF views outside the active workarea gate.

        CDXC:GPUIKanbanLifecycle 2026-06-24-08:09:
        Kanban sleep/wake/lifecycle must preserve the explicit project/board runtime identity plus separate CEF bridge state, including load-failed, without becoming runtime CEF instantiation, runtime URL issuance, hidden mounts, placeholder replacement, fallback probes, logging/persistence/private payloads, or WKWebView/WebKit/non-CEF paths.

        CDXC:GPUIKanbanLifecycle 2026-06-28-17:09:
        Kanban runtime CEF creation is owned by the direct runtime URL/CefSurface gate, not by shell lifecycle. Do not widen readiness or sidebar bridge routing into URL/path/CEF payloads, hidden mounts, placeholder replacement, fallback probes, logging, persistence, private payloads, or WKWebView/WebKit paths.

        CDXC:GPUIProjectEditor 2026-06-22-08:15:
        The optional project-editor companion has explicit shell-owned hide and restore controls before real Source, Browser, Kanban, Automate, and Docs companion content exists. Hiding only toggles companion visibility and focus; it preserves the stored width ratio plus Browser tab/surface identity, placeholder editor identity, command-pane state, and terminal placeholder state.
        */
        Self {
            left_companion_visible: true,
            left_companion_width_ratio: PROJECT_EDITOR_COMPANION_WIDTH_RATIO,
            left_companion_split_enabled: true,
            left_companion_split_ratio: PROJECT_EDITOR_COMPANION_SPLIT_RATIO,
            source_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            browser_lifecycle: ProjectEditorModeLifecycle {
                state: ProjectEditorLifecycleState::Awake,
                recency: 1,
            },
            kanban_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            automate_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            manage_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            next_lifecycle_recency: 2,
        }
    }

    pub(crate) fn lifecycle(&self, mode: TitlebarMode) -> Option<ProjectEditorModeLifecycle> {
        match mode {
            TitlebarMode::Source => Some(self.source_lifecycle),
            TitlebarMode::Browser => Some(self.browser_lifecycle),
            TitlebarMode::Kanban => Some(self.kanban_lifecycle),
            TitlebarMode::Automate => Some(self.automate_lifecycle),
            TitlebarMode::Manage => Some(self.manage_lifecycle),
            TitlebarMode::Agents => None,
        }
    }

    pub(crate) fn lifecycle_mut(&mut self, mode: TitlebarMode) -> Option<&mut ProjectEditorModeLifecycle> {
        match mode {
            TitlebarMode::Source => Some(&mut self.source_lifecycle),
            TitlebarMode::Browser => Some(&mut self.browser_lifecycle),
            TitlebarMode::Kanban => Some(&mut self.kanban_lifecycle),
            TitlebarMode::Automate => Some(&mut self.automate_lifecycle),
            TitlebarMode::Manage => Some(&mut self.manage_lifecycle),
            TitlebarMode::Agents => None,
        }
    }

    pub(crate) fn is_mode_awake(&self, mode: TitlebarMode) -> bool {
        self.lifecycle(mode)
            .is_some_and(|lifecycle| lifecycle.state == ProjectEditorLifecycleState::Awake)
    }

    pub(crate) fn mark_mode_awake(&mut self, mode: TitlebarMode) -> bool {
        if !mode.is_project_editor_mode() {
            return false;
        }

        let recency = self.next_lifecycle_recency.max(1);
        self.next_lifecycle_recency = recency.saturating_add(1);
        let Some(lifecycle) = self.lifecycle_mut(mode) else {
            return false;
        };
        lifecycle.state = ProjectEditorLifecycleState::Awake;
        lifecycle.recency = recency;
        self.enforce_awake_mode_cap(mode);
        true
    }

    pub(crate) fn enforce_awake_mode_cap(&mut self, active_mode: TitlebarMode) {
        let mut awake_modes = project_editor_modes()
            .iter()
            .filter_map(|mode| {
                let lifecycle = self.lifecycle(*mode)?;
                if lifecycle.state == ProjectEditorLifecycleState::Awake {
                    Some((*mode, lifecycle.recency))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if awake_modes.len() <= PROJECT_EDITOR_AWAKE_MODE_CAP {
            return;
        }

        awake_modes.sort_by_key(|(mode, recency)| (*recency, mode.project_editor_order()));
        let mut modes_to_sleep = awake_modes.len() - PROJECT_EDITOR_AWAKE_MODE_CAP;
        for (mode, _) in awake_modes {
            if modes_to_sleep == 0 {
                break;
            }
            if mode == active_mode {
                continue;
            }
            if let Some(lifecycle) = self.lifecycle_mut(mode) {
                lifecycle.state = ProjectEditorLifecycleState::Sleeping;
                modes_to_sleep -= 1;
            }
        }
    }

    pub(crate) fn mark_mode_sleeping(&mut self, mode: TitlebarMode) -> bool {
        let Some(lifecycle) = self.lifecycle_mut(mode) else {
            return false;
        };
        if lifecycle.state == ProjectEditorLifecycleState::Sleeping {
            return false;
        }

        lifecycle.state = ProjectEditorLifecycleState::Sleeping;
        true
    }

    pub(crate) fn set_left_companion_width_ratio(&mut self, ratio: f32, content_span: f32) -> bool {
        let next_ratio = project_editor_companion_width_ratio_for_span(ratio, content_span);
        if (self.left_companion_width_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_width_ratio = next_ratio;
        true
    }

    pub(crate) fn reset_left_companion_width_ratio(&mut self, content_span: Option<f32>) -> bool {
        let next_ratio = content_span
            .map(|content_span| {
                project_editor_companion_width_ratio_for_span(
                    PROJECT_EDITOR_COMPANION_WIDTH_RATIO,
                    content_span,
                )
            })
            .unwrap_or(PROJECT_EDITOR_COMPANION_WIDTH_RATIO);
        if (self.left_companion_width_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_width_ratio = next_ratio;
        true
    }

    pub(crate) fn set_left_companion_split_ratio(&mut self, ratio: f32, content_span: f32) -> bool {
        let Some((minimum, maximum)) = split_drag_ratio_bounds_from_minimums(
            PANE_RESIZE_MINIMUM_HEIGHT,
            PANE_RESIZE_MINIMUM_HEIGHT,
            content_span,
        ) else {
            return false;
        };
        let next_ratio = ratio.clamp(minimum, maximum);
        if (self.left_companion_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_split_ratio = next_ratio;
        true
    }

    pub(crate) fn reset_left_companion_split_ratio(&mut self, content_span: Option<f32>) -> bool {
        let next_ratio = content_span
            .and_then(|content_span| {
                split_drag_ratio_bounds_from_minimums(
                    PANE_RESIZE_MINIMUM_HEIGHT,
                    PANE_RESIZE_MINIMUM_HEIGHT,
                    content_span,
                )
                .map(|(minimum, maximum)| {
                    PROJECT_EDITOR_COMPANION_SPLIT_RATIO.clamp(minimum, maximum)
                })
            })
            .unwrap_or(PROJECT_EDITOR_COMPANION_SPLIT_RATIO);
        if (self.left_companion_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_split_ratio = next_ratio;
        true
    }

    pub(crate) fn hide_left_companion(&mut self) -> bool {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-14:42:
        Companion close/hide needs a pure model transition for regression coverage: it may only hide the companion pane and must preserve stored width, mode lifecycle/recency, Browser tab identity, command-pane state, and terminal placeholder state for later restore.
        */
        if !self.left_companion_visible {
            return false;
        }

        self.left_companion_visible = false;
        true
    }

    pub(crate) fn restore_left_companion(&mut self) -> bool {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-14:42:
        Companion restore reuses the current shell-owned width and lifecycle state instead of recreating placeholder surfaces or resetting the project-editor layout.
        */
        if self.left_companion_visible {
            return false;
        }

        self.left_companion_visible = true;
        true
    }
}


impl ProjectEditorModeLifecycle {
    pub(crate) fn sleeping() -> Self {
        Self {
            state: ProjectEditorLifecycleState::Sleeping,
            recency: 0,
        }
    }
}


pub(crate) fn project_editor_modes() -> [TitlebarMode; 5] {
    [
        TitlebarMode::Source,
        TitlebarMode::Browser,
        TitlebarMode::Kanban,
        TitlebarMode::Automate,
        TitlebarMode::Manage,
    ]
}


impl BrowserNavigationHistory {
    pub(crate) fn empty() -> Self {
        Self {
            entries: Vec::new(),
            current_index: None,
        }
    }

    pub(crate) fn loaded(url: &str) -> Self {
        let url = url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
            return Self::empty();
        }

        Self {
            entries: vec![url.to_string()],
            current_index: Some(0),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.current_index = None;
    }

    pub(crate) fn append_loaded_url(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
            self.clear();
            return;
        }

        if let Some(current_index) = self.current_index {
            self.entries.truncate(current_index.saturating_add(1));
        } else {
            self.entries.clear();
        }
        self.entries.push(url.to_string());
        self.current_index = Some(self.entries.len().saturating_sub(1));
        self.enforce_cap();
    }

    pub(crate) fn record_address_change(&mut self, url: &str) -> bool {
        let url = url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
            let changed = !self.entries.is_empty() || self.current_index.is_some();
            self.clear();
            return changed;
        }

        let Some(current_index) = self
            .current_index
            .filter(|index| *index < self.entries.len())
        else {
            self.entries = vec![url.to_string()];
            self.current_index = Some(0);
            return true;
        };

        if self.entries[current_index] == url {
            return false;
        }

        if current_index > 0 && self.entries[current_index - 1] == url {
            self.current_index = Some(current_index - 1);
            return true;
        }

        if current_index + 1 < self.entries.len() && self.entries[current_index + 1] == url {
            self.current_index = Some(current_index + 1);
            return true;
        }

        self.entries.truncate(current_index.saturating_add(1));
        self.entries.push(url.to_string());
        self.current_index = Some(self.entries.len().saturating_sub(1));
        self.enforce_cap();
        true
    }

    pub(crate) fn rows_around_current(&self, max_rows: usize) -> Vec<BrowserHistoryRow> {
        let Some(current_index) = self
            .current_index
            .filter(|index| *index < self.entries.len())
        else {
            return Vec::new();
        };
        let visible_count = max_rows.max(1).min(self.entries.len());
        let mut start_index = current_index.saturating_sub(visible_count / 2);
        if start_index + visible_count > self.entries.len() {
            start_index = self.entries.len().saturating_sub(visible_count);
        }
        let end_index = start_index + visible_count;

        self.entries[start_index..end_index]
            .iter()
            .enumerate()
            .map(|(offset, url)| {
                let index = start_index + offset;
                BrowserHistoryRow {
                    index,
                    url: url.clone(),
                }
            })
            .collect()
    }

    pub(crate) fn enforce_cap(&mut self) {
        if self.entries.len() <= BROWSER_HISTORY_MAX_ENTRIES {
            return;
        }

        let remove_count = self.entries.len() - BROWSER_HISTORY_MAX_ENTRIES;
        self.entries.drain(0..remove_count);
        if let Some(current_index) = self.current_index {
            self.current_index = Some(current_index.saturating_sub(remove_count));
        }
    }
}

