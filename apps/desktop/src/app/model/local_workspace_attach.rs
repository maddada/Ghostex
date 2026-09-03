// C1 wave-3 re-cluster: local workspace session-key/attach/lifecycle plumbing, including the local and remote workspace session mapping persistence helpers, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GpuiLocalWorkspaceSessionKey {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceAttachIntent {
    Attach,
    Wake,
}

impl GpuiLocalWorkspaceAttachIntent {
    pub(crate) fn rpc_path(self) -> &'static str {
        match self {
            Self::Attach => "/api/attachSessionMetadata",
            Self::Wake => "/api/wakeSession",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceAttachOrigin {
    SidebarFocus,
    SurfacedRestore,
    WakeRecovery,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarWorkspaceTerminalLifecycleResultMessage {
    pub(crate) ok: bool,
    pub(crate) request_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceLifecycleAction {
    Close,
    Sleep,
    Wake,
}

impl GpuiLocalWorkspaceLifecycleAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Sleep => "sleep",
            Self::Wake => "wake",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiLocalWorkspaceLifecycleMutationKind {
    DirectClose,
    ScopedClose,
    DirectSleep,
    DirectWake,
    ScopedSleep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiLocalWorkspaceLifecycleRequest {
    pub(crate) action: GpuiLocalWorkspaceLifecycleAction,
    pub(crate) confirmed_close_slot_id: Option<AgentsTerminalBodyMountSlotId>,
    pub(crate) mutation_kind: GpuiLocalWorkspaceLifecycleMutationKind,
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) replacement_shell_session_id: Option<TerminalSessionId>,
    pub(crate) shell_session_id: TerminalSessionId,
}

pub(crate) fn gpui_local_workspace_lifecycle_request_is_pending(
    requests: &HashMap<u64, GpuiLocalWorkspaceLifecycleRequest>,
    request: &GpuiLocalWorkspaceLifecycleRequest,
) -> bool {
    /*
    CDXC:Workarea 2026-06-27-00:33:
    Pending mapped Sleep/Wake requests must de-dupe only exact native mutations. Direct/scoped Sleep, replacement focus, and pane origin carry different macOS tab semantics, so session/action-only de-dupe can apply the wrong UX when a second request races an async SidebarApp ack. Close is local-first and never enters this pending set.
    */
    requests.values().any(|pending| pending == request)
}

impl From<&GpuiSidebarWorkspaceTerminalFocusMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalFocusMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}

impl From<&GpuiSidebarWorkspaceTerminalRenameCommandMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalRenameCommandMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}

impl From<&GpuiSidebarWorkspaceTerminalEnterMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalEnterMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}

pub(crate) fn local_workspace_session_mappings_to_shell_state_json(
    mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    workspace: &WorkspaceModel,
) -> serde_json::Value {
    let mut entries = mappings
        .iter()
        .filter(|(_, shell_session_id)| workspace.has_session(**shell_session_id))
        .map(|(key, shell_session_id)| {
            (
                shell_session_id.0,
                key.project_id.as_str(),
                key.session_id.as_str(),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_unstable();
    serde_json::Value::Array(
        entries
            .into_iter()
            .map(|(shell_session_id, project_id, session_id)| {
                serde_json::json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "shellSessionId": shell_session_id,
                })
            })
            .collect(),
    )
}

pub(crate) fn remote_workspace_session_mappings_to_shell_state_json(
    mappings: &HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    workspace: &WorkspaceModel,
    workspace_project_id: Option<&str>,
) -> serde_json::Value {
    let mut entries = mappings
        .iter()
        .filter_map(|(key, shell_session_id)| {
            let scoped_project_id = gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            );
            (workspace_project_id == Some(scoped_project_id.as_str())
                && workspace.has_session(*shell_session_id))
            .then(|| {
                (
                    shell_session_id.0,
                    key.remote_machine_id.as_str(),
                    key.project_id.as_str(),
                    key.session_id.as_str(),
                )
            })
        })
        .collect::<Vec<_>>();
    entries.sort_unstable();
    serde_json::Value::Array(
        entries
            .into_iter()
            .map(
                |(shell_session_id, remote_machine_id, project_id, session_id)| {
                    serde_json::json!({
                        "remoteMachineId": remote_machine_id,
                        "projectId": project_id,
                        "sessionId": session_id,
                        "shellSessionId": shell_session_id,
                    })
                },
            )
            .collect(),
    )
}

pub(crate) fn local_workspace_session_mappings_from_shell_state(
    value: &serde_json::Value,
    workspace: &WorkspaceModel,
) -> Option<HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>> {
    let entries = value.as_array()?;
    if entries.len() > GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX {
        return None;
    }
    let mut mappings = HashMap::with_capacity(entries.len());
    let mut mapped_shell_session_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object()?;
        if object.len() != 3
            || !object.contains_key("projectId")
            || !object.contains_key("sessionId")
            || !object.contains_key("shellSessionId")
        {
            return None;
        }
        let project_id = json_string_field(object, "projectId")?.trim();
        let session_id = json_string_field(object, "sessionId")?.trim();
        let shell_session_id = TerminalSessionId(json_u64_field(object, "shellSessionId")?);
        if !gpui_remote_sidebar_project_id_allowed(project_id)
            || !gpui_sidebar_local_gxserver_session_id_allowed(session_id)
            || !workspace.has_session(shell_session_id)
        {
            return None;
        }
        let key = GpuiLocalWorkspaceSessionKey {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        };
        if mappings.insert(key, shell_session_id).is_some()
            || !mapped_shell_session_ids.insert(shell_session_id)
        {
            return None;
        }
    }
    Some(mappings)
}

pub(crate) fn remote_workspace_session_mappings_from_shell_state(
    value: &serde_json::Value,
    workspace: &WorkspaceModel,
    workspace_project_id: Option<&str>,
) -> Option<HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>> {
    let entries = value.as_array()?;
    if entries.len() > GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX {
        return None;
    }
    let mut mappings = HashMap::with_capacity(entries.len());
    let mut mapped_shell_session_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object()?;
        if object.len() != 4
            || !object.contains_key("remoteMachineId")
            || !object.contains_key("projectId")
            || !object.contains_key("sessionId")
            || !object.contains_key("shellSessionId")
        {
            return None;
        }
        let remote_machine_id = json_string_field(object, "remoteMachineId")?.trim();
        let project_id = json_string_field(object, "projectId")?.trim();
        let session_id = json_string_field(object, "sessionId")?.trim();
        let shell_session_id = TerminalSessionId(json_u64_field(object, "shellSessionId")?);
        let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
        let scoped_project_id =
            gpui_remote_scoped_project_id(remote_machine_id.as_str(), project_id);
        if !gpui_remote_sidebar_project_id_allowed(project_id)
            || !gpui_remote_sidebar_session_id_allowed(session_id)
            || workspace_project_id != Some(scoped_project_id.as_str())
            || !workspace.has_session(shell_session_id)
        {
            return None;
        }
        let key = GpuiRemoteAttachSessionKey {
            remote_machine_id,
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        };
        if mappings.insert(key, shell_session_id).is_some()
            || !mapped_shell_session_ids.insert(shell_session_id)
        {
            return None;
        }
    }
    Some(mappings)
}
