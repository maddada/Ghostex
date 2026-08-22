// C1 wave-3 re-cluster: Agents workspace tab chrome: lifecycle visual role/tone, chrome signature, and tab icon/status-indicator rendering, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).
#![allow(dead_code)]

use crate::*;


#[derive(Clone, Copy)]
pub(crate) enum WorkspaceTabActionIcon {
    NewTerminal,
    NewBrowser,
    Overflow,
}


/*
CDXC:GPUIAgentsTabChrome 2026-06-22-17:07:
Agents workspace tab chrome is focus-invariant: pane focus may change pane borders and keyboard ownership, but tab-bar visuals derive only from a tab's presentation lifecycle, semantic status, and active membership inside its own tab group.

CDXC:GPUIAgentsTabChrome 2026-06-22-17:27:
Selected Agents tabs use selected chrome even when their terminal lifecycle is sleeping, mounting, failed startup, restored/unmounted, or popped out. Workspace tab fill and title colors mirror the native AppKit tab strip: selected tabs use the active white overlay, and all inactive tabs share the inactive overlay while lifecycle state moves to the trailing status slot or placeholder badge.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceTabLifecycleVisualRole {
    SelectedRunning,
    InactiveRunning,
    SelectedNonRunning,
    InactiveNonRunning,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceTabLifecycleVisualTone {
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) role: WorkspaceTabLifecycleVisualRole,
}


impl WorkspaceTabLifecycleVisualTone {
    pub(crate) fn new(
        presentation_state: TerminalSessionPresentationState,
        active_in_tab_group: bool,
    ) -> Self {
        let role = match (presentation_state.is_running(), active_in_tab_group) {
            (true, true) => WorkspaceTabLifecycleVisualRole::SelectedRunning,
            (true, false) => WorkspaceTabLifecycleVisualRole::InactiveRunning,
            (false, true) => WorkspaceTabLifecycleVisualRole::SelectedNonRunning,
            (false, false) => WorkspaceTabLifecycleVisualRole::InactiveNonRunning,
        };

        Self {
            presentation_state,
            role,
        }
    }

    pub(crate) fn uses_selected_treatment(self) -> bool {
        matches!(
            self.role,
            WorkspaceTabLifecycleVisualRole::SelectedRunning
                | WorkspaceTabLifecycleVisualRole::SelectedNonRunning
        )
    }

    pub(crate) fn uses_inactive_running_treatment(self) -> bool {
        matches!(self.role, WorkspaceTabLifecycleVisualRole::InactiveRunning)
    }

    pub(crate) fn uses_subdued_non_running_treatment(self) -> bool {
        matches!(
            self.role,
            WorkspaceTabLifecycleVisualRole::InactiveNonRunning
        )
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceTabChromeSignature {
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) tab_status: AgentTerminalTabStatus,
    pub(crate) active_in_tab_group: bool,
    pub(crate) lifecycle_visual_tone: WorkspaceTabLifecycleVisualTone,
}


pub(crate) fn workspace_tab_chrome_signature(
    tab_group: &WorkspaceTabGroup,
    session_id: TerminalSessionId,
    session: Option<&TerminalSession>,
) -> WorkspaceTabChromeSignature {
    let presentation_state = session
        .map(|session| session.presentation_state)
        .unwrap_or(TerminalSessionPresentationState::Running);
    let active_in_tab_group = tab_group.active_session_id() == Some(session_id);
    WorkspaceTabChromeSignature {
        presentation_state,
        tab_status: session
            .map(TerminalSession::tab_status)
            .unwrap_or(AgentTerminalTabStatus::Idle),
        active_in_tab_group,
        lifecycle_visual_tone: WorkspaceTabLifecycleVisualTone::new(
            presentation_state,
            active_in_tab_group,
        ),
    }
}


pub(crate) fn workspace_tab_icon_element(
    element_id: impl Into<String>,
    agent_icon: Option<&'static str>,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> AnyElement {
    let element_id = element_id.into();
    if let Some(agent_icon) = agent_icon
        && let Some(icon_path) = workspace_tab_agent_icon_path(agent_icon)
    {
        return workspace_tab_agent_icon_element(element_id, agent_icon, icon_path, visual_tone);
    }
    workspace_tab_terminal_icon_element(element_id, visual_tone)
}


pub(crate) fn workspace_tab_agent_icon_element(
    element_id: impl Into<String>,
    agent_icon: &'static str,
    icon_path: &'static str,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> AnyElement {
    div()
        .id(element_id.into())
        .flex()
        .flex_shrink_0()
        .size(px(WORKSPACE_TAB_AGENT_ICON_SIZE))
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(icon_path)
                .size(px(workspace_tab_agent_svg_size(agent_icon)))
                .text_color(workspace_tab_agent_icon_text_color(agent_icon, visual_tone)),
        )
        .into_any_element()
}


pub(crate) fn workspace_tab_terminal_icon_element(
    element_id: impl Into<String>,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> AnyElement {
    let presentation_state = visual_tone.presentation_state;
    div()
        .id(element_id.into())
        .relative()
        .flex_shrink_0()
        .w(px(WORKSPACE_TAB_ICON_WIDTH))
        .h(px(11.0))
        .rounded(px(2.0))
        .border_1()
        .border_color(if visual_tone.uses_selected_treatment() {
            workspace_tab_terminal_icon_active_color(presentation_state)
        } else {
            workspace_tab_terminal_icon_inactive_color(presentation_state)
        })
        .bg(if visual_tone.uses_selected_treatment() {
            workspace_tab_terminal_icon_active_background(presentation_state)
        } else {
            workspace_tab_terminal_icon_inactive_background(presentation_state)
        })
        .child(
            div()
                .absolute()
                .left(px(3.0))
                .top(px(3.0))
                .w(px(3.0))
                .h(px(1.0))
                .bg(workspace_tab_terminal_icon_glyph_color(visual_tone)),
        )
        .child(
            div()
                .absolute()
                .left(px(6.0))
                .top(px(6.0))
                .w(px(5.0))
                .h(px(1.0))
                .bg(workspace_tab_terminal_icon_glyph_color(visual_tone)),
        )
        .into_any_element()
}


pub(crate) fn workspace_tab_status_indicator_visible(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
    tab_hovered: bool,
) -> bool {
    visual_tone.presentation_state.is_running()
        && !tab_hovered
        && !matches!(tab_status, AgentTerminalTabStatus::Idle)
}


pub(crate) fn workspace_tab_status_title_trailing_reserved_width(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
) -> f32 {
    if visual_tone.presentation_state == TerminalSessionPresentationState::Sleeping {
        WORKSPACE_TAB_SLEEP_TITLE_RESERVED_WIDTH
    } else if workspace_tab_status_indicator_visible(visual_tone, tab_status, false) {
        WORKSPACE_TAB_STATUS_TITLE_RESERVED_WIDTH
    } else {
        0.0
    }
}


pub(crate) fn workspace_tab_status_indicator_element(
    element_id: impl Into<String>,
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
) -> AnyElement {
    div()
        .id(element_id.into())
        .absolute()
        .right(px(WORKSPACE_TAB_STATUS_INDICATOR_TRAILING_PADDING))
        .top(px((WORKSPACE_TAB_BAR_HEIGHT
            - WORKSPACE_TAB_STATUS_INDICATOR_SIZE)
            / 2.0))
        .size(px(WORKSPACE_TAB_STATUS_INDICATOR_SIZE))
        .rounded_full()
        .bg(workspace_tab_status_dot_color(visual_tone, tab_status))
        .into_any_element()
}


pub(crate) fn workspace_tab_sleep_icon_visible(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_hovered: bool,
) -> bool {
    visual_tone.presentation_state == TerminalSessionPresentationState::Sleeping && !tab_hovered
}


pub(crate) fn workspace_tab_sleep_icon_color() -> Hsla {
    rgb(0xdbdbdb).opacity(0.42).into()
}
