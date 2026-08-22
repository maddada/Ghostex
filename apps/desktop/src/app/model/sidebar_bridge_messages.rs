// C1 wave-3 re-cluster: sidebar bridge message payloads for workspace terminal focus, rename, and browser tab/create-project events, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).
#![allow(dead_code)]

use crate::*;


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTabSession {
    pub(crate) activity: AgentTerminalActivity,
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) key: GpuiWorkspaceTerminalSessionKey,
    pub(crate) kind: AgentsWorkspaceSessionKind,
    pub(crate) is_generating_first_prompt_title: bool,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) title: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalFocusMessage {
    pub(crate) force_remount: bool,
    pub(crate) placement_target_session_id: Option<String>,
    pub(crate) preferred_interface: GpuiPreferredAgentInterface,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuiPreferredAgentInterface {
    Chat,
    #[default]
    Terminal,
}


impl GpuiPreferredAgentInterface {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(Self::Chat),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarCreateProjectAgentMessage {
    pub(crate) agent_id: String,
    pub(crate) preferred_interface: GpuiPreferredAgentInterface,
    pub(crate) project_id: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarCreateProjectTerminalMessage {
    pub(crate) project_id: String,
}


/// True for sidebar bridge events that act on per-project runtime state, so
/// they must not run ahead of a project switch that is still queued behind the
/// settle window. The listed pass-through events are project-agnostic status,
/// telemetry, and compatibility no-ops; flushing on those would defeat the
/// debounce because they arrive on every presentation publish.
pub(crate) fn gpui_sidebar_bridge_event_must_follow_pending_project_switch(
    event: &cef::SidebarBridgeEvent,
) -> bool {
    !matches!(
        event,
        cef::SidebarBridgeEvent::ActiveProjectContext(_)
            | cef::SidebarBridgeEvent::GxserverPresentationFocusState(_)
            | cef::SidebarBridgeEvent::WorkspaceTerminalFocus(_)
            | cef::SidebarBridgeEvent::SessionCompletionSound(_)
            | cef::SidebarBridgeEvent::SessionStatusIndicators(_)
            | cef::SidebarBridgeEvent::PetOverlayState(_)
            | cef::SidebarBridgeEvent::TitlebarGitMenuState(_)
            | cef::SidebarBridgeEvent::ProjectBoardConversationResponse(_)
            | cef::SidebarBridgeEvent::SourceWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::BrowserWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ProjectWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ManageFileWorkareaOperationRequest(_)
    )
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiBrowserRendererOpenReuse {
    Exact,
    None,
    Similar,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarOpenBrowserUrlMessage {
    pub(crate) url: String,
    pub(crate) reuse: GpuiBrowserRendererOpenReuse,
    pub(crate) from_quick_header: bool,
    pub(crate) project_id: Option<String>,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarBrowserTabFocusMessage {
    pub(crate) project_id: String,
    pub(crate) tab_id: BrowserTabId,
}


/*
CDXC:GPUISidebarRename 2026-07-28:
`command` is a fixed selector, not renderer-provided command text: it may only
be the literal "rename" (default), "name" (Pi), or "title" (Hermes Agent), and
Rust alone turns it into terminal input.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceTerminalRenameCommandKind {
    Name,
    Rename,
    Title,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalRenameCommandMessage {
    pub(crate) command: GpuiWorkspaceTerminalRenameCommandKind,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) title: String,
}


/*
CDXC:GPUIWorkspaceRenameCommand 2026-07-29:
Rename delivery selects the target tab first, so its Ghostty surface may still
be mounting when the command arrives. The bounded timer below re-validates the
same exact target until the surface mounts; it never retargets or falls back.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceRenameCommandDelivery {
    Delivered,
    SurfaceNotMounted,
    TargetInvalid,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalEnterMessage {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiWorkspaceTerminalRenameCommandTarget {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) needs_tab_selection: bool,
}


pub(crate) fn gpui_workspace_terminal_rename_command_target_from_model(
    workspace: &WorkspaceModel,
    local_workspace_session_mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    key: &GpuiLocalWorkspaceSessionKey,
) -> Option<GpuiWorkspaceTerminalRenameCommandTarget> {
    /*
    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    Rename-command target selection is model-only until the final Ghostty owner check: require an existing local gxserver mapping, a Running Agents shell session, and a pane that owns that tab. Sleeping, mounting, restored, popped-out, stale, unmapped, command-pane, and fallback-focused terminals must not become rename targets.
    */
    let shell_session_id = local_workspace_session_mappings.get(key).copied()?;
    if !workspace.session(shell_session_id).is_some_and(|session| {
        session.presentation_state == TerminalSessionPresentationState::Running
    }) {
        return None;
    }
    let pane_id = workspace.pane_id_for_session(shell_session_id)?;
    let active_session_id = workspace
        .find_leaf(pane_id)
        .and_then(|leaf| leaf.tab_group.active_session_id());
    Some(GpuiWorkspaceTerminalRenameCommandTarget {
        pane_id,
        shell_session_id,
        slot_id: AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        },
        needs_tab_selection: active_session_id != Some(shell_session_id),
    })
}
