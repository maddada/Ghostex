// C1 wave-3 re-cluster: keyboard ownership/routing targets, workspace focus direction, and spatial/render-order focus scoring, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellFocusTarget {
    /*
    CDXC:FocusRouting 2026-06-22-22:59:
    The GPUI shell needs explicit surface ownership before full keyboard/mouse delivery exists. Track which shell surface owns keyboard actions so tab cycling, Cmd-W close, command-pane F12 focus, project-editor companion close, and runtime Agents Ghostty surface focus mirror the macOS workspace without adding native hit-test routing or runtime teardown.

    CDXC:FocusRouting 2026-06-22-07:54:
    The command pane must remember the last valid non-command workspace/editor focus before it takes keyboard ownership, then restore that focus when command chrome or the final command placeholder hides the pane. Store only the same enum/id focus metadata already used by shell-state persistence.

    CDXC:FocusRouting 2026-06-27-09:42:
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
    TitlebarExtensionPopup,
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
CDXC:Hotkeys 2026-07-24:
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
    Minimize,
}

pub(crate) fn command_pane_palette_open_decision(
    command_pane_expanded: bool,
    shell_focus: ShellFocusTarget,
) -> CommandPanePaletteOpenDecision {
    /*
    Open Commands Panel and F12 share one open/focus/minimize contract.
    Hidden panels open through the normal default-height path. Visible panels
    that do not already own shell focus become active. When the command pane
    is already the focused surface (click or a previous F12), the same hotkey
    minimizes it; the next press expands it again.
    */
    if !command_pane_expanded {
        CommandPanePaletteOpenDecision::OpenAndFocus
    } else if shell_focus == ShellFocusTarget::CommandPane {
        CommandPanePaletteOpenDecision::Minimize
    } else {
        CommandPanePaletteOpenDecision::FocusVisible
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
    pub(crate) fn from_command_palette_directional_focus_action_id(
        action_id: &str,
    ) -> Option<Self> {
        /*
        CDXC:CommandPalette 2026-06-26-07:33:
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

pub(crate) fn command_pane_focused_split_axis(
    direction: FocusedTerminalSplitDirection,
) -> WorkspaceSplitAxis {
    /*
    CDXC:FocusMode 2026-06-25-16:05:
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

pub(crate) fn pane_focus_bounds_from_child_bounds(
    child_bounds: &[Bounds<Pixels>],
) -> Option<Bounds<Pixels>> {
    let mut bounds_iter = child_bounds
        .iter()
        .filter(|bounds| bounds.size.width.as_f32() > 1.0 && bounds.size.height.as_f32() > 1.0);
    let first = bounds_iter.next()?;
    let mut left = first.origin.x.as_f32();
    let mut top = first.origin.y.as_f32();
    let mut right = left + first.size.width.as_f32();
    let mut bottom = top + first.size.height.as_f32();

    for bounds in bounds_iter {
        let child_left = bounds.origin.x.as_f32();
        let child_top = bounds.origin.y.as_f32();
        left = left.min(child_left);
        top = top.min(child_top);
        right = right.max(child_left + bounds.size.width.as_f32());
        bottom = bottom.max(child_top + bounds.size.height.as_f32());
    }

    let width = right - left;
    let height = bottom - top;
    if width <= 1.0 || height <= 1.0 {
        return None;
    }

    Some(Bounds::from_corners(
        gpui::point(px(left), px(top)),
        gpui::point(px(right), px(bottom)),
    ))
}

pub(crate) fn focus_bounds_center(bounds: Bounds<Pixels>) -> (f32, f32) {
    (
        bounds.origin.x.as_f32() + bounds.size.width.as_f32() / 2.0,
        bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0,
    )
}

pub(crate) fn spatial_focus_score(
    current_bounds: Bounds<Pixels>,
    candidate_bounds: Bounds<Pixels>,
    direction: WorkspaceFocusDirection,
) -> Option<(f32, f32, f32)> {
    let (current_x, current_y) = focus_bounds_center(current_bounds);
    let (candidate_x, candidate_y) = focus_bounds_center(candidate_bounds);
    let delta_x = candidate_x - current_x;
    let delta_y = candidate_y - current_y;

    let (primary_distance, secondary_distance) = match direction {
        WorkspaceFocusDirection::Left if delta_x < -SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (-delta_x, delta_y.abs())
        }
        WorkspaceFocusDirection::Right if delta_x > SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (delta_x, delta_y.abs())
        }
        WorkspaceFocusDirection::Up if delta_y < -SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (-delta_y, delta_x.abs())
        }
        WorkspaceFocusDirection::Down if delta_y > SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (delta_y, delta_x.abs())
        }
        _ => return None,
    };
    let squared_distance = delta_x.mul_add(delta_x, delta_y * delta_y);

    Some((primary_distance, secondary_distance, squared_distance))
}

pub(crate) fn nearest_spatial_focus_target(
    current_bounds: Bounds<Pixels>,
    current_target: SpatialFocusTarget,
    direction: WorkspaceFocusDirection,
    candidates: &[FocusCandidate],
) -> Option<SpatialFocusTarget> {
    candidates
        .iter()
        .filter(|candidate| candidate.target != current_target)
        .filter_map(|candidate| {
            spatial_focus_score(current_bounds, candidate.bounds, direction).map(
                |(primary_distance, secondary_distance, squared_distance)| {
                    (
                        candidate.target,
                        candidate.order,
                        primary_distance,
                        secondary_distance,
                        squared_distance,
                    )
                },
            )
        })
        .min_by(|left, right| {
            left.2
                .total_cmp(&right.2)
                .then_with(|| left.3.total_cmp(&right.3))
                .then_with(|| left.4.total_cmp(&right.4))
                .then_with(|| left.1.cmp(&right.1))
        })
        .map(|(target, _, _, _, _)| target)
}

pub(crate) fn render_order_focus_target(
    targets: &[SpatialFocusTarget],
    current_target: Option<SpatialFocusTarget>,
    direction: WorkspaceFocusDirection,
) -> Option<SpatialFocusTarget> {
    if targets.is_empty() {
        return None;
    }

    if let Some(current_target) = current_target {
        let current_index = targets
            .iter()
            .position(|target| *target == current_target)
            .unwrap_or(0);
        if direction.uses_previous_order_fallback() {
            current_index
                .checked_sub(1)
                .and_then(|index| targets.get(index).copied())
        } else {
            targets.get(current_index + 1).copied()
        }
    } else if direction.uses_previous_order_fallback() {
        targets.last().copied()
    } else {
        targets.first().copied()
    }
}

pub(crate) fn workspace_render_order_focus_targets(
    pane_ids: Vec<WorkspacePaneId>,
    command_is_expanded: bool,
    command_has_sessions: bool,
    command_group_ids: Vec<CommandPaneGroupId>,
) -> Vec<SpatialFocusTarget> {
    let mut targets = pane_ids
        .into_iter()
        .map(SpatialFocusTarget::AgentsPane)
        .collect::<Vec<_>>();
    /*
    CDXC:CommandPane 2026-06-25-23:35:
    Render-order keyboard fallback must target the same live expanded command groups as spatial focus. Do not append a generic command-pane target for collapsed strips, empty panels, or stored sessions that no longer belong to a rendered command group.
    */
    targets.extend(command_pane_render_order_focus_targets(
        command_is_expanded,
        command_has_sessions,
        command_group_ids,
    ));
    targets
}

pub(crate) fn command_pane_render_order_focus_targets(
    is_expanded: bool,
    has_sessions: bool,
    command_group_ids: Vec<CommandPaneGroupId>,
) -> Vec<SpatialFocusTarget> {
    if !is_expanded || !has_sessions {
        return Vec::new();
    }

    command_group_ids
        .into_iter()
        .map(SpatialFocusTarget::CommandPaneGroup)
        .collect()
}

pub(crate) fn project_editor_render_order_focus_targets_for_state(
    mode: TitlebarMode,
    left_companion_visible: bool,
    browser_is_awake: bool,
    browser_pane_ids: Vec<BrowserPaneId>,
    command_is_expanded: bool,
    command_has_sessions: bool,
    command_group_ids: Vec<CommandPaneGroupId>,
) -> Vec<SpatialFocusTarget> {
    let mut targets = Vec::new();
    if left_companion_visible {
        targets.push(SpatialFocusTarget::ProjectEditorCompanion(mode));
    }

    if mode == TitlebarMode::Browser && browser_is_awake {
        if browser_pane_ids.is_empty() {
            targets.push(SpatialFocusTarget::ProjectEditorSurface(mode));
        } else {
            targets.extend(
                browser_pane_ids
                    .into_iter()
                    .map(SpatialFocusTarget::BrowserPane),
            );
        }
    } else {
        targets.push(SpatialFocusTarget::ProjectEditorSurface(mode));
    }

    targets.extend(command_pane_render_order_focus_targets(
        command_is_expanded,
        command_has_sessions,
        command_group_ids,
    ));

    targets
}
