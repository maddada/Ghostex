use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::{
    GXSERVER_LOCAL_API_HOST, GXSERVER_LOCAL_API_PORT, GXSERVER_PRODUCT, GXSERVER_PROTOCOL_VERSION,
    GXSERVER_REMOTE_API_HOST, GXSERVER_REMOTE_API_PORT,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiPermission {
    FullLocal,
    RemoteAllowed,
    RemoteBlocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Http,
    WebSocket,
}

#[derive(Clone, Debug)]
pub struct EndpointDescriptor {
    pub path: String,
    pub permission: ApiPermission,
    pub requires_auth: bool,
    pub requires_protocol_version: bool,
    pub transport: Transport,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListenerKind {
    Local,
    Remote,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerAuthConfig {
    pub mode: String,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ListenerAuthConfig>,
    pub enabled: bool,
    pub host: String,
    pub kind: String,
    pub port: u16,
}

impl ListenerConfig {
    pub fn local_default() -> Self {
        Self::local_with_port(GXSERVER_LOCAL_API_PORT)
    }

    pub fn local_with_port(port: u16) -> Self {
        Self {
            auth: None,
            enabled: true,
            host: GXSERVER_LOCAL_API_HOST.to_string(),
            kind: "local".to_string(),
            port,
        }
    }

    pub fn remote_default() -> Self {
        Self {
            auth: Some(ListenerAuthConfig {
                mode: "bearerToken".to_string(),
                required: true,
            }),
            enabled: false,
            host: GXSERVER_REMOTE_API_HOST.to_string(),
            kind: "remote".to_string(),
            port: GXSERVER_REMOTE_API_PORT,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenersConfig {
    pub local: ListenerConfig,
    pub remote: ListenerConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatus {
    pub applied_migrations: Vec<String>,
    pub current_version: usize,
    pub state_db_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_imports: Option<MigrationStateImports>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStateImports {
    pub legacy_macos_state: LegacyMacosStateImportStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMacosStateImportStatus {
    pub completed_at: String,
    pub id: String,
    pub logs_imported: LegacyMacosLogsImportStatus,
    pub projects_imported: usize,
    pub sessions_imported: usize,
    pub skipped_reason: String,
    pub source_files_read: Vec<String>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMacosLogsImportStatus {
    pub files_read: usize,
    pub malformed_line_count: usize,
    pub migrated_line_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetadata {
    pub build_identity: String,
    pub pid: u32,
    pub port: u16,
    pub protocol_version: u64,
    pub server_id: String,
    pub started_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalHealthResponse {
    pub ok: bool,
    pub product: String,
    pub protocol_version: u64,
    pub version: String,
}

impl MinimalHealthResponse {
    pub fn new(version: &str) -> Self {
        Self {
            ok: true,
            product: GXSERVER_PRODUCT.to_string(),
            protocol_version: GXSERVER_PROTOCOL_VERSION,
            version: version.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHealthResponse {
    pub ok: bool,
    pub product: String,
    pub protocol_version: u64,
    pub version: String,
    pub build_identity: String,
    pub capabilities: Vec<String>,
    pub listeners: ListenersConfig,
    pub migration: MigrationStatus,
    pub pid: u32,
    pub port: u16,
    pub server_id: String,
    pub started_at: String,
    pub tools: Vec<ToolCapabilityStatus>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCapabilityStatus {
    pub availability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_paths: Option<Vec<String>>,
    pub capability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub tool: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<ServerHealthResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RuntimeMetadata>,
    pub message: String,
    pub ok: bool,
    pub product: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSuccessResponse {
    pub ok: bool,
    pub product: String,
    pub protocol_version: u64,
    pub request_id: String,
    pub result: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcErrorResponse {
    pub error: String,
    pub message: String,
    pub ok: bool,
    pub product: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

pub fn rpc_success(request_id: impl Into<String>, result: Value) -> RpcSuccessResponse {
    RpcSuccessResponse {
        ok: true,
        product: GXSERVER_PRODUCT.to_string(),
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        request_id: request_id.into(),
        result,
    }
}

pub fn rpc_error(
    error: impl Into<String>,
    message: impl Into<String>,
    request_id: Option<String>,
) -> RpcErrorResponse {
    RpcErrorResponse {
        error: error.into(),
        message: message.into(),
        ok: false,
        product: GXSERVER_PRODUCT.to_string(),
        protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
        request_id,
    }
}

pub fn protocol_mismatch_error(
    actual_protocol_version: Option<Value>,
    request_id: Option<String>,
) -> RpcErrorResponse {
    let actual = actual_protocol_version
        .map(|value| match value {
            Value::String(text) => text,
            other => other.to_string(),
        })
        .unwrap_or_else(|| "undefined".to_string());
    rpc_error(
        "protocolMismatch",
        format!(
            "gxserver protocol mismatch. Expected protocol {GXSERVER_PROTOCOL_VERSION}, got {actual}. Update Ghostex and gxserver so their protocol versions match."
        ),
        request_id,
    )
}

/*
CDXC:GxserverProtocol 2026-06-14-20:37:
Phase 1 mirrors the TypeScript endpoint catalog so auth, method, protocol, and remote-listener gates run before any Rust milestone-specific handler decides whether an endpoint is implemented.
*/
pub fn endpoint_for(path: &str) -> Option<EndpointDescriptor> {
    let remote_allowed = |path| {
        descriptor(
            path,
            ApiPermission::RemoteAllowed,
            true,
            true,
            Transport::Http,
        )
    };
    let remote_blocked = |path| {
        descriptor(
            path,
            ApiPermission::RemoteBlocked,
            true,
            true,
            Transport::Http,
        )
    };
    let full_local = |path| descriptor(path, ApiPermission::FullLocal, true, true, Transport::Http);
    Some(match path {
        "/api/health" => descriptor(
            path,
            ApiPermission::RemoteAllowed,
            false,
            false,
            Transport::Http,
        ),
        "/api/health/server" => remote_allowed(path),
        "/api/events" => descriptor(
            path,
            ApiPermission::RemoteAllowed,
            true,
            true,
            Transport::WebSocket,
        ),
        "/api/control/stop" | "/api/control/stopAll" => remote_blocked(path),
        "/api/readAgentSettings"
        | "/api/updateAgentSettings"
        | "/api/ingestAgentHookEvent"
        | "/api/createSession"
        | "/api/createAgentSession"
        | "/api/forkSession"
        | "/api/readAgentLaunchPlan"
        | "/api/readAgentResumePlan"
        | "/api/requestSessionRename"
        | "/api/cancelFirstPromptAutoTitle"
        | "/api/ingestSessionStateEvent"
        | "/api/ingestTerminalTitleEvent"
        | "/api/updateAgentActivity"
        | "/api/readPresentationSnapshot"
        | "/api/searchSessions"
        | "/api/listPreviousSessions"
        | "/api/transitionSession"
        | "/api/sleepSession"
        | "/api/wakeSession"
        | "/api/startSessionProvider"
        | "/api/killSession"
        | "/api/probeSessionProvider"
        | "/api/listSessions"
        | "/api/removeSession"
        | "/api/readSessionText"
        | "/api/sendSessionText"
        | "/api/sendSessionMessage"
        | "/api/sendSessionEnter"
        | "/api/focusSession"
        | "/api/dispatchRendererCommand"
        | "/api/attachSessionMetadata"
        | "/api/createProject"
        | "/api/updateProject"
        | "/api/listProjects"
        | "/api/readProjectStatus"
        | "/api/addProjectPath"
        | "/api/removeProject"
        | "/api/deleteWorktreeProject"
        | "/api/updateSession"
        | "/api/updateSessionOrder"
        | "/api/runGitAction"
        | "/api/runGitHubAction"
        | "/api/runWorktreeAction"
        | "/api/runProjectSetupCommand"
        | "/api/runBeadsAction"
        | "/api/previewRepositoryClone"
        | "/api/startRepositoryClone"
        | "/api/readRepositoryCloneJob"
        | "/api/cancelRepositoryCloneJob"
        | "/api/browseProjectDirectories" => remote_allowed(path),
        /*
        CDXC:AgentHooks 2026-06-19-14:15:
        Hook read/install/uninstall endpoints inspect or mutate user-local provider config files, so Rust keeps the TypeScript contract as full-local HTTP APIs requiring auth and protocol-version gates.
        */
        "/api/readAgentHookStatus"
        | "/api/installAgentHooks"
        | "/api/uninstallAgentHooks"
        | "/api/readAgentSkillStatus"
        | "/api/installAgentSkills"
        | "/api/queryLogs" => full_local(path),
        "/api/resolveGitRootForPath"
        | "/api/updateAuth"
        | "/api/updateListenerConfig"
        | "/api/installTool"
        | "/api/browseFilesystem"
        | "/api/destructiveAdminAction" => remote_blocked(path),
        _ => return None,
    })
}

fn descriptor(
    path: &str,
    permission: ApiPermission,
    requires_auth: bool,
    requires_protocol_version: bool,
    transport: Transport,
) -> EndpointDescriptor {
    EndpointDescriptor {
        path: path.to_string(),
        permission,
        requires_auth,
        requires_protocol_version,
        transport,
    }
}

pub fn is_remote_endpoint_allowed(listener_kind: ListenerKind, permission: ApiPermission) -> bool {
    matches!(listener_kind, ListenerKind::Local)
        || matches!(permission, ApiPermission::RemoteAllowed)
}
