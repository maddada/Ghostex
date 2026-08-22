// C1 wave-3 re-cluster: command pane id types, hover-tab state, resize-hover targets, and the sticky active-tab edge, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).
#![allow(dead_code)]

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandSessionId(pub(crate) u64);


pub(crate) fn gpui_command_session_external_id(session_id: CommandSessionId) -> String {
    /*
    CDXC:GPUICommandPaneBridge 2026-06-27-07:05:
    SidebarApp and shared app-modal command-session payloads use canonical `G{u64}` bridge ids so GPUI matches native local session id shape. Keep `CommandSessionId` numeric inside the Rust model and shell/layout persistence, and do not emit legacy numeric strings across the external command-pane bridge.
    */
    format!("G{}", session_id.0)
}


pub(crate) fn gpui_command_session_id_from_external_id(value: &str) -> Option<CommandSessionId> {
    /*
    CDXC:GPUICommandPaneBridge 2026-06-27-07:05:
    External command-pane ids are accepted only as uppercase `G` plus a positive decimal integer. Reject raw numeric strings, lowercase prefixes, empty ids, malformed suffixes, and zero instead of falling back to legacy numeric parsing.
    */
    let numeric = value.strip_prefix('G')?;
    if numeric.is_empty() || !numeric.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let id = numeric.parse::<u64>().ok()?;
    (id > 0).then_some(CommandSessionId(id))
}


/// Agents workspace sessions cross the app-modal bridge as `GW{u64}` ids so
/// Delayed Send payloads cannot collide with `G{u64}` command-pane ids or
/// gxserver session ids.
pub(crate) fn gpui_agents_session_external_id(session_id: TerminalSessionId) -> String {
    format!("GW{}", session_id.0)
}


pub(crate) fn gpui_agents_session_id_from_external_id(value: &str) -> Option<TerminalSessionId> {
    let numeric = value.strip_prefix("GW")?;
    if numeric.is_empty() || !numeric.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let id = numeric.parse::<u64>().ok()?;
    (id > 0).then_some(TerminalSessionId(id))
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandPaneGroupId(pub(crate) u64);


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneHoverTab {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceHoverTab {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserHoverTab {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) tab_id: BrowserTabId,
}


/*
CDXC:GPUICommandPaneResize 2026-06-25-13:19:
Resize hover affordance is runtime-only chrome owned by the exact rail under the pointer: the command-panel rail or one command split rail. Do not persist it into command-pane layout state or infer it from drag state.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneResizeHoverTarget {
    PanelRail,
    Split(CommandPaneSplitId),
}


pub(crate) fn clear_command_resize_hover_state_fields(
    hovering: &mut Option<CommandPaneResizeHoverTarget>,
    visible: &mut Option<CommandPaneResizeHoverTarget>,
    epoch: &mut u64,
) -> bool {
    /*
    CDXC:GPUICommandPaneResize 2026-06-27-03:12:
    Native command-panel resize cursors are refreshed when the rail gesture ends, resets, or disappears. GPUI mirrors that by explicitly clearing runtime resize hover chrome and invalidating delayed hover timers whenever command resize ownership ends, without persisting or mutating layout state.
    */
    if hovering.is_none() && visible.is_none() {
        return false;
    }

    *hovering = None;
    *visible = None;
    *epoch = epoch.wrapping_add(1);
    true
}


pub(crate) fn clear_command_resize_hover_state_fields_if_command_pane_hidden(
    command_pane_has_sessions: bool,
    hovering: &mut Option<CommandPaneResizeHoverTarget>,
    visible: &mut Option<CommandPaneResizeHoverTarget>,
    epoch: &mut u64,
) -> bool {
    /*
    CDXC:GPUICommandPaneResize 2026-06-27-03:16:
    Final command-tab removal hides the command panel just like explicit minimize/collapse. Clear runtime resize hover chrome only after the command pane becomes empty so ordinary tab close, scoped close, confirmed close, and process-exit cleanup keep hover affordances while the panel remains visible.

    CDXC:GPUICommandPaneResize 2026-06-27-07:23:
    User/direct command-tab closes, sidebar Action clears, and scoped tab closes can remove the final command session without going through the explicit minimize control. They must invalidate resize-hover cursor chrome at the successful model-removal boundary, while non-final closes preserve hover because the panel still has a live rail.
    */
    if command_pane_has_sessions {
        return false;
    }

    clear_command_resize_hover_state_fields(hovering, visible, epoch)
}


/*
CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
The command sticky active-tab proxy is runtime-only tab-strip navigation chrome. It appears at the edge where the selected command tab is clipped and never enters command-pane persistence or tab identity state.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandPaneStickyActiveTabEdge {
    Leading,
    Trailing,
}


impl CommandPaneStickyActiveTabEdge {
    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            CommandPaneStickyActiveTabEdge::Leading => "leading",
            CommandPaneStickyActiveTabEdge::Trailing => "trailing",
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandPaneSplitId(pub(crate) u64);
