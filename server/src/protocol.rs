use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::{
    GXSERVER_LOCAL_API_HOST, GXSERVER_LOCAL_API_PORT, GXSERVER_PRODUCT, GXSERVER_PROTOCOL_VERSION,
    GXSERVER_REMOTE_API_HOST, GXSERVER_REMOTE_API_PORT,
};
use crate::portless::PortlessStatusPayload;

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
    /*
    CDXC:ServerDaemon 2026-06-22-05:10:
    Migration status is a storage wire contract, not only an internal Rust shape. Keep legacy import detail fields optional so `notRun`, skipped, completed, and TypeScript-created state.db metadata can serialize with the same field presence as the TypeScript daemon.
    */
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs_imported: Option<LegacyMacosLogsImportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects_imported: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions_imported: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_files_read: Option<Vec<String>>,
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
    pub portless: PortlessStatusPayload,
    pub port: u16,
    pub server_id: String,
    pub started_at: String,
    pub tools: Vec<ToolCapabilityStatus>,
    /// Windows only: how this daemon was started (`platform/launch_context.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_context: Option<LaunchContextReport>,
}

/// CDXC:PlatformSupport 2026-10-04 SEE-ALSO:
/// The desktop app reads `limited` and `background` to replace a Windows gxserver started from SSH or session 0 (`gpui_local_gxserver_limited_launch` in apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs), and `gxserver start` reads them to replace it from a desktop hand-off.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchContextReport {
    /// Started in session 0 or with a network logon (SSH).
    pub background: bool,
    /// `background`, or RedirectionGuard is enforced: agents under it cannot use junction-based tools.
    pub limited: bool,
    pub network_logon: Option<bool>,
    pub redirection_trust_enforced: Option<bool>,
    pub restricted_token: bool,
    pub windows_session_id: Option<u32>,
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
        .map(js_string_for_protocol_version)
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
CDXC:ServerApi 2026-06-22-04:10:
Protocol-mismatch messages are part of the client contract. TypeScript reports `String(actualProtocolVersion)`, so Rust must preserve JavaScript-like stringification for non-scalar JSON values instead of leaking serde JSON syntax into update guidance.
*/
fn js_string_for_protocol_version(value: Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => {
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(value) => value,
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::Null => String::new(),
                other => js_string_for_protocol_version(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/*
CDXC:ServerApi 2026-06-14-20:37:
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
        /*
        CDXC:RemoteMachines 2026-08-16:
        Remote GPUI project-header terminal creation uses the daemon-owned
        atomic workspace-terminal endpoint so the new session, provider, and
        attach plan are created as one operation. It carries only a trusted
        project id plus the bounded prompt-editor selector and is the atomic
        equivalent of the already remote-allowed createSession and
        startSessionProvider operations, so blocking it makes the native
        sidebar action fail before a terminal can be created.
        */
        | "/api/createWorkspaceTerminal"
        | "/api/createAgentSession"
        | "/api/relocateProject"
        | "/api/forkSession"
        /*
        CDXC:Drafts 2026-08-28:
        Switching a draft's agent is remote-allowed for the same reason
        createAgentSession and startSessionProvider above are: the agent CLI
        runs on the machine that owns the project, so a client looking at a
        remote machine has to ask that machine. It carries only bounded project
        / session / agent ids the daemon itself published.
        */
        | "/api/switchDraftAgent"
        // CDXC:AgentBox 2026-10-01 WHY: same reasoning as switchDraftAgent above; where a draft
        // runs is decided by the daemon that owns the row and runs its CLI.
        | "/api/draftRunLocation"
        // CDXC:AgentProviders 2026-09-03: same reasoning as switchDraftAgent
        // above; the resume command is built by the daemon that owns the row.
        | "/api/switchSessionAgent"
        | "/api/agentCliMaintenance"
        | "/api/managedTools"
        /*
        CDXC:AgentBox 2026-10-01 WHY:
        Remote-allowed like managedTools beside it: the boxes and the agentbox CLI belong to the machine that owns the project, so a client looking at a remote machine asks that machine. Setup commands come back as text gxserver chose, never text the caller supplied.
        */
        | "/api/agentbox"
        | "/api/agentAccounts"
        | "/api/readAgentLaunchPlan"
        | "/api/readAgentResumePlan"
        | "/api/requestSessionRename"
        | "/api/generateSessionTitle"
        | "/api/cancelFirstPromptAutoTitle"
        | "/api/ingestSessionStateEvent"
        | "/api/ingestTerminalTitleEvent"
        | "/api/updateAgentActivity"
        | "/api/readPresentationSnapshot"
        | "/api/readSidebarHud"
        | "/api/mutateSidebarHudSettings"
        | "/api/readWorkspaceSessionGroups"
        | "/api/updateWorkspaceSessionGroups"
        /*
        CDXC:Navigation 2026-08-19:
        Titlebar Back/Forward walks a daemon-owned trail of previously active
        sessions and projects. It carries only the same bounded routing ids and
        display titles the sidebar projection already sends, so it is remote
        allowed exactly like the other sidebar-state endpoints beside it.
        */
        | "/api/readNavigationHistory"
        | "/api/recordNavigationVisit"
        | "/api/navigateHistory"
        /*
        CDXC:Notifications 2026-09-11 WHY:
        The notification feed is a sidebar-state document like the trail above:
        bounded routing ids, display titles, and the agent's last message, read
        by the desktop bell, the web app, and mobile alike, so it is remote
        allowed beside the other sidebar-state endpoints.
        */
        | "/api/readNotificationFeed"
        | "/api/updateNotificationFeed"
        | "/api/createNotification"
        | "/api/readSidebarProjectCollections"
        | "/api/updateSidebarProjectCollections"
        | "/api/assignProjectToSidebarCollection"
        /*
        CDXC:Spaces 2026-08-27:
        Spaces are a saved sidebar filter owned by the daemon that owns the
        projects, so a remote gxserver section must be able to read and edit its
        own Space set. The document carries only the same bounded ids, names,
        icon ids, and colors the collections endpoints beside it already do.
        */
        | "/api/readSidebarSpaces"
        | "/api/updateSidebarSpaces"
        /*
        CDXC:Sessions 2026-09-11 WHY:
        The custom tag catalog belongs to the daemon whose sessions carry the
        ids, so a remote gxserver section reads and edits its own catalog
        exactly like its Spaces. The document carries only bounded ids, names,
        icon ids, and colors.
        */
        | "/api/readCustomSessionTags"
        | "/api/updateCustomSessionTags"
        | "/api/scheduleDelayedSend"
        | "/api/cancelDelayedSend"
        | "/api/postponeDelayedSend"
        | "/api/readDelayedSends"
        | "/api/readAutomationState"
        | "/api/saveAutomation"
        | "/api/deleteAutomation"
        | "/api/runAutomationNow"
        | "/api/setAutomationEnabled"
        | "/api/archiveAutomationRun"
        | "/api/markAutomationRunRead"
        | "/api/searchSessions"
        | "/api/listPreviousSessions"
        /*
        CDXC:SessionFork 2026-08-28:
        Remote-allowed for the same reason `/api/listPreviousSessions` above it
        is: the fork family is derived from the registry on the machine that ran
        the sessions. It carries only the bounded project/session ids, titles,
        and lifecycle states those list rows already send.
        */
        | "/api/sessionForkBranches"
        /*
        CDXC:SessionChat 2026-09-02:
        Remote-allowed for the same reason `/api/sendSessionChatMessage` is: the
        rewind is driven through the agent CLI's own dialog in the terminal, and
        that terminal only exists on the machine running the session, so a
        client looking at a remote daemon has to ask that machine. It carries
        only projectId / sessionId plus a transcript row id the daemon itself
        published, and it writes nothing but keystrokes into that one session.
        */
        | "/api/rewindSessionChat"
        | "/api/selectSessionChatModel"
        | "/api/readSessionTranscriptSizes"
        | "/api/transitionSession"
        /*
        CDXC:KeepAwake 2026-08-19:
        Ghostex mobile attaches over SSH and renews a keep-awake lease so the
        machine's Auto Sleep sweep cannot retire a terminal the phone is looking
        at. It carries only bounded project/session ids plus a TTL, exactly like
        the other session-scoped lifecycle calls next to it.
        */
        | "/api/holdSessionsAwake"
        /*
        CDXC:Zmx 2026-10-08 WHY:
        A chat shown on a remote machine's session claims that session's grid on the machine running it, like the keep-awake lease above. It carries only project/session ids, a holder id and a TTL.
        */
        | "/api/holdSessionChatGrid"
        /*
        CDXC:Coordinators 2026-09-30 WHY:
        A coordinator and its threads live on the machine that runs them, so a
        client looking at a remote machine reads and steers them there, like
        the session calls around these. They carry project/session ids plus
        bounded text (goal, instructions, one memory note).
        */
        | "/api/readCoordinator"
        | "/api/readCoordinatorThreads"
        | "/api/listCoordinators"
        | "/api/updateCoordinator"
        | "/api/linkCoordinatorThread"
        | "/api/setCoordinatorThreadResolved"
        | "/api/openCoordinatorThread"
        | "/api/promoteCoordinator"
        | "/api/toggleCloseAfterDone"
        | "/api/openConversation"
        | "/api/sleepSession"
        | "/api/wakeSession"
        | "/api/startSessionProvider"
        | "/api/killSession"
        | "/api/probeSessionProvider"
        | "/api/readResourceSessionOwners"
        | "/api/listSessions"
        | "/api/removeSession"
        | "/api/readSessionText"
        /*
        CDXC:PromptSearch 2026-08-20:
        The Find surface is remote-allowed for the same reason chat is: an
        agent's prompt history lives on the machine that ran the agent, so a
        client looking at a remote machine has to ask that machine. These calls
        carry a bounded query plus index positions the daemon itself handed out;
        they never name a filesystem path and never write outside the favorites
        file the terminal picker already owns.
        */
        | "/api/searchAgentPrompts"
        | "/api/readAgentPromptText"
        | "/api/toggleAgentPromptFavorite"
        | "/api/resolveAgentPromptLaunch"
        | "/api/readSessionChat"
        /*
        CDXC:SessionChat 2026-08-26:
        Remote-allowed for the same reason `/api/readSessionText` next to it is:
        the screen only exists on the machine running the session, and this
        carries a bounded thirty-line tail of it plus the composer verdict. A
        client that just had a send refused needs to SHOW the user what is in
        the terminal, and it cannot do that from the other side of an SSH hop
        without asking.
        */
        | "/api/readSessionTerminalTail"
        | "/api/readSessionChatSkills"
        | "/api/readSessionChatFiles"
        | "/api/sendSessionChatMessage"
        | "/api/saveSessionChatImage"
        | "/api/saveSessionChatAttachment"
        | "/api/readSessionChatImage"
        | "/api/answerSessionChatPrompt"
        | "/api/interruptSessionChat"
        | "/api/acknowledgeSessionChatDraftHandoff"
        | "/api/handoffSessionChatDraft"
        | "/api/replaceSessionChatDraft"
        | "/api/claimSessionChatLaunchDraft"
        /*
        CDXC:SessionChat 2026-08-21:
        The chat prompt queue and the synced composer draft are remote-allowed
        for the same reason chat itself is: the queue is owned by the daemon
        that runs the session, so a client looking at a remote machine (or a
        phone over SSH) has to ask that machine. They carry only projectId /
        sessionId, a row id the daemon itself handed out, and prompt text the
        user typed into that session's composer.
        */
        | "/api/readSessionChatQueue"
        | "/api/queueSessionChatPrompt"
        | "/api/updateSessionChatQueuedPrompt"
        | "/api/removeSessionChatQueuedPrompt"
        | "/api/reorderSessionChatQueue"
        | "/api/sendSessionChatQueuedPrompt"
        | "/api/setSessionChatDraft"
        | "/api/listSessionChatDrafts"
        /*
        CDXC:TranscriptExport 2026-08-20:
        Exporting a transcript is remote-allowed for the same reason reading a
        chat is: the transcript file only exists on the machine that runs the
        agent, so a client looking at a remote session has to ask that machine.
        It carries only projectId/sessionId, reads a transcript the daemon
        resolved itself, and writes into the daemon's own exports directory —
        the caller never names a path.
        */
        | "/api/exportSessionTranscript"
        | "/api/sendSessionText"
        | "/api/sendSessionMessage"
        | "/api/sendSessionEnter"
        | "/api/focusSession"
        | "/api/dispatchRendererCommand"
        | "/api/attachSessionMetadata"
        | "/api/createProject"
        | "/api/updateProject"
        | "/api/listProjects"
        | "/api/closeProjectToRecent"
        | "/api/listRecentProjects"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/readProjectStatus"
        | "/api/addProjectPath"
        | "/api/listProjectWorktrees"
        | "/api/createProjectWorktree"
        | "/api/openProjectWorktree"
        | "/api/mergeWorktreeIntoMain"
        | "/api/checkoutProjectNewBranch"
        | "/api/removeProject"
        | "/api/deleteWorktreeProject"
        /*
        CDXC:Worktrees 2026-08-09-18:40:
        Renaming a worktree is remote-allowed for the same reason deleting one
        is: a GPUI sidebar showing a remote machine's projects has to be able to
        rename a worktree there. `projectId` is an opaque selector and `name` is
        validated against the daemon's own ref policy before it reaches git; the
        destination folder is computed by the daemon, never named by the caller.
        */
        | "/api/renameWorktreeProject"
        /*
        CDXC:Worktrees 2026-07-29-00:00:
        Worktree sessions are remote-allowed for the same reason the other
        worktree RPCs are: a GPUI sidebar showing a remote machine's projects has
        to be able to start work in a worktree there. The `existingWorktree.path`
        and `worktreePath` parameters are SELECTORS, never authority — each is
        accepted only after it matches an entry in that daemon's own
        `git worktree list` for the requested project's family, so a renderer
        still cannot name an arbitrary directory on the remote machine.
        */
        | "/api/createWorktreeSession"
        | "/api/removeSessionWorktree"
        | "/api/updateSession"
        | "/api/updateSessionOrder"
        /*
        CDXC:StateSync 2026-07-29-00:00:
        Settle/snooze is sidebar inventory state, so the remote listener must
        expose it exactly like the other session-metadata RPCs: a GPUI sidebar
        showing a remote machine's sessions has to be able to settle and snooze
        them too.
        */
        | "/api/settleSession"
        | "/api/unsettleSession"
        | "/api/snoozeSession"
        | "/api/unsnoozeSession"
        | "/api/runGitAction"
        | "/api/readProjectGitState"
        | "/api/runGitHubAction"
        | "/api/runWorktreeAction"
        | "/api/runProjectSetupCommand"
        | "/api/runBeadsAction"
        | "/api/runProjectDocsAction"
        /*
        CDXC:ProjectBoard 2026-08-08:
        Remote board dispatch is allowed because it creates and starts the
        worker on the selected daemon. beadId, projectId, and agent are opaque
        selectors only; none is interpreted as a path or command.
        */
        | "/api/startBoardWork"
        /*
        CDXC:ProjectBoard 2026-08-24:
        Associating is remote-allowed for the same reason dispatching is: the
        board being worked can live on another daemon, and the call carries only
        opaque beadId/projectId/sessionId selectors. It links a session that
        daemon already owns; a session it does not know is rejected.
        */
        | "/api/associateBoardSession"
        | "/api/previewRepositoryClone"
        | "/api/startRepositoryClone"
        | "/api/readRepositoryCloneJob"
        | "/api/cancelRepositoryCloneJob"
        | "/api/browseProjectDirectories"
        /*
        CDXC:AddProject 2026-08-18:
        Creating the destination folder is remote-allowed for the same reason
        browsing is: the Add Project dialog runs against a chosen machine and
        has to be able to make a folder there before adding or cloning into it.
        `parentPath` must already be an existing directory and `name` is a
        single validated path segment, so this creates one child of a directory
        the caller could already browse.
        */
        | "/api/createProjectDirectory"
        /*
        CDXC:AddProject 2026-07-30:
        The Add Project dialog runs against a chosen machine, so a GPUI sidebar
        adding a project on a remote daemon must be able to ask THAT daemon
        which hosting CLIs it has and to resolve `owner/repo` there. Discovery
        returns readiness plus an install/auth hint with token lines stripped,
        and lookup returns only the repository's public clone URLs, so neither
        hands the renderer authority it did not already have over the clone
        endpoints below.
        */
        | "/api/discoverSourceControl"
        | "/api/lookupRepository"
        | "/api/listExtensions"
        | "/api/extensionsCatalog"
        | "/api/installExtension"
        | "/api/uninstallExtension"
        | "/api/updateExtensionState"
        | "/api/projectView"
        | "/api/startExtension"
        | "/api/stopExtension"
        | "/api/extensionStatus"
        | "/api/extensionBadge"
        /*
        CDXC:WorkMode 2026-10-09 WHY:
        A sidebar showing a remote machine's projects can flip their Work mode switch and set a
        session's links like any other session metadata; both take opaque project and session ids.
        */
        | "/api/setProjectWorkMode"
        | "/api/setSessionWorkLinks"
        // The Link to picker's suggestions and the merged-PR Clean up / Keep answer act on one
        // session by its ids, like the links themselves.
        | "/api/listWorkLinkCandidates"
        | "/api/answerWorkCleanup"
        /*
        CDXC:WorkMode 2026-10-09 WHY:
        Starting work on a ticket and creating a Linear ticket are remote-allowed like
        createWorktreeSession: a sidebar showing a remote machine's projects starts work there. They
        use that machine's Linear key without ever returning it, and take only opaque ids and text.
        */
        | "/api/startWorkOnTicket"
        | "/api/createLinearIssue"
        | "/api/listLinearTeams"
        | "/api/listLinearProjects" => remote_allowed(path),
        /*
        CDXC:WorkMode 2026-10-09 WHY:
        The Work page reads the daemon that owns the projects, like the sidebar: the list, a
        ticket's details and its team flow take opaque ids and return what Linear and `gh` say,
        never a key. Editing the team flow is a setting of this computer's.
        */
        "/api/listWorkItems" | "/api/readWorkItem" | "/api/readTeamFlow" => remote_allowed(path),
        /*
        CDXC:WorkMode 2026-10-09 WHY:
        A remote sidebar creates a GitHub issue on the machine that owns the project, like a Linear
        ticket, and closes the Work page's notice there. Reading and changing a workspace's tracker
        is a setting of this computer's (a team workspace's choice goes through its member token).
        */
        "/api/createGithubIssue" | "/api/dismissWorkNotice" => remote_allowed(path),
        "/api/readWorkTracker" | "/api/setWorkTracker" => full_local(path),
        "/api/updateTeamFlow" => full_local(path),
        /*
        CDXC:WorkMode 2026-10-10 WHY:
        A cloud session starts with this computer's own `claude` login and Git remote, and its
        link is recorded on this computer, so only this computer's clients may draft or start one.
        */
        "/api/draftCloudWork" | "/api/startCloudWork" => full_local(path),
        /*
        CDXC:Workspaces 2026-10-09 WHY:
        Workspaces belong to the daemon that owns the projects, exactly like its Spaces, so a
        remote gxserver section reads and edits its own; the document carries only bounded ids,
        names, letters, colors and an account id.
        */
        "/api/readWorkspaces"
        | "/api/createWorkspace"
        | "/api/updateWorkspace"
        | "/api/deleteWorkspace"
        | "/api/moveProjectToWorkspace"
        | "/api/moveMachineToWorkspace" => remote_allowed(path),
        /*
        CDXC:WorkMode 2026-10-09 WHY:
        The Linear key is a secret on this computer, so only this computer's clients may set it or
        ask which keys exist.
        */
        "/api/setLinearApiKey" | "/api/readWorkModeStatus" => full_local(path),
        /*
        CDXC:TeamSync 2026-10-09 WHY:
        These hold or use this computer's member token for the team's Convex project, so only this
        computer's clients may call them; a remote sidebar gets its own machine's team data there.
        */
        "/api/joinTeamSync"
        | "/api/connectTeamSync"
        | "/api/leaveTeamSync"
        | "/api/readTeamSyncStatus"
        | "/api/createTeamSyncInvite"
        | "/api/setTeamSyncIdentity"
        | "/api/pingTeamSync"
        | "/api/listTeamSyncCommands"
        | "/api/listTicketSlackThreads"
        | "/api/setTeamLinearKey"
        | "/api/setOwnLinearKey"
        | "/api/readSlackFlowSettings"
        | "/api/setSlackFlowSettings"
        | "/api/readSlackManifest"
        | "/api/postSlackWorkingThread" => full_local(path),
        "/api/createQuickProject" | "/api/syncBotProjects" => full_local(path),
        /*
        CDXC:Bots 2026-09-27 WHY:
        The feed reads this computer's Hermes cron output, which holds whole
        agent reports, so only the desktop on this computer may read it.
        */
        "/api/listBotFeed" => full_local(path),
        /*
        CDXC:Telemetry 2026-08-26:
        The desktop app's loopback analytics ping. Authenticated like every other
        local endpoint, but deliberately NOT protocol-version gated: the caller
        is fire-and-forget, never reads the response, and has no way to react to
        a 426 — so a version skew would silently turn into a retry loop against a
        wall instead of an event that simply does not arrive. Local-only, because
        a remote helper's analytics are the desktop's to report, not its own.
        */
        "/api/recordClientEvent" => descriptor(
            path,
            ApiPermission::FullLocal,
            true,
            false,
            Transport::Http,
        ),
        /*
        CDXC:AgentHooks 2026-06-19-14:15:
        Hook read/install/uninstall endpoints inspect or mutate user-local provider config files, so Rust keeps the TypeScript contract as full-local HTTP APIs requiring auth and protocol-version gates.
        */
        "/api/readAgentHookStatus"
        | "/api/installAgentHooks"
        | "/api/uninstallAgentHooks"
        | "/api/readAgentSkillStatus"
        | "/api/installAgentSkills"
        /*
        CDXC:ServerDaemon 2026-06-24-13:30:
        Pinned Prompts can carry user-authored bodies, so their shared gxserver
        RPCs are local-only authenticated endpoints rather than remote-listener
        APIs.
        */
        | "/api/readAppUserData"
        | "/api/savePinnedPrompt"
        /*
        CDXC:SavedPrompts 2026-07-29-00:00:
        Stashed prompts carry user-authored prompt bodies captured from the
        prompt editor, so like Pinned Prompts they stay local-only
        authenticated endpoints.
        */
        | "/api/saveStashedPrompt"
        | "/api/listStashedPrompts"
        | "/api/deleteStashedPrompt"
        /*
        CDXC:SavedPrompts 2026-08-23:
        Tag names are user-authored labels for those same prompt bodies, so the
        tag catalogue and its assignments stay on the same local-only
        authenticated listener as the prompts they describe.
        */
        | "/api/listStashedPromptTags"
        | "/api/saveStashedPromptTag"
        | "/api/deleteStashedPromptTag"
        | "/api/setStashedPromptTags"
        /*
        CDXC:SessionNotes 2026-08-24:
        A session note is user-authored prose about a conversation, so it stays
        in the same class as Pinned and Stashed prompts: local-only
        authenticated endpoints, and the body is never logged.
        */
        | "/api/saveSessionAgentNote"
        | "/api/readSessionAgentNote"
        /*
        CDXC:Git 2026-06-24-16:11:
        Commit-message generation carries staged diff content and generated
        commit text through the authenticated response. Keep this endpoint on
        the local listener, but allow GPUI saved-machine SSH tunnels to reach
        the remote daemon's localhost API after the Rust bridge validates the
        machine id, endpoint, timeout, and response shape.

        CDXC:Git 2026-06-24-16:28:
        Background PR creation confirms `gh pr create --fill` through a
        sanitized gxserver result before GPUI can open the PR or delete a
        worktree. Remote GPUI may use the same local-listener endpoint only
        through its saved-machine tunnel, and the GPUI bridge strips PR URL
        launch authority from remote responses.
        */
        | "/api/generateCommitMessage"
        | "/api/createPullRequest"
        | "/api/runGitShipWorkflow"
        | "/api/updatePortlessState"
        /*
        CDXC:RemotePairing 2026-09-01:
        The tailcat status response carries the address blob that dials this
        machine, so both endpoints stay on the authenticated local listener and
        are never reachable from the remote listener.
        */
        | "/api/tailcatStatus"
        | "/api/installTailcat"
        | "/api/updateTailcatState"
        /*
        CDXC:RemotePairing 2026-09-03:
        The pairing code carries the one-time pairing secret and the login
        user, and enabling SSH access opens an admin prompt on this machine,
        so all three stay on the authenticated local listener.
        */
        | "/api/remoteAccessStatus"
        | "/api/enableSshAccess"
        | "/api/remotePairingCode"
        /*
        CDXC:RemotePairing 2026-09-03:
        Paired-device management edits `~/.ssh/authorized_keys` and the tailcat
        allow-list, so it stays on the authenticated local listener like the
        pairing code that created the devices.
        */
        | "/api/pairedDevices"
        | "/api/removePairedDevice"
        | "/api/pairedDeviceSeen"
        | "/api/queryLogs"
        | "/api/resolveGitRootForPath" => full_local(path),
        /*
        CDXC:Feedback 2026-10-04 WHY:
        Sending feedback files a public GitHub issue through the Ghostex
        relay, so only this computer's own app may ask for it; a remote
        listener caller must never post issues in this user's name.
        */
        "/api/draftFeedback" | "/api/sendFeedback" => full_local(path),
        /*
        CDXC:ServerApi 2026-10-06 WHY:
        `ghostex show` publishes a page an agent on this computer wrote, so only the loopback listener accepts it; the remote listener must not let another machine store pages that this gxserver then serves.
        */
        "/api/publishVisualPage" => full_local(path),
        /*
        CDXC:RemotePairing 2026-09-03:
        The only unauthenticated write endpoint. A phone registers its SSH key
        through the Easy Connect tunnel before it holds any gxserver credential,
        so the bearer gate is off and the one-time pairing secret (hash-compared,
        single-use, 15-minute TTL, 5 attempts/minute) is the gate instead. It
        stays FullLocal: only the loopback API listener (and the tunnel that
        forwards to it) serves it, never the remote listener.
        */
        "/api/pairDevice" => descriptor(
            path,
            ApiPermission::FullLocal,
            false,
            true,
            Transport::Http,
        ),
        "/api/updateAuth"
        | "/api/updateListenerConfig"
        | "/api/installTool"
        | "/api/browseFilesystem"
        | "/api/destructiveAdminAction"
        => return None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn protocol_mismatch_message_uses_typescript_stringification() {
        let object = protocol_mismatch_error(Some(json!({ "version": 2 })), Some("r1".to_string()));
        assert!(object.message.contains("got [object Object]."));

        let array = protocol_mismatch_error(Some(json!([1, null, "x"])), Some("r2".to_string()));
        assert!(array.message.contains("got 1,,x."));

        let missing = protocol_mismatch_error(None, Some("r3".to_string()));
        assert!(missing.message.contains("got undefined."));
    }
}
