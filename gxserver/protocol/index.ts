/*
CDXC:GxserverProtocol 2026-05-30-14:04:
The gxserver protocol is the shared contract for the daemon, future gx/ghostex CLI clients, macOS clients, and remote clients. JSON fields and endpoint path tokens stay camelCase; protocol mismatch is a hard failure that asks the user to update instead of falling back to compatibility behavior.

CDXC:GxserverProtocol 2026-06-04-03:20:
`zmxName` is the canonical provider identity and must carry the full server-project-session id. Clients should treat shorter project/session or compact g-* names as legacy display/state data, not as the reconnect target for gxserver zmx sessions.

CDXC:GxserverRendererCommands 2026-06-13-02:24:
CLI commands that still require visible macOS UI, AppKit, CEF, or sidebar-local workspace state must enter through a typed gxserver command contract. gxserver owns auth, protocol checks, dispatch, timeouts, and the supported action list; the macOS app is only the renderer-side executor for behavior that cannot live in the daemon yet.
*/

export const GXSERVER_PRODUCT = "gxserver" as const;
export const GXSERVER_PROTOCOL_VERSION = 1 as const;
export const GXSERVER_LOCAL_API_HOST = "127.0.0.1" as const;
export const GXSERVER_LOCAL_API_PORT = 58744 as const;
export const GXSERVER_REMOTE_API_HOST = "0.0.0.0" as const;
export const GXSERVER_REMOTE_API_PORT = 58745 as const;
export const GXSERVER_MACOS_BRIDGE_PORT = 58743 as const;
export const GXSERVER_RUNTIME_METADATA_PATH = "~/.ghostex/gxserver/runtime/server.json" as const;
export const GXSERVER_STORAGE_ROOT_PATH = "~/.ghostex/gxserver" as const;

export type GxserverProduct = typeof GXSERVER_PRODUCT;
export type GxserverProtocolVersion = typeof GXSERVER_PROTOCOL_VERSION;
export type GxserverServerId = `S${number}${Lowercase<string>}`;
export type GxserverProjectId = `P${number}${Lowercase<string>}`;
export type GxserverSessionId = `G${number}${Lowercase<string>}`;
export type GxserverGlobalSessionRef = `${GxserverServerId}:${GxserverProjectId}:${GxserverSessionId}`;
export type GxserverZmxSessionName = `${GxserverServerId}-${GxserverProjectId}-${GxserverSessionId}`;
export type GxserverAuthToken = string & { readonly __gxserverAuthToken: unique symbol };
export type GxserverLogLevel = "debug" | "info" | "warn" | "error";
export type GxserverLogOrder = "asc" | "desc";
export type GxserverListenerKind = "local" | "remote";
export type GxserverApiPermission = "fullLocal" | "remoteAllowed" | "remoteBlocked";
export type GxserverRpcErrorCode =
  | "badRequest"
  | "corruptState"
  | "dependencyUnavailable"
  | "forbidden"
  | "internalError"
  | "methodNotAllowed"
  | "notFound"
  | "notImplemented"
  | "protocolMismatch"
  | "unauthorized";

export const GXSERVER_RENDERER_COMMAND_ACTIONS = [
  "assertSidebarCard",
  "automationArchiveRun",
  "automationMarkRunRead",
  "automationRunNow",
  "automationSave",
  "automationSetEnabled",
  "automationState",
  "clickButton",
  "focusGroup",
  "focusSession",
  "fullReloadSession",
  "moveProject",
  "moveSidebar",
  "openBrowser",
  "openBrowserPane",
  "openPaths",
  "restartSession",
  "runCommand",
  "saveAgent",
  "sendMessage",
  "setViewMode",
  "setVisibleCount",
  "switchProject",
  "toggleSidebarCollapsed",
  "waitFor",
] as const;

export type GxserverRendererCommandAction = typeof GXSERVER_RENDERER_COMMAND_ACTIONS[number];

export type GxserverEndpointPath =
  | "/api/health"
  | "/api/health/server"
  | "/api/events"
  | "/api/control/stop"
  | "/api/control/stopAll"
  | "/api/readAgentSettings"
  | "/api/updateAgentSettings"
  | "/api/readAgentHookStatus"
  | "/api/installAgentHooks"
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
  | "/api/browseProjectDirectories"
  | "/api/resolveGitRootForPath"
  | "/api/queryLogs"
  | "/api/updateAuth"
  | "/api/updateListenerConfig"
  | "/api/installTool"
  | "/api/browseFilesystem"
  | "/api/destructiveAdminAction";

export type GxserverRpcEndpointPath = Exclude<
  GxserverEndpointPath,
  "/api/health" | "/api/health/server" | "/api/events"
>;

export type GxserverLifecycleState =
  | "running"
  | "stopped"
  | "starting"
  | "stopping"
  | "stale"
  | "unreachable"
  | "portConflict"
  | "protocolMismatch";

export interface GxserverMinimalHealthResponse {
  ok: true;
  product: GxserverProduct;
  protocolVersion: GxserverProtocolVersion;
  version: string;
}

export interface GxserverListenerConfig {
  auth?: GxserverListenerAuthConfig;
  enabled: boolean;
  host: string;
  kind: GxserverListenerKind;
  port: number;
}

export interface GxserverListenerAuthConfig {
  mode: "bearerToken";
  required: true;
}

export interface GxserverMigrationStatus {
  appliedMigrations: readonly string[];
  currentVersion: number;
  stateImports?: {
    legacyMacosState?: GxserverStateImportStatus;
  };
  stateDbFile: string;
}

export interface GxserverStateImportStatus {
  completedAt?: string;
  id: string;
  logsImported?: GxserverLegacyLogImportStatus;
  projectsImported?: number;
  sessionsImported?: number;
  skippedReason?: "alreadyCompleted" | "noLegacyState";
  sourceFilesRead?: readonly string[];
  status: "notRun" | "completed" | "skipped";
}

export interface GxserverLegacyLogImportStatus {
  filesRead: number;
  malformedLineCount: number;
  migratedLineCount: number;
}

export interface GxserverServerHealthResponse extends GxserverMinimalHealthResponse {
  buildIdentity: string;
  capabilities: readonly string[];
  listeners: {
    local: GxserverListenerConfig;
    remote: GxserverListenerConfig;
  };
  migration: GxserverMigrationStatus;
  pid: number;
  port: typeof GXSERVER_LOCAL_API_PORT;
  serverId: GxserverServerId;
  startedAt: string;
  tools: readonly GxserverToolCapabilityStatus[];
}

export type GxserverToolName = "zmx" | "zehn" | "bd";
export type GxserverToolAvailability = "available" | "missing" | "notExecutable" | "unsupported";
export type GxserverToolResolutionSource = "devSubmodule" | "appResource" | "gxserverBundle";

export interface GxserverToolCapabilityStatus {
  availability: GxserverToolAvailability;
  candidatePaths?: readonly string[];
  capability: "zmxLifecycle" | "previousSessionHistory" | "beadsProjectBoard" | "deferred";
  executablePath?: string;
  guidance?: string;
  message: string;
  source?: GxserverToolResolutionSource;
  tool: GxserverToolName;
}

export interface GxserverRuntimeMetadata {
  buildIdentity: string;
  pid: number;
  port: typeof GXSERVER_LOCAL_API_PORT;
  protocolVersion: GxserverProtocolVersion;
  serverId: GxserverServerId;
  startedAt: string;
  version: string;
}

export interface GxserverStatusResponse {
  health?: GxserverServerHealthResponse;
  metadata?: GxserverRuntimeMetadata;
  message: string;
  ok: boolean;
  product: GxserverProduct;
  state: GxserverLifecycleState;
}

export interface GxserverProtocolMismatch {
  actualProtocolVersion: unknown;
  expectedProtocolVersion: GxserverProtocolVersion;
  message: string;
  product: GxserverProduct;
}

export interface GxserverRpcRequest<TParams extends Record<string, unknown> = Record<string, unknown>> {
  params?: TParams;
  protocolVersion: GxserverProtocolVersion;
}

export interface GxserverRpcSuccessResponse<TResult extends Record<string, unknown> = Record<string, unknown>> {
  ok: true;
  product: GxserverProduct;
  protocolVersion: GxserverProtocolVersion;
  requestId: string;
  result: TResult;
}

export interface GxserverRpcErrorResponse {
  error: GxserverRpcErrorCode;
  message: string;
  ok: false;
  product: GxserverProduct;
  protocolVersion?: GxserverProtocolVersion;
  requestId?: string;
}

export interface GxserverAgentSettings {
  agentAcceptAllEnabled: boolean;
}

export interface GxserverReadAgentSettingsResult {
  isPersisted: boolean;
  settings: GxserverAgentSettings;
}

export interface GxserverUpdateAgentSettingsParams {
  agentAcceptAllEnabled?: boolean;
}

export type GxserverAgentHookStatus = "cliMissing" | "installed" | "missing" | "updateRequired";

export interface GxserverAgentHookStatusRow {
  agentId: string;
  cliCommand: string;
  cliInstalled: boolean;
  detail: string;
  hookInstalled: boolean;
  paths: readonly string[];
  status: GxserverAgentHookStatus;
}

export interface GxserverReadAgentHookStatusParams {
  agentIds?: readonly string[];
  autoUpgradeInstalled?: boolean;
}

export interface GxserverReadAgentHookStatusResult {
  agents: readonly GxserverAgentHookStatusRow[];
  autoUpgradedPaths?: readonly string[];
  generatedAt: string;
  hookStateDirectory: string;
  notifyHookPath: string;
  type: "agentHookStatus";
}

export interface GxserverInstallAgentHooksParams {
  agentIds?: readonly string[];
}

export interface GxserverInstallAgentHooksResult extends GxserverReadAgentHookStatusResult {
  installedPaths: readonly string[];
}

export interface GxserverIngestAgentHookEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  eventName?: string;
  firstUserMessage?: string;
  rawEventName?: string;
  status?: GxserverAgentActivityState["activity"];
  statusUpdatedAt?: string;
  title?: string;
}

export interface GxserverIngestAgentHookEventResult {
  activity?: GxserverAgentActivityState;
  changed: boolean;
  enteredAttention: boolean;
  previousActivity?: GxserverAgentActivityState["activity"];
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
}

export interface GxserverEndpointDescriptor {
  path: GxserverEndpointPath;
  permission: GxserverApiPermission;
  requiresAuth: boolean;
  requiresProtocolVersion: boolean;
  transport: "http" | "webSocket";
}

export interface GxserverRendererCommand {
  action: GxserverRendererCommandAction;
  commandId: string;
  createdAt: string;
  payload: Record<string, unknown>;
  timeoutMs: number;
}

export interface GxserverProjectDirectoryBrowseParams {
  cwd?: string;
  limit?: number;
  partialPath: string;
}

export interface GxserverProjectDirectoryBrowseEntry {
  fullPath: string;
  name: string;
}

export interface GxserverProjectDirectoryBrowseResult {
  entries: GxserverProjectDirectoryBrowseEntry[];
  parentPath: string;
}

export interface GxserverStoragePaths {
  authToken: "~/.ghostex/gxserver/auth/token";
  config: "~/.ghostex/gxserver/config.json";
  identity: "~/.ghostex/gxserver/identity.json";
  logs: "~/.ghostex/gxserver/logs/gxserver.jsonl";
  migrations: "~/.ghostex/gxserver/migrations";
  root: typeof GXSERVER_STORAGE_ROOT_PATH;
  runtime: "~/.ghostex/gxserver/runtime";
  stateDb: "~/.ghostex/gxserver/state.db";
  zmx: "~/.ghostex/gxserver/zmx";
}

export interface GxserverLogEntry {
  ts: string;
  level: GxserverLogLevel;
  event: string;
  serverId?: GxserverServerId;
  requestId?: string;
  projectId?: GxserverProjectId;
  sessionId?: GxserverSessionId;
  client?: string;
  durationMs?: number;
  error?: string;
  details?: Record<string, unknown>;
  legacyFile?: string;
  message?: string;
  source?: string;
}

export interface GxserverQueryLogsParams {
  client?: string;
  event?: string;
  eventPrefix?: string;
  level?: GxserverLogLevel | readonly GxserverLogLevel[];
  limit?: number;
  order?: GxserverLogOrder;
  projectId?: GxserverProjectId;
  reverse?: boolean;
  serverId?: GxserverServerId;
  sessionId?: GxserverSessionId;
  since?: string;
  until?: string;
}

export interface GxserverQueryLogsResult {
  entries: GxserverLogEntry[];
  logFileSizeBytes?: number;
  malformedLineCount: number;
  malformedLineCountIsExact?: boolean;
  scannedBytes?: number;
  scannedLineCount?: number;
  totalMatched: number;
  totalMatchedIsExact?: boolean;
  truncated?: boolean;
  truncatedReason?: "fileWindowExceeded";
}

export type GxserverGitAction =
  | "branch"
  | "addAll"
  | "checkout"
  | "checkoutNewBranch"
  | "commit"
  | "countFileLines"
  | "deleteLocalBranch"
  | "deleteRemoteBranch"
  | "diff"
  | "diffCached"
  | "diffCachedNoExt"
  | "diffCachedStat"
  | "diffNoExt"
  | "diffNoIndexAgainstNull"
  | "diffNumstat"
  | "getOriginRemoteUrl"
  | "isInsideWorkTree"
  | "isUntrackedFile"
  | "list"
  | "listRemotes"
  | "listUntracked"
  | "merge"
  | "push"
  | "pushSetUpstream"
  | "remoteBranchExists"
  | "status"
  | "statusPorcelain"
  | "upstreamCounts"
  | "verifyRef";
export type GxserverWorktreeAction = "create" | "ensureBeadsHooks" | "list" | "pathExists" | "prune" | "remove" | "switch";
export type GxserverBeadsAction =
  | "addLabel"
  | "board"
  | "close"
  | "comment"
  | "configGet"
  | "configGetIssuePrefix"
  | "configSet"
  | "create"
  | "delete"
  | "depAdd"
  | "depRemove"
  | "list"
  | "listAllLabels"
  | "renamePrefix"
  | "removeLabel"
  | "search"
  | "setLabels"
  | "show"
  | "status"
  | "storageExists"
  | "update"
  | "updateDescription"
  | "updateEstimate"
  | "updatePriority"
  | "updateStatus"
  | "updateTitle";
export type GxserverBeadsStatus = "backlog" | "closed" | "in_progress" | "open" | "review" | "test";
export type GxserverGitHubAction = "prCreateFill" | "prView" | "version";
export type GxserverProjectSetupAction = "worktreeSetupCommand";

export interface GxserverProjectOperationScope {
  projectId?: GxserverProjectId;
  projectPath?: string;
}

export interface GxserverRunGitActionParams extends GxserverProjectOperationScope {
  action: GxserverGitAction;
  branch?: string;
  filePath?: string;
  filePaths?: readonly string[];
  messageBody?: string;
  messageSubject?: string;
  noVerify?: boolean;
  ref?: string;
  remoteName?: string;
}

export interface GxserverRunWorktreeActionParams extends GxserverProjectOperationScope {
  action: GxserverWorktreeAction;
  baseRef?: string;
  branch?: string;
  force?: boolean;
  worktreePath?: string;
}

export interface GxserverRunGitHubActionParams extends GxserverProjectOperationScope {
  action: GxserverGitHubAction;
}

export interface GxserverRunProjectSetupCommandParams extends GxserverProjectOperationScope {
  action: GxserverProjectSetupAction;
  setupCommandProjectId?: GxserverProjectId;
  setupCommandProjectPath?: string;
}

export interface GxserverDeleteWorktreeProjectParams {
  deleteLocalBranch?: boolean;
  deleteRemoteBranch?: boolean;
  projectId: GxserverProjectId;
  remoteName?: string;
}

export type GxserverDeleteWorktreeProjectWarningKind =
  | "localBranchDeleteFailed"
  | "localBranchNotResolved"
  | "pruneFailed"
  | "remoteBranchDeleteFailed"
  | "remoteBranchNotResolved";

export interface GxserverDeleteWorktreeProjectWarning {
  kind: GxserverDeleteWorktreeProjectWarningKind;
  message: string;
}

export interface GxserverDeleteWorktreeProjectResult {
  checkoutRemoval: {
    forced: boolean;
    retriedForSubmodules: boolean;
  };
  project: GxserverProjectDomainState;
  warnings: readonly GxserverDeleteWorktreeProjectWarning[];
}

export interface GxserverRunBeadsActionParams extends GxserverProjectOperationScope {
  action: GxserverBeadsAction;
  comment?: string;
  dependsOnId?: string;
  description?: string;
  depType?: string;
  estimate?: number;
  issueId?: string;
  label?: string;
  labels?: readonly string[];
  priority?: string;
  query?: string;
  status?: GxserverBeadsStatus;
  title?: string;
  value?: string;
}

export interface GxserverResolveGitRootForPathParams {
  path: string;
}

export interface GxserverResolveGitRootForPathResult {
  gitRoot?: string;
}

export interface GxserverRepositoryCloneOptions {
  branchName?: string;
  cloneMainOnly?: boolean;
  shallowClone?: boolean;
}

export interface GxserverRepositoryClonePreviewParams extends GxserverRepositoryCloneOptions {
  destinationFolderName?: string;
  folderPath?: string;
  newFolderName?: string;
  parentPath?: string;
  repositoryInput: string;
}

export interface GxserverRepositoryCloneStartParams extends GxserverRepositoryClonePreviewParams {}

export interface GxserverRepositoryCloneJobParams {
  jobId: string;
}

export interface GxserverRepositoryClonePreviewResult {
  branchName?: string;
  cloneMainOnly: boolean;
  cloneUrl: string;
  defaultFolderName: string;
  destinationExists: boolean;
  destinationExistsKind?: "directory" | "file" | "other";
  destinationFolderName: string;
  destinationIsEmpty?: boolean;
  destinationPath: string;
  parentPath: string;
  repositoryName: string;
  shallowClone: boolean;
  warning?: string;
}

export type GxserverRepositoryCloneJobState = "running" | "completed" | "failed" | "canceled";

export interface GxserverRepositoryCloneJobStatus {
  completedAt?: string;
  error?: string;
  exitCode?: number;
  jobId: string;
  message: string;
  preview: GxserverRepositoryClonePreviewResult;
  project?: GxserverProjectDomainState;
  projectPath?: string;
  startedAt: string;
  state: GxserverRepositoryCloneJobState;
  stderr?: string;
  stdout?: string;
}

export interface GxserverRepositoryClonePreviewRpcResult {
  preview: GxserverRepositoryClonePreviewResult;
}

export interface GxserverRepositoryCloneJobRpcResult {
  job: GxserverRepositoryCloneJobStatus;
}

export interface GxserverTypedCommand {
  args: readonly string[];
  cwd: string;
  executable: string;
}

export type GxserverTypedOperationFailureCode =
  | "aborted"
  | "stderrLimitExceeded"
  | "stdinFailed"
  | "stdoutLimitExceeded"
  | "timeout";

export interface GxserverTypedOperationFailure {
  capturedBytes?: number;
  code: GxserverTypedOperationFailureCode;
  limitBytes?: number;
  message: string;
  stream?: "stderr" | "stdout";
  timeoutMs?: number;
}

export interface GxserverWorktreeListEntry {
  bare: boolean;
  branch: string;
  detached: boolean;
  path: string;
}

export interface GxserverTypedOperationResult {
  action:
    | GxserverGitAction
    | GxserverGitHubAction
    | GxserverWorktreeAction
    | GxserverProjectSetupAction
    | GxserverBeadsAction;
  command?: GxserverTypedCommand;
  error?: GxserverTypedOperationFailure;
  exitCode: number;
  stderr: string;
  stdout: string;
  worktrees?: readonly GxserverWorktreeListEntry[];
}

export interface GxserverBeadsBoardResult extends GxserverTypedOperationResult {
  issues: readonly Record<string, unknown>[];
}

export type GxserverSharedStateArea =
  | "projects"
  | "sessions"
  | "zmxLifecycle"
  | "sleepWakePolicy"
  | "agentStatus"
  | "remoteControl"
  | "pinnedFavorite"
  | "customAgentsCommands"
  | "launchRuntimeSettings"
  | "previousSessionHistory"
  | "worktreeGitActions"
  | "beadsProjectBoard";

export type GxserverClientLocalStateArea =
  | "sidebarGroups"
  | "splitTabLayout"
  | "visibleSessionCount"
  | "browserEditorCodeServerPanes"
  | "cefBrowserProfiles"
  | "popOutWindows"
  | "visualSettings";

export type GxserverMixedStateArea =
  | "notificationRules"
  | "commandDefinitions"
  | "projectIcons"
  | "theme";

export type GxserverSessionKind = "terminal" | "agent";
export type GxserverSessionSurface = "workspace" | "commands";
export type GxserverSessionTag =
  | "favorite"
  | "high-priority"
  | "research"
  | "todo"
  | "in-progress"
  | "testing"
  | "blocked"
  | "low-priority"
  | "on-hold"
  | "done"
  | "bug"
  | "feature"
  | "design";
export type GxserverDomainLifecycleState = "running" | "sleeping" | "stopped" | "missing" | "unknown";
export type GxserverProviderLifecycleState = "exists" | "missing" | "unknown";
export type GxserverStartupTextDisposition =
  | "discardExistingProvider"
  | "discardUnknownProvider"
  | "none"
  | "queueAfterTerminalReady";
export type GxserverRestoreBlockReason = "missingCwd";

export interface GxserverProjectDomainState {
  attentionRules: Record<string, unknown>;
  completionRules: Record<string, unknown>;
  createdAt: string;
  customAgentOrder: readonly string[];
  customAgents: readonly Record<string, unknown>[];
  customCommandOrder: readonly string[];
  customCommands: readonly Record<string, unknown>[];
  defaultCommand?: string;
  deletedDefaultCommandIds: readonly string[];
  gitConfig: Record<string, unknown>;
  identityIcon?: Record<string, unknown>;
  isFavorite: boolean;
  isPinned: boolean;
  launchSettings: Record<string, unknown>;
  name: string;
  notificationRules: Record<string, unknown>;
  path?: string;
  previousSessionHistory: readonly Record<string, unknown>[];
  projectBoardConfig: Record<string, unknown>;
  projectId: GxserverProjectId;
  runtimeSettings: Record<string, unknown>;
  updatedAt: string;
  worktree?: Record<string, unknown>;
}

export type GxserverConnectionTransport = "local" | "tailscale" | "direct" | "ssh";
export type GxserverConnectionProfileId = string & { readonly __gxserverConnectionProfileId: unique symbol };

export interface GxserverCredentialSecretRef {
  account: string;
  service: "ghostex.gxserver";
}

export interface GxserverConnectionProfile {
  baseUrl?: string;
  createdAt: string;
  id: string;
  name: string;
  serverId?: GxserverServerId;
  sshUrl?: string;
  tokenSecretRef?: GxserverCredentialSecretRef;
  transport: GxserverConnectionTransport;
  updatedAt: string;
}

export interface GxserverConnectionProfilesFile {
  profiles: readonly GxserverConnectionProfile[];
  version: 1;
}

export interface GxserverRouteRef {
  projectId?: GxserverProjectId;
  serverId: GxserverServerId;
  sessionId?: GxserverSessionId;
}

export interface GxserverRemoteProjectListMetadata {
  icon: "cloud";
  profileId: string;
  serverId: GxserverServerId;
  transport: Exclude<GxserverConnectionTransport, "local">;
}

export interface GxserverSshForwardPlan {
  baseUrl: string;
  checkCommand: readonly string[];
  installGuidance: string;
  localPort: number;
  portForwardCommand: readonly string[];
  remoteLocalPort: number;
  startCommand: readonly string[];
}

export interface GxserverRemoteAttachMetadata {
  attachCommand: string;
  profileId: string;
  provider: "zmx";
  serverId?: GxserverServerId;
  transport: "ssh";
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionHiddenMetadata {
  restoredFromHistoryId?: string;
  restoredFromSessionId?: GxserverSessionId;
}

export interface GxserverSessionDomainState {
  agentId?: string;
  attentionRules: Record<string, unknown>;
  commandId?: string;
  completionRules: Record<string, unknown>;
  createdAt: string;
  cwd?: string;
  globalRef: GxserverGlobalSessionRef;
  hiddenMetadata: GxserverSessionHiddenMetadata;
  isFavorite: boolean;
  isPinned: boolean;
  kind: GxserverSessionKind;
  lastActiveAt?: string;
  launchSettings: Record<string, unknown>;
  lifecycleState: GxserverDomainLifecycleState;
  notificationRules: Record<string, unknown>;
  projectId: GxserverProjectId;
  providerState: {
    lifecycleState: GxserverProviderLifecycleState;
    zmxName: GxserverZmxSessionName;
  } & Record<string, unknown>;
  runtimeSettings: Record<string, unknown>;
  sessionId: GxserverSessionId;
  sessionTag?: GxserverSessionTag;
  sidebarOrder?: number;
  surface: GxserverSessionSurface;
  title: string;
  updatedAt: string;
  worktree?: Record<string, unknown>;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverCreateProjectParams {
  attentionRules?: Record<string, unknown>;
  completionRules?: Record<string, unknown>;
  customAgentOrder?: readonly string[];
  customAgents?: readonly Record<string, unknown>[];
  customCommandOrder?: readonly string[];
  customCommands?: readonly Record<string, unknown>[];
  defaultCommand?: string;
  deletedDefaultCommandIds?: readonly string[];
  gitConfig?: Record<string, unknown>;
  identityIcon?: Record<string, unknown>;
  isFavorite?: boolean;
  isPinned?: boolean;
  launchSettings?: Record<string, unknown>;
  name: string;
  notificationRules?: Record<string, unknown>;
  path?: string;
  previousSessionHistory?: readonly Record<string, unknown>[];
  projectBoardConfig?: Record<string, unknown>;
  runtimeSettings?: Record<string, unknown>;
  worktree?: Record<string, unknown>;
}

export type GxserverUpdateProjectParams = Partial<GxserverCreateProjectParams> & {
  projectId: GxserverProjectId;
};

export interface GxserverCreateSessionParams {
  agentId?: string;
  attentionRules?: Record<string, unknown>;
  commandId?: string;
  completionRules?: Record<string, unknown>;
  cwd?: string;
  isFavorite?: boolean;
  isPinned?: boolean;
  kind?: GxserverSessionKind;
  lastActiveAt?: string;
  launchSettings?: Record<string, unknown>;
  lifecycleState?: GxserverDomainLifecycleState;
  notificationRules?: Record<string, unknown>;
  projectId?: GxserverProjectId;
  projectName?: string;
  projectPath?: string;
  providerState?: Partial<GxserverSessionDomainState["providerState"]>;
  restoredFromHistoryId?: string;
  restoredFromSessionId?: GxserverSessionId;
  runtimeSettings?: Record<string, unknown>;
  sessionTag?: GxserverSessionTag | null;
  sidebarOrder?: number;
  surface?: GxserverSessionSurface;
  title?: string;
  worktree?: Record<string, unknown>;
}

export type GxserverUpdateSessionParams = Partial<Omit<GxserverCreateSessionParams, "projectId">> & {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
};

export interface GxserverForkSessionParams extends GxserverSessionLifecycleParams {}

export interface GxserverAgentForkPlan {
  agentId?: string;
  baseCommand?: string;
  displayCommand?: string;
  primaryCommand?: string;
  runtimeCommand?: string;
  startupText?: string;
  startupTextDisposition: GxserverAgentStartupTextDisposition;
}

export interface GxserverForkSessionResult {
  plan: GxserverAgentForkPlan;
  provider?: GxserverStartSessionProviderResult;
  session: GxserverSessionDomainState;
  sourceSession: GxserverSessionDomainState;
}

export interface GxserverUpdateSessionOrderParams {
  projectId: GxserverProjectId;
  sessionIds: readonly GxserverSessionId[];
}

export interface GxserverUpdateSessionOrderResult {
  sessions: readonly GxserverSessionDomainState[];
}

export interface GxserverRemoveSessionParams {
  projectId: GxserverProjectId;
  reason?: string;
  sessionId: GxserverSessionId;
}

export interface GxserverSessionLifecycleParams {
  projectId: GxserverProjectId;
  reason?: string;
  sessionId: GxserverSessionId;
}

export type GxserverSessionTransitionAction = "close" | "sleep";
export interface GxserverSessionTransitionParams extends GxserverSessionLifecycleParams {
  /*
  CDXC:ProjectSidebarOwnership 2026-06-02-13:01:
  gxserver owns the shared lifecycle mutation for close/sleep, but macOS owns selected tab and local pane focus. Keep visual order and focus-target selection out of this protocol so pane-tab layout cannot become gxserver-owned state.
  */
  action: GxserverSessionTransitionAction;
}

export interface GxserverSessionTransitionResult {
  action: GxserverSessionTransitionAction;
  session: GxserverSessionDomainState;
  transition: Record<string, unknown> & {
    session: GxserverSessionDomainState;
  };
}

export interface GxserverCancelFirstPromptAutoTitleParams extends GxserverSessionLifecycleParams {}

export interface GxserverCancelFirstPromptAutoTitleResult {
  changed: boolean;
  previousStatus?: string;
  reason: string;
  session: GxserverSessionDomainState;
}

export type GxserverSessionTitleSource =
  | "browser-auto"
  | "generated"
  | "placeholder"
  | "terminal-auto"
  | "user";

export interface GxserverSessionTitleProjection {
  displayTitle?: string;
  displayTitleTooltip?: string;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  primaryTitle?: string;
  terminalTitle?: string;
  title: string;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
}

export type GxserverPresentationRevision = number & { readonly __gxserverPresentationRevision: unique symbol };
export type GxserverPresentationSessionActivity = "attention" | "idle" | "working";
/*
CDXC:SessionStatus 2026-06-07-00:30:
zmx title observation health is presentation metadata for working-status detection. Publish only coarse watcher states and timestamps so clients can avoid treating unavailable detection as idle without exposing terminal titles, commands, paths, or user content.
*/
export type GxserverTitleObservationStatus = "active" | "failed" | "retrying" | "starting";

export interface GxserverTitleObservationState {
  failureCount?: number;
  lastFailedAt?: string;
  lastObservedAt?: string;
  lastStartedAt?: string;
  nextRetryAt?: string;
  status: GxserverTitleObservationStatus;
}

export interface GxserverPresentationAttentionState {
  acknowledged: boolean;
  enteredAt?: string;
  eventId?: string;
}

export interface GxserverPresentationSessionActions {
  acknowledgeAttention: boolean;
  attach: boolean;
  focus: boolean;
  kill: boolean;
  readText: boolean;
  sendMessage: boolean;
  sendText: boolean;
  sleep: boolean;
  wake: boolean;
}

export interface GxserverPresentationProject {
  createdAt: string;
  groupIds: readonly string[];
  isFavorite: boolean;
  isPinned: boolean;
  path?: string;
  projectId: GxserverProjectId;
  sortKey: string;
  title: string;
  updatedAt: string;
  worktree?: Record<string, unknown>;
}

export interface GxserverPresentationGroup {
  groupId: string;
  projectId: GxserverProjectId;
  sessionIds: readonly GxserverSessionId[];
  sortKey: string;
  title: string;
}

export interface GxserverPresentationSession {
  actions: GxserverPresentationSessionActions;
  activity: GxserverPresentationSessionActivity;
  agentIcon?: string;
  agentId?: string;
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  attention?: GxserverPresentationAttentionState;
  createdAt: string;
  cwd?: string;
  groupId: string;
  isFavorite: boolean;
  isGeneratingFirstPromptTitle: boolean;
  isPinned: boolean;
  shouldSubmitStagedFirstPromptTitleCommand?: boolean;
  kind: GxserverSessionKind;
  lastActiveAt?: string;
  lifecycleState: GxserverDomainLifecycleState;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  sessionTag?: GxserverSessionTag;
  sidebarOrder?: number;
  sortKey: string;
  subtitle?: string;
  surface: GxserverSessionSurface;
  displayTitle?: string;
  displayTitleTooltip?: string;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  primaryTitle?: string;
  terminalTitle?: string;
  title: string;
  titleObservation?: GxserverTitleObservationState;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
  tooltip?: string;
  updatedAt: string;
  visibleInSidebarByDefault: boolean;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverPresentationSnapshot {
  generatedAt: string;
  groups: readonly GxserverPresentationGroup[];
  projects: readonly GxserverPresentationProject[];
  revision: GxserverPresentationRevision;
  sessions: readonly GxserverPresentationSession[];
}

export type GxserverPresentationDelta =
  | {
      domainProject?: GxserverProjectDomainState;
      project: GxserverPresentationProject;
      type: "projectAdded" | "projectUpdated";
    }
  | {
      projectId: GxserverProjectId;
      type: "projectRemoved";
    }
  | {
      group: GxserverPresentationGroup;
      type: "groupAdded" | "groupUpdated" | "groupOrderChanged";
    }
  | {
      groupId: string;
      projectId: GxserverProjectId;
      type: "groupRemoved";
    }
  | {
      session: GxserverPresentationSession;
      type:
        | "sessionAdded"
        | "sessionUpdated"
        | "sessionMoved"
        | "sessionTitleChanged"
        | "sessionActivityChanged"
        | "sessionLifecycleChanged"
        | "sessionSurfaceChanged"
        | "sessionPresentationChanged";
    }
  | {
      projectId: GxserverProjectId;
      sessionId: GxserverSessionId;
      type: "sessionRemoved";
    };

export interface GxserverPresentationDeltaEvent {
  delta: GxserverPresentationDelta;
  revision: GxserverPresentationRevision;
}

export interface GxserverPresentationSubscribeMessage {
  clientId?: string;
  lastRevision?: GxserverPresentationRevision;
  type: "subscribePresentation";
}

export interface GxserverPresentationSearchParams {
  cursor?: string;
  includeActive?: boolean;
  includePrevious?: boolean;
  limit?: number;
  projectId?: GxserverProjectId;
  query?: string;
  sessionTags?: readonly GxserverSessionTag[];
}

export interface GxserverPresentationSearchResult {
  agentId?: string;
  createdAt: string;
  cwd?: string;
  displayTitle?: string;
  displayTitleTooltip?: string;
  isFavorite: boolean;
  isPinned: boolean;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  lastActiveAt?: string;
  lifecycleState: GxserverDomainLifecycleState;
  match?: {
    field: "agent" | "command" | "cwd" | "id" | "project" | "timestamp" | "title";
    snippet?: string;
  };
  projectId: GxserverProjectId;
  projectTitle: string;
  primaryTitle?: string;
  sessionId: GxserverSessionId;
  sessionTag?: GxserverSessionTag;
  sidebarOrder?: number;
  subtitle?: string;
  surface: GxserverSessionSurface;
  terminalTitle?: string;
  title: string;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
  updatedAt: string;
}

export interface GxserverPresentationSearchResponse {
  cursor?: string;
  results: readonly GxserverPresentationSearchResult[];
}

export interface GxserverTerminalTitleEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  previousTerminalTitle?: string;
  protectStoredTitleFromAutomation?: boolean;
  rawTitle?: string;
  sessionPersistenceProvider?: "off" | "tmux" | "zellij" | "zmx";
}

export interface GxserverTerminalTitleEventResult {
  agentSessionId?: string;
  activity: GxserverAgentActivityState;
  changed: boolean;
  enteredAttention: boolean;
  previousActivity: GxserverAgentActivityState["activity"];
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
  visibleTitle?: string;
}

export type GxserverFirstPromptTitleGenerationAgent =
  | "codex"
  | "cursor"
  | "claude"
  | "grok"
  | "custom";

export interface GxserverSessionStateEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  firstPromptTitleGenerationAgent?: GxserverFirstPromptTitleGenerationAgent;
  firstPromptTitleGenerationCommand?: string;
  firstUserMessage?: string;
  startupText?: string;
  title?: string;
  titleSource?: GxserverSessionTitleSource;
}

export interface GxserverSessionStateEventResult {
  changed: boolean;
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
}

export interface GxserverSessionRenameRequestParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  title: string;
  titleSource?: Extract<GxserverSessionTitleSource, "generated" | "user">;
}

export interface GxserverSessionRenameRequestResult {
  changed: boolean;
  pendingAgentMetadata: boolean;
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
  shouldSendAgentRenameCommand: boolean;
}

export interface GxserverAttachSessionMetadataParams extends GxserverSessionLifecycleParams {
  promptEditor?: "monaco";
  startupText?: string;
}

export type GxserverAgentStartupTextDisposition = "none" | "queueAfterTerminalReady";

export interface GxserverAgentLaunchPlanParams {
  agentId: string;
  agentSessionId?: string;
  projectId: GxserverProjectId;
}

export interface GxserverAgentLaunchPlan {
  agentCommand?: string;
  command: string;
  delayedSend?: {
    deadlineAt: string;
    disposition: "scheduled";
  };
  firstUserMessage?: string;
  startupText: string;
  startupTextDisposition: GxserverAgentStartupTextDisposition;
}

export interface GxserverAgentResumePlanParams extends GxserverSessionLifecycleParams {}

export interface GxserverAgentResumePlan {
  agentId?: string;
  baseCommand?: string;
  copyCommand?: string;
  displayCommand?: string;
  fallbackCommand?: string;
  lookupCommand?: string;
  primaryCommand?: string;
  runtimeCommand?: string;
  startupText?: string;
  startupTextDisposition: GxserverAgentStartupTextDisposition;
}

export type GxserverAgentActivityEvent =
  | "acknowledge"
  | "agentDetected"
  | "bell"
  | "escape"
  | "launch"
  | "resume"
  | "terminalError"
  | "terminalExited"
  | "title"
  | "wake";

export interface GxserverAgentActivityState {
  activity: "attention" | "idle" | "working";
  agentName?: "antigravity" | "claude" | "codex" | "copilot" | "cursor" | "gemini" | "opencode" | "pi";
  attentionEventId?: string;
  attentionSuppressedUntil?: string;
  hasSeenWorking?: boolean;
  isAcknowledged?: boolean;
  lastChangedAt?: string;
  lastTitle?: string;
  lastTitleChangeAt?: string;
  suppressedUntil?: string;
  workingSource?: "explicit" | "title";
  workingStartedAt?: string;
}

export interface GxserverAgentActivityInput {
  activity?: GxserverAgentActivityState["activity"];
  agentId?: string;
  event?: GxserverAgentActivityEvent;
  nowIso?: string;
  nowMs?: number;
  settledTitle?: string;
  previous?: unknown;
  title?: string;
}

export interface GxserverUpdateAgentActivityParams extends GxserverSessionLifecycleParams {
  activity?: GxserverAgentActivityState["activity"];
  agentName?: string;
  event?: GxserverAgentActivityEvent;
  nowMs?: number;
  settledTitle?: string;
  title?: string;
}

export interface GxserverUpdateAgentActivityResult {
  activity: GxserverAgentActivityState;
  enteredAttention: boolean;
  previousActivity: GxserverAgentActivityState["activity"];
  session: GxserverSessionDomainState;
}

export interface GxserverProviderProbeResult {
  error?: string;
  lifecycleState: GxserverProviderLifecycleState;
  probedAt: string;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionRestoreBlocked {
  cwd?: string;
  reason: GxserverRestoreBlockReason;
}

export interface GxserverAttachSessionMetadataResult {
  attachCommand?: string;
  cwd?: string;
  persistenceSessionCreated?: boolean;
  provider: "zmx";
  providerState: GxserverProviderProbeResult;
  restoreBlocked?: GxserverSessionRestoreBlocked;
  session: GxserverSessionDomainState;
  startupText?: string;
  startupTextDisposition: GxserverStartupTextDisposition;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverStartSessionProviderParams extends GxserverSessionLifecycleParams {
  promptEditor?: "monaco";
  startupText?: string;
}

export interface GxserverStartSessionProviderResult {
  exitCode?: number;
  provider: "zmx";
  providerState: GxserverProviderProbeResult;
  session: GxserverSessionDomainState;
  started: boolean;
  startupTextDisposition: GxserverStartupTextDisposition;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionProviderProbeResponse {
  provider: "zmx";
  providerState: GxserverProviderProbeResult;
  session: GxserverSessionDomainState;
}

export interface GxserverProviderKillResult {
  error?: string;
  exitCode: number;
  killed: boolean;
  stderr: string;
  stdout: string;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionLifecycleResult {
  attach?: GxserverAttachSessionMetadataResult;
  kill?: GxserverProviderKillResult;
  session: GxserverSessionDomainState;
}

export type GxserverEvent =
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: "eventStreamReady";
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: "serverStarted";
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: "serverStopping";
    }
  | {
      path: GxserverEndpointPath;
      protocolVersion: GxserverProtocolVersion;
      requestId: string;
      serverId: GxserverServerId;
      type: "apiRequestHandled";
    }
  | {
      clientId?: string;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      snapshot: GxserverPresentationSnapshot;
      type: "presentationSnapshot";
    }
  | {
      delta: GxserverPresentationDelta;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: "presentationDelta";
    }
  | {
      command: GxserverRendererCommand;
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: "rendererCommand";
    };
