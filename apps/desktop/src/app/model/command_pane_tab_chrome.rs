// C1 wave-3 re-cluster: command pane tab chrome: mode/layout plan, tab context/focus/order helpers, tab scroll/hover/tooltip geometry, and tab chrome/border signatures, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaneMode {
    Pinned,
    Floating,
    Collapsed,
}

impl CommandPaneMode {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "pinned" => Some(Self::Pinned),
            "floating" => Some(Self::Floating),
            "collapsed" => Some(Self::Collapsed),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::Floating => "floating",
            Self::Collapsed => "collapsed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneNewCommandControlPlacement {
    FixedActionCluster,
    InlineTabRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneBottomReservationChrome {
    PlainChrome,
    CollapsedStrip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandPaneWorkspaceBottomReservation {
    pub(crate) chrome: CommandPaneBottomReservationChrome,
    pub(crate) height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CommandPaneWorkspaceLayoutPlan {
    Hidden,
    Pinned {
        panel_height: f32,
    },
    PinnedRight {
        panel_width: f32,
    },
    Floating {
        panel_height: f32,
        bottom_reservation: CommandPaneWorkspaceBottomReservation,
    },
    Collapsed {
        bottom_reservation: CommandPaneWorkspaceBottomReservation,
    },
}

pub(crate) fn agents_workspace_tab_context_close_scope_label(
    scope: AgentsWorkspaceTabCloseScope,
) -> &'static str {
    match scope {
        AgentsWorkspaceTabCloseScope::Close => "Close Tab",
        AgentsWorkspaceTabCloseScope::CloseLeft => "Close Left",
        AgentsWorkspaceTabCloseScope::CloseOthers => "Close Other Tabs",
        AgentsWorkspaceTabCloseScope::CloseRight => "Close Right",
    }
}

pub(crate) fn agents_workspace_tab_context_sleep_scope_label(
    scope: AgentsWorkspaceTabSleepScope,
) -> &'static str {
    match scope {
        AgentsWorkspaceTabSleepScope::Sleep => "Sleep",
        AgentsWorkspaceTabSleepScope::SleepLeft => "Sleep Left",
        AgentsWorkspaceTabSleepScope::SleepOthers => "Sleep Other Tabs",
        AgentsWorkspaceTabSleepScope::SleepRight => "Sleep Right",
    }
}

pub(crate) fn agents_workspace_tab_context_focus_label() -> &'static str {
    "Focus"
}

pub(crate) fn agents_workspace_tab_context_scoped_close_order() -> [AgentsWorkspaceTabCloseScope; 3]
{
    /*
    CDXC:GPUIAgentsTabContextMenu 2026-06-26-06:57:
    Native workspace tab right-click menus omit direct Close Tab and order scoped close rows as Close Right, Close Left, then Close Other Tabs. GPUI Agents menus use the same row set while direct close remains owned by inline tab chrome and middle-click gestures.
    */
    [
        AgentsWorkspaceTabCloseScope::CloseRight,
        AgentsWorkspaceTabCloseScope::CloseLeft,
        AgentsWorkspaceTabCloseScope::CloseOthers,
    ]
}

pub(crate) fn agents_workspace_tab_context_sleep_order(
    clicked_tab_is_sleeping: bool,
) -> Vec<AgentsWorkspaceTabSleepScope> {
    /*
    CDXC:GPUIAgentsTabContextMenu 2026-06-26-06:57:
    Native workspace tab menus show direct Sleep only for awake clicked tabs, then always show Sleep Right, Sleep Left, and Sleep Other Tabs before the close group. Empty sibling scopes remain action rows and no-op in the pane-local resolver.
    */
    let mut scopes = Vec::with_capacity(4);
    if !clicked_tab_is_sleeping {
        scopes.push(AgentsWorkspaceTabSleepScope::Sleep);
    }
    scopes.extend([
        AgentsWorkspaceTabSleepScope::SleepRight,
        AgentsWorkspaceTabSleepScope::SleepLeft,
        AgentsWorkspaceTabSleepScope::SleepOthers,
    ]);
    scopes
}

pub(crate) fn command_pane_tab_context_close_scope_label(
    scope: CommandPaneTabCloseScope,
) -> &'static str {
    match scope {
        CommandPaneTabCloseScope::Close => "Close Tab",
        CommandPaneTabCloseScope::CloseLeft => "Close Left",
        CommandPaneTabCloseScope::CloseOthers => "Close Other Tabs",
        CommandPaneTabCloseScope::CloseRight => "Close Right",
    }
}

pub(crate) fn command_pane_tab_context_sleep_scope_label(
    scope: CommandPaneTabSleepScope,
) -> &'static str {
    match scope {
        CommandPaneTabSleepScope::Sleep => "Sleep",
        CommandPaneTabSleepScope::SleepLeft => "Sleep Left",
        CommandPaneTabSleepScope::SleepOthers => "Sleep Other Tabs",
        CommandPaneTabSleepScope::SleepRight => "Sleep Right",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneTabSessionAction {
    Rename,
    #[allow(dead_code)]
    // never constructed: kept so the chrome-status enum stays a complete mirror of the tab status vocabulary
    DelayedSend,
    CloseAfterDone,
}

pub(crate) fn command_pane_tab_context_focus_label() -> &'static str {
    "Focus"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneTabContextFocusPolicy {
    SelectAndFocus,
    SelectExpandWakeAndFocus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneScopedTabMutationFocusPolicy {
    FocusCommandPane,
    PreserveCurrentFocus,
}

pub(crate) fn command_pane_tab_context_session_action_focus_policy(
    action: CommandPaneTabSessionAction,
) -> CommandPaneTabContextFocusPolicy {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-18:33:
    Retained clicked-tab action handlers focus the clicked terminal before dispatch. GPUI mirrors native dispatch for Rename and Close After Done by selecting/focusing the clicked command tab without expanding hidden chrome, while Delayed Send still expands and wakes because it must target a visible mounted command body for the later Return.

    CDXC:GPUICommandTabContextMenu 2026-06-27-01:49:
    Keep these dispatch policies for existing non-menu action handlers, but do not use them as evidence that command-tab right-click should expose Rename Session, Delayed Send, or Close After Done rows.
    */
    match action {
        CommandPaneTabSessionAction::Rename | CommandPaneTabSessionAction::CloseAfterDone => {
            CommandPaneTabContextFocusPolicy::SelectAndFocus
        }
        CommandPaneTabSessionAction::DelayedSend => {
            CommandPaneTabContextFocusPolicy::SelectExpandWakeAndFocus
        }
    }
}

pub(crate) fn command_pane_tab_context_scoped_lifecycle_focus_policy()
-> CommandPaneScopedTabMutationFocusPolicy {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-18:38:
    Native scoped Sleep/Close context-menu rows do not focus the clicked terminal before dispatch; only primary tab actions route through `nativeTabContextMenuAction`. Preserve GPUI shell focus for scoped lifecycle rows while direct/focused command close and focused Sleep keep their existing focus ownership.
    */
    CommandPaneScopedTabMutationFocusPolicy::PreserveCurrentFocus
}

pub(crate) fn command_pane_tab_context_runtime_action_count(
    _command_pane: &CommandPaneModel,
    _group_id: CommandPaneGroupId,
    _session_id: CommandSessionId,
) -> usize {
    /*
    CDXC:GPUICommandTabRuntimeActions 2026-06-25-21:59:
    Fork Session, Reload Session, and Pop Out Pane must stay absent from GPUI command-tab context menus until they can dispatch to real command-pane runtime semantics. Current command Ghostty surfaces support mount, focus, input, close/confirm, sleep parking, and action timers only; there is no command-session clone, live embedded reload, or popped-out command-owner transfer path. Do not add disabled rows, fallback toasts, shell-only duplicates, surface drops, or placeholder menu actions.

    CDXC:GPUICommandTabRuntimeActions 2026-06-28-15:12:
    GPUI tests are intentionally absent, so preserve this as the production row-count policy instead of retaining unused runtime-action enums or test-gated assertion helpers.
    */
    0
}

pub(crate) fn command_pane_tab_tooltip(
    title: &str,
    delayed_send_remaining_label: Option<&str>,
) -> String {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-17:57:
    Native command tabs keep the normal title tooltip but append "Delayed Send in <remaining>" while a live timer label exists. Use only the visible title plus the runtime countdown label already allowed for tab/body/sidebar timer chrome; do not read command text, terminal content, shell-state JSON, paths, stdout/stderr, or persisted placeholder flags.
    */
    let title = if title.trim().is_empty() {
        COMMAND_PANE_DEFAULT_SESSION_TITLE
    } else {
        title
    };
    delayed_send_remaining_label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| format!("{title}\nDelayed Send in {label}"))
        .unwrap_or_else(|| title.to_string())
}

pub(crate) fn command_pane_tab_context_scoped_close_order() -> [CommandPaneTabCloseScope; 3] {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-14:07:
    Native tab-button context menus order scoped close rows as Close Right, Close Left, then Close Other Tabs. Command-role GPUI menus should match those labels and order while preserving the existing clicked-group close resolution.
    */
    [
        CommandPaneTabCloseScope::CloseRight,
        CommandPaneTabCloseScope::CloseLeft,
        CommandPaneTabCloseScope::CloseOthers,
    ]
}

pub(crate) fn command_pane_tab_context_sleep_order(
    clicked_tab_is_sleeping: bool,
) -> Vec<CommandPaneTabSleepScope> {
    /*
    CDXC:GPUICommandTabContextMenu 2026-06-25-14:27:
    Native tab menus show direct Sleep only for awake clicked tabs, then always show Sleep Right, Sleep Left, and Sleep Other Tabs before the close group. Keep the row order separate from close ordering so sleeping command tabs remain visible while not offering a redundant direct Sleep action.
    */
    let mut scopes = Vec::with_capacity(4);
    if !clicked_tab_is_sleeping {
        scopes.push(CommandPaneTabSleepScope::Sleep);
    }
    scopes.extend([
        CommandPaneTabSleepScope::SleepRight,
        CommandPaneTabSleepScope::SleepLeft,
        CommandPaneTabSleepScope::SleepOthers,
    ]);
    scopes
}

pub(crate) fn command_pane_new_command_control_placement() -> CommandPaneNewCommandControlPlacement
{
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:13:
    macOS command-pane chrome keeps New Terminal inline with the tab run, while the fixed right action cluster is reserved for panel actions such as Pin/Unpin and Minimize/Expand. GPUI should not render New Terminal in that fixed cluster.
    */
    CommandPaneNewCommandControlPlacement::InlineTabRun
}

pub(crate) fn command_pane_tab_add_tooltip() -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:23:
    Native command chrome names the inline plus action New Terminal, even inside the command panel. Keep GPUI's visible tooltip aligned with the macOS tab-add button while the internal model still creates a command-terminal placeholder.
    */
    "New Terminal"
}

pub(crate) fn command_pane_tab_add_icon_path() -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:54:
    Native command-pane New Terminal chrome is the tab-strip add button, not the generic `.newTerminal` titlebar action button. It uses plus symbol chrome with the New Terminal tooltip, so GPUI should render a plus icon rather than the terminal action symbol here.
    */
    COMMAND_ICON_PLUS
}

pub(crate) fn command_pane_panel_mode_controls_visible(expanded_chrome: bool) -> bool {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-12:05:
    Hidden/collapsed command-panel chrome mirrors macOS expand-only panel actions, so Pin/Unpin is visible only in expanded command-pane titlebars. Keep New Terminal as inline tab-run chrome and Expand in the collapsed strip for existing GPUI collapsed-strip creation/open behavior, but do not expose panel mode mutation while hidden.
    */
    COMMAND_PANE_FLOATING_MODE_ENABLED && expanded_chrome
}

pub(crate) fn command_pane_mode_for_current_release(mode: CommandPaneMode) -> CommandPaneMode {
    if COMMAND_PANE_FLOATING_MODE_ENABLED || mode != CommandPaneMode::Floating {
        mode
    } else {
        CommandPaneMode::Pinned
    }
}

pub(crate) fn command_pane_panel_pin_icon_path(mode: CommandPaneMode) -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:40:
    macOS command-panel mode chrome uses pin and pin.slash symbols for Pin/Unpin Commands Panel. GPUI should not expose raw P/U fallback letters when SVG chrome is available.
    */
    match mode {
        CommandPaneMode::Pinned => COMMAND_ICON_PIN_SLASH,
        CommandPaneMode::Floating | CommandPaneMode::Collapsed => COMMAND_ICON_PIN,
    }
}

pub(crate) fn command_pane_panel_visibility_icon_path(expanded: bool) -> &'static str {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:40:
    macOS command-panel visibility chrome uses chevron.down for Minimize Commands Panel and chevron.up for Expand Commands Panel. Keep GPUI on symbol chrome instead of visible v/^ fallback text.
    */
    if expanded {
        COMMAND_ICON_CHEVRON_DOWN
    } else {
        COMMAND_ICON_CHEVRON_UP
    }
}

pub(crate) fn command_pane_bottom_reservation_chrome(
    mode: CommandPaneMode,
) -> Option<CommandPaneBottomReservationChrome> {
    /*
    CDXC:GPUICommandPaneFloating 2026-06-25-18:19:
    Native floating command panels reserve `collapsedCommandsPanelHeight` as a plain black `CommandsPanelChromeView` while the expanded floating panel owns the actual tabs and controls. Render the interactive collapsed strip only for collapsed command-pane mode so floating mode does not duplicate command tabs below the panel.
    */
    match mode {
        CommandPaneMode::Pinned => None,
        CommandPaneMode::Floating => Some(CommandPaneBottomReservationChrome::PlainChrome),
        CommandPaneMode::Collapsed => Some(CommandPaneBottomReservationChrome::CollapsedStrip),
    }
}

pub(crate) fn command_pane_workspace_layout_plan(
    mode: CommandPaneMode,
    has_sessions: bool,
    content_height: f32,
    height_ratio: f32,
    side: GpuiCommandPaneSide,
    content_width: f32,
    width_ratio: f32,
) -> CommandPaneWorkspaceLayoutPlan {
    /*
    CDXC:GPUICommandPaneLayout 2026-06-27-08:32:
    Native TerminalWorkspaceView lays command panels out from session presence and mode: pinned reserves the full command-panel height and pushes the workspace up, floating expanded overlays the panel while reserving only collapsedCommandsPanelHeight as plain bottom chrome, and collapsed renders only the interactive collapsed strip.

    CDXC:GPUICommandPaneLayout 2026-06-27-15:00:
    The command-pane chrome belongs to live command sessions. Once the final command session closes, hide the entire bottom strip so the active workspace reclaims its full height; reopening a command pane still follows the existing session-creation path.

    CDXC:GPUICommandPaneSide 2026-08-16:
    `commandsPanelSide: "right"` only changes the pinned placement: the expanded pane becomes a workspace column sized by `widthRatio`. Floating (release-disabled) and the collapsed footer strip keep their bottom layout so the pane is discoverable from the same place on both sides.
    */
    if !has_sessions {
        return CommandPaneWorkspaceLayoutPlan::Hidden;
    }

    if side == GpuiCommandPaneSide::Right && mode == CommandPaneMode::Pinned {
        return CommandPaneWorkspaceLayoutPlan::PinnedRight {
            panel_width: command_pane_width_for_ratio(width_ratio, content_width),
        };
    }

    let bottom_reservation = command_pane_bottom_reservation_chrome(mode).map(|chrome| {
        CommandPaneWorkspaceBottomReservation {
            chrome,
            height: COMMAND_PANE_STRIP_HEIGHT,
        }
    });

    match mode {
        CommandPaneMode::Pinned => CommandPaneWorkspaceLayoutPlan::Pinned {
            panel_height: command_pane_height_for_ratio(height_ratio, content_height),
        },
        CommandPaneMode::Floating => CommandPaneWorkspaceLayoutPlan::Floating {
            panel_height: command_pane_floating_height_for_ratio(height_ratio, content_height),
            bottom_reservation: bottom_reservation
                .expect("floating command panes reserve plain bottom chrome"),
        },
        CommandPaneMode::Collapsed => CommandPaneWorkspaceLayoutPlan::Collapsed {
            bottom_reservation: bottom_reservation
                .expect("collapsed command panes reserve collapsed bottom chrome"),
        },
    }
}

pub(crate) fn command_pane_control_trailing_padding(expanded_chrome: bool) -> f32 {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-13:47:
    Expanded and collapsed command action clusters keep their rightmost button flush inside the surrounding command chrome. The collapsed strip supplies its separate outer right margin.
    */
    if expanded_chrome {
        COMMAND_PANE_CONTROL_EXPANDED_TRAILING_PADDING
    } else {
        COMMAND_PANE_CONTROL_COLLAPSED_TRAILING_PADDING
    }
}

pub(crate) fn command_pane_tab_left_mouse_up_selects(
    pending_click: Option<CommandPanePendingTabClick>,
    target: CommandPanePendingTabClick,
    command_tab_drag_active: bool,
) -> bool {
    /*
    CDXC:GPUICommandTabSelection 2026-06-25-19:14:
    Left-click command tab activation is a mouse-up commit after a matching mouse-down token. Once GPUI starts a command-tab drag, the pending token is canceled and mouse-up must not mutate the active command selection.
    */
    pending_click == Some(target) && !command_tab_drag_active
}

pub(crate) fn command_pane_tab_left_mouse_up_focuses(
    click_count: usize,
    pending_click: Option<CommandPanePendingTabClick>,
    target: CommandPanePendingTabClick,
    command_tab_drag_active: bool,
    command_pane: &CommandPaneModel,
) -> bool {
    /*
    CDXC:GPUICommandFocusMode 2026-06-25-21:50:
    Native command-pane tabs route double-click mouse-up to Focus instead of the normal selection request, but only for split owners that expose the Focus tab action. Keep the same pending-click and drag gates as single-click selection so double-click Focus cannot fire after drag start, stale mouse-up delivery, collapsed hidden-open tabs, or non-eligible command groups.
    */
    click_count >= 2
        && command_pane_tab_left_mouse_up_selects(pending_click, target, command_tab_drag_active)
        && command_pane.tab_context_allows_focus_mode(target.group_id, target.session_id)
}

pub(crate) fn command_pane_tab_left_mouse_up_finishes_drag(command_tab_drag_active: bool) -> bool {
    /*
    CDXC:GPUICommandTabSelection 2026-06-25-19:21:
    A left mouse-up delivered to a command tab still has to clear any active command-tab drag state after skipping click selection. Native AppKit's tab mouse-up ends the drag/click gesture through the same owner; GPUI must not rely only on the root mouse-up path because the tab handler consumes the event.
    */
    command_tab_drag_active
}

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

pub(crate) fn command_pane_sticky_active_tab_icon_path(
    edge: CommandPaneStickyActiveTabEdge,
) -> &'static str {
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

pub(crate) fn command_pane_empty_titlebar_double_click_creates_new_terminal(
    click_count: usize,
) -> bool {
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
    #[allow(dead_code)] // no caller: group border widths are applied inline in render/command_pane_tabs_core.rs; kept with the border-width chrome model
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
