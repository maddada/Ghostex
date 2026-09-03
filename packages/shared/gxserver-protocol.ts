/*
CDXC:ServerApi 2026-05-30-14:04:
The gxserver protocol is the shared contract for the daemon, future gx/ghostex CLI clients, macOS clients, and remote clients. JSON fields and endpoint path tokens stay camelCase; protocol mismatch is a hard failure that asks the user to update instead of falling back to compatibility behavior.

CDXC:ServerApi 2026-06-04-03:20:
`zmxName` is the canonical provider identity and must carry the full server-project-session id. Clients should treat shorter project/session or compact g-* names as legacy display/state data, not as the reconnect target for gxserver zmx sessions.

CDXC:CefRuntime 2026-06-13-02:24:
CLI commands that still require visible macOS UI, AppKit, CEF, or sidebar-local workspace state must enter through a typed gxserver command contract. gxserver owns auth, protocol checks, dispatch, timeouts, and the supported action list; the macOS app is only the renderer-side executor for behavior that cannot live in the daemon yet.

CDXC:ServerApi 2026-06-22-16:17:
Local starts now rely only on server and no longer keep the deleted gxserver/ TypeScript source tree. Keep the TypeScript protocol contract in packages/shared/ so native web builds and Rust daemon packaging consume an app-owned contract without reaching into gxserver/.
*/

import type { EasyConnectCode, TailscaleCode } from './ghostex-remote-pairing';
import type {
  GxserverSessionChatAppendedEvent,
  GxserverSessionChatReplacedEvent,
  GxserverSessionChatSnapshotEvent,
  GxserverSessionChatStateEvent,
} from './session-chat';

// Session Chat wire types live in ./session-chat (canonical) and are
// re-exported here so protocol consumers keep a single import surface.
export type {
  GxserverAnswerSessionChatPromptParams,
  GxserverAnswerSessionChatPromptResult,
  GxserverHandoffSessionChatDraftParams,
  GxserverHandoffSessionChatDraftResult,
  GxserverInterruptSessionChatParams,
  GxserverInterruptSessionChatResult,
  GxserverQueueSessionChatPromptParams,
  GxserverQueueSessionChatPromptResult,
  GxserverReadSessionChatParams,
  GxserverReadSessionChatQueueParams,
  GxserverReadSessionChatQueueResult,
  GxserverReadSessionChatResult,
  GxserverRemoveSessionChatQueuedPromptParams,
  GxserverReorderSessionChatQueueParams,
  GxserverSendSessionChatMessageParams,
  GxserverSendSessionChatMessageResult,
  GxserverSendSessionChatQueuedPromptParams,
  GxserverSendSessionChatQueuedPromptResult,
  GxserverSessionChatAppendedEvent,
  GxserverSessionChatEvent,
  GxserverSessionChatQueueResult,
  GxserverSessionChatRemoveQueuedPromptResult,
  GxserverSessionChatReplacedEvent,
  GxserverSessionChatSnapshotEvent,
  GxserverSessionChatStateEvent,
  GxserverSetSessionChatDraftParams,
  GxserverSetSessionChatDraftResult,
  GxserverSubscribeSessionChatMessage,
  GxserverUnsubscribeSessionChatMessage,
  GxserverUpdateSessionChatQueuedPromptParams,
  SessionChatDraft,
  SessionChatQueuedPrompt,
  SessionChatQueuedPromptState,
} from './session-chat';

export const GXSERVER_PRODUCT = 'gxserver' as const;
export const GXSERVER_PROTOCOL_VERSION = 1 as const;
export const GXSERVER_LOCAL_API_HOST = '127.0.0.1' as const;
export const GXSERVER_LOCAL_API_PORT = 58744 as const;
export const GXSERVER_REMOTE_API_HOST = '0.0.0.0' as const;
export const GXSERVER_REMOTE_API_PORT = 58745 as const;
export const GXSERVER_MACOS_BRIDGE_PORT = 58743 as const;
export const GXSERVER_TERMINAL_WS_ENDPOINT = '/api/terminal' as const;
export const GXSERVER_WEB_BOOTSTRAP_ENDPOINT = '/api/webBootstrap' as const;

export type GxserverProduct = typeof GXSERVER_PRODUCT;
export type GxserverProtocolVersion = typeof GXSERVER_PROTOCOL_VERSION;
export type GxserverTerminalWsEndpointPath = typeof GXSERVER_TERMINAL_WS_ENDPOINT;
export type GxserverWebBootstrapEndpointPath = typeof GXSERVER_WEB_BOOTSTRAP_ENDPOINT;
export type GxserverServerId = `S${number}${Lowercase<string>}`;
export type GxserverProjectId = `P${number}${Lowercase<string>}`;
export type GxserverSessionId = `G${number}${Lowercase<string>}`;
export type GxserverGlobalSessionRef = `${GxserverServerId}:${GxserverProjectId}:${GxserverSessionId}`;
export type GxserverZmxSessionName = `${GxserverServerId}-${GxserverProjectId}-${GxserverSessionId}`;
export type GxserverAuthToken = string & { readonly __gxserverAuthToken: unique symbol };
export type GxserverLogLevel = 'debug' | 'info' | 'warn' | 'error';
export type GxserverLogOrder = 'asc' | 'desc';
export type GxserverListenerKind = 'local' | 'remote';
export type GxserverApiPermission = 'fullLocal' | 'remoteAllowed' | 'remoteBlocked';
export type GxserverRpcErrorCode =
  | 'badRequest'
  /*
  CDXC:SessionChat 2026-08-26:
  The send was refused because the agent CLI has no input box on screen — it is
  still booting, or a trust/auth/setup screen owns the terminal. Distinct from
  `dependencyUnavailable` (the terminal refused bytes we DID write) because
  nothing was written at all: no clear burst, no paste, and never an Enter.

  The screen behind the refusal is not carried on the error. Read it from
  `/api/readSessionTerminalTail`, which answers with the same verdict plus the
  last thirty lines of the terminal.
  */
  | 'composerNotReady'
  | 'corruptState'
  | 'dependencyUnavailable'
  | 'forbidden'
  /*
  Raised when an ANSWERABLE terminal notice (Claude Code's resume-usage picker)
  owns the input line: the message would confirm a row instead of being sent.
  Emitted by `/api/sendSessionChatMessage` since 2026-08-21; mirrored here so a
  client can distinguish it from a generic internal error.
  */
  | 'invalidState'
  | 'internalError'
  | 'methodNotAllowed'
  | 'notFound'
  | 'notImplemented'
  | 'protocolMismatch'
  | 'projectPathUnavailable'
  | 'unauthorized';

export const GXSERVER_RENDERER_COMMAND_ACTIONS = [
  'assertSidebarCard',
  'clickButton',
  'focusGroup',
  'focusSession',
  'fullReloadSession',
  'moveProject',
  'moveSidebar',
  'openBrowser',
  'openBrowserPane',
  'openPaths',
  'restartSession',
  'renameCommand',
  'runCommand',
  'saveAgent',
  'sendMessage',
  'setViewMode',
  'setVisibleCount',
  'switchProject',
  'toggleSidebarCollapsed',
  'waitFor',
] as const;

export type GxserverRendererCommandAction = (typeof GXSERVER_RENDERER_COMMAND_ACTIONS)[number];

export type GxserverEndpointPath =
  | '/api/health'
  | '/api/health/server'
  | '/api/events'
  | '/api/control/stop'
  | '/api/control/stopAll'
  | '/api/readAgentSettings'
  | '/api/updateAgentSettings'
  | '/api/readAppUserData'
  | '/api/savePinnedPrompt'
  | '/api/saveStashedPrompt'
  | '/api/listStashedPrompts'
  | '/api/deleteStashedPrompt'
  | '/api/listStashedPromptTags'
  | '/api/saveStashedPromptTag'
  | '/api/deleteStashedPromptTag'
  | '/api/setStashedPromptTags'
  | '/api/saveSessionAgentNote'
  | '/api/readSessionAgentNote'
  | '/api/readAgentSkillStatus'
  | '/api/installAgentSkills'
  | '/api/readAgentHookStatus'
  | '/api/installAgentHooks'
  | '/api/uninstallAgentHooks'
  | '/api/ingestAgentHookEvent'
  | '/api/createSession'
  | '/api/createAgentSession'
  | '/api/forkSession'
  /*
   * CDXC:Drafts 2026-08-28:
   * Rewrites which agent CLI a DRAFT session launches. Drafts only — after the
   * first prompt reaches the agent the session's agent is fixed.
   */
  | '/api/switchDraftAgent'
  /*
   * CDXC:AgentProviders 2026-09-03:
   * Moves a PROMPTED session onto another agent configuration of the same CLI
   * family (another account), so the client's Full Reload resumes the same
   * conversation under that agent's command.
   */
  | '/api/switchSessionAgent'
  | '/api/readAgentLaunchPlan'
  | '/api/readAgentResumePlan'
  | '/api/requestSessionRename'
  | '/api/generateSessionTitle'
  | '/api/cancelFirstPromptAutoTitle'
  | '/api/ingestSessionStateEvent'
  | '/api/ingestTerminalTitleEvent'
  | '/api/updateAgentActivity'
  | '/api/readPresentationSnapshot'
  | '/api/readSidebarHud'
  | '/api/mutateSidebarHudSettings'
  | '/api/readWorkspaceSessionGroups'
  | '/api/updateWorkspaceSessionGroups'
  /*
   * CDXC:Navigation 2026-08-19:
   * Titlebar Back/Forward walks a daemon-owned trail of previously active
   * sessions and projects, shared by the gpui desktop titlebar and the web
   * titlebar. See packages/shared/navigation-history for the entry/state contract.
   */
  | '/api/readNavigationHistory'
  | '/api/recordNavigationVisit'
  | '/api/navigateHistory'
  | '/api/readSidebarProjectCollections'
  | '/api/updateSidebarProjectCollections'
  | '/api/assignProjectToSidebarCollection'
  | '/api/readSidebarSpaces'
  | '/api/updateSidebarSpaces'
  | '/api/scheduleDelayedSend'
  | '/api/cancelDelayedSend'
  | '/api/readDelayedSends'
  | '/api/readAutomationState'
  | '/api/saveAutomation'
  | '/api/deleteAutomation'
  | '/api/runAutomationNow'
  | '/api/setAutomationEnabled'
  | '/api/archiveAutomationRun'
  | '/api/markAutomationRunRead'
  | '/api/searchSessions'
  | '/api/listPreviousSessions'
  | '/api/sessionForkBranches'
  | '/api/rewindSessionChat'
  | '/api/selectSessionChatModel'
  | '/api/readSessionTranscriptSizes'
  | '/api/transitionSession'
  | '/api/holdSessionsAwake'
  | '/api/sleepSession'
  | '/api/wakeSession'
  | '/api/startSessionProvider'
  | '/api/killSession'
  | '/api/probeSessionProvider'
  | '/api/listSessions'
  | '/api/removeSession'
  | '/api/readSessionText'
  | '/api/searchAgentPrompts'
  | '/api/readAgentPromptText'
  | '/api/toggleAgentPromptFavorite'
  | '/api/resolveAgentPromptLaunch'
  | '/api/readSessionChat'
  | '/api/readSessionTerminalTail'
  | '/api/readSessionChatSkills'
  | '/api/readSessionChatFiles'
  | '/api/sendSessionChatMessage'
  | '/api/saveSessionChatImage'
  | '/api/saveSessionChatAttachment'
  | '/api/readSessionChatImage'
  | '/api/answerSessionChatPrompt'
  | '/api/interruptSessionChat'
  | '/api/handoffSessionChatDraft'
  | '/api/claimSessionChatLaunchDraft'
  | '/api/readSessionChatQueue'
  | '/api/queueSessionChatPrompt'
  | '/api/updateSessionChatQueuedPrompt'
  | '/api/removeSessionChatQueuedPrompt'
  | '/api/reorderSessionChatQueue'
  | '/api/sendSessionChatQueuedPrompt'
  | '/api/setSessionChatDraft'
  | '/api/listSessionChatDrafts'
  | '/api/exportSessionTranscript'
  | '/api/sendSessionText'
  | '/api/sendSessionMessage'
  | '/api/sendSessionEnter'
  | '/api/focusSession'
  | '/api/dispatchRendererCommand'
  | '/api/attachSessionMetadata'
  | '/api/createProject'
  | '/api/updateProject'
  | '/api/relocateProject'
  | '/api/listProjects'
  | '/api/closeProjectToRecent'
  | '/api/listRecentProjects'
  | '/api/restoreRecentProject'
  | '/api/removeRecentProject'
  | '/api/readProjectStatus'
  | '/api/runProjectDocsAction'
  | '/api/addProjectPath'
  | '/api/createQuickProject'
  | '/api/listProjectWorktrees'
  | '/api/createProjectWorktree'
  | '/api/openProjectWorktree'
  | '/api/mergeWorktreeIntoMain'
  | '/api/checkoutProjectNewBranch'
  | '/api/removeProject'
  | '/api/deleteWorktreeProject'
  | '/api/renameWorktreeProject'
  | '/api/updateSession'
  | '/api/updateSessionOrder'
  | '/api/settleSession'
  | '/api/unsettleSession'
  | '/api/snoozeSession'
  | '/api/unsnoozeSession'
  | '/api/createWorktreeSession'
  | '/api/removeSessionWorktree'
  | '/api/runGitAction'
  | '/api/generateCommitMessage'
  | '/api/createPullRequest'
  | '/api/runGitHubAction'
  | '/api/runWorktreeAction'
  | '/api/runProjectSetupCommand'
  | '/api/runBeadsAction'
  | '/api/previewRepositoryClone'
  | '/api/startRepositoryClone'
  | '/api/readRepositoryCloneJob'
  | '/api/cancelRepositoryCloneJob'
  | '/api/browseProjectDirectories'
  | '/api/createProjectDirectory'
  | '/api/discoverSourceControl'
  | '/api/lookupRepository'
  | '/api/resolveGitRootForPath'
  | '/api/queryLogs'
  | '/api/updateAuth'
  | '/api/updateListenerConfig'
  | '/api/updatePortlessState'
  | '/api/tailcatStatus'
  | '/api/updateTailcatState'
  | '/api/remoteAccessStatus'
  | '/api/enableSshAccess'
  | '/api/remotePairingCode'
  | '/api/pairedDevices'
  | '/api/removePairedDevice'
  | '/api/pairDevice'
  | '/api/pairedDeviceSeen'
  | '/api/installTool'
  | '/api/browseFilesystem'
  | '/api/destructiveAdminAction';

export type GxserverRpcEndpointPath = Exclude<
  GxserverEndpointPath,
  '/api/health' | '/api/health/server' | '/api/events'
>;

export type GxserverLifecycleState =
  'running' | 'stopped' | 'starting' | 'stopping' | 'stale' | 'unreachable' | 'portConflict' | 'protocolMismatch';

export interface GxserverMinimalHealthResponse {
  ok: true;
  product: GxserverProduct;
  protocolVersion: GxserverProtocolVersion;
  version: string;
}

export interface GxserverWebBootstrapResult {
  authToken: GxserverAuthToken;
  baseUrl: string;
  machineLabel: string;
  protocolVersion: GxserverProtocolVersion;
}

export interface GxserverListenerConfig {
  auth?: GxserverListenerAuthConfig;
  enabled: boolean;
  host: string;
  kind: GxserverListenerKind;
  port: number;
}

export interface GxserverListenerAuthConfig {
  mode: 'bearerToken';
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
  skippedReason?: 'alreadyCompleted' | 'noLegacyState';
  sourceFilesRead?: readonly string[];
  status: 'notRun' | 'completed' | 'skipped';
}

export interface GxserverLegacyLogImportStatus {
  filesRead: number;
  malformedLineCount: number;
  migratedLineCount: number;
}

/*
CDXC:Portless 2026-06-23-00:25:
Phase 12 Portless contracts are metadata-only. Health and presentation may expose enums, counts, stable project/session ids, protocol, hostname, and port fields, while action availability remains explicit and local-Mac-only so remote gxserver payloads cannot advertise runnable privileged setup actions.

CDXC:Portless 2026-06-23-04:02:
Phase 14 adds read-only assigned domains to presentation metadata so Settings
can show persisted project/worktree hostnames separately from live route
previews. Keep the payload to stable ids and hostnames; names, paths, full
URLs, command text, process output, and runtime variables stay out.

CDXC:Portless 2026-06-23-04:28:
Phase 16 adds one metadata-only gxserver state update RPC for Portless protocol
changes, admin results, Disable, retry, and explicit service removal. Payloads
must stay enum/boolean/protocol only so setup recovery never carries paths,
commands, process output, URLs, tokens, environment values, or Portless files.
*/
export type GxserverPortlessProtocol = 'https' | 'http';
export type GxserverPortlessSetupOwnership = 'unknown' | 'missing' | 'ghostex' | 'standalone';
export type GxserverPortlessSetupStatus = 'unknown' | 'needed' | 'active' | 'failed' | 'disabled' | 'postponed';
export type GxserverPortlessRuntimeStatus = 'unknown' | 'inactive' | 'active' | 'failed';
export type GxserverPortlessPayloadSourceStatus = 'current' | 'missing' | 'unavailable';
export type GxserverPortlessAdminAction = 'install' | 'reconfigure' | 'retry' | 'remove';
export type GxserverPortlessActionUnavailableReason = 'nativeAdminBridgeRequired' | 'notRecommended';
export type GxserverPortlessRoutePreviewStatus = 'current' | 'disabled' | 'unavailable';
export type GxserverPortlessRoutePreviewKind = 'primary' | 'additional';
export type GxserverPortlessAssignedDomainKind = 'project' | 'worktree';
export type GxserverPortlessStateUpdateParams =
  | {
      enabled: boolean;
      kind: 'setEnabled';
    }
  | {
      kind: 'setProtocol';
      protocol: GxserverPortlessProtocol;
    }
  | {
      action: GxserverPortlessAdminAction;
      kind: 'recordAdminResult';
      ok: boolean;
      protocol?: GxserverPortlessProtocol;
    };

export interface GxserverPortlessStateUpdateResult {
  presentation: GxserverPortlessPresentation;
  status: GxserverPortlessStatus;
}

export interface GxserverPortlessAdminActionAvailability {
  available: boolean;
  localMacOnly: true;
  recommended: boolean;
  unavailableReason?: GxserverPortlessActionUnavailableReason;
}

export type GxserverPortlessAdminActionSet = Record<
  GxserverPortlessAdminAction,
  GxserverPortlessAdminActionAvailability
>;

export interface GxserverPortlessStatus {
  actions: GxserverPortlessAdminActionSet;
  enabled: boolean;
  protocol: GxserverPortlessProtocol;
  runtimeStatus: GxserverPortlessRuntimeStatus;
  setupOwnership: GxserverPortlessSetupOwnership;
  setupStatus: GxserverPortlessSetupStatus;
  sourceStatus: GxserverPortlessPayloadSourceStatus;
  updatedAt?: string;
}

export interface GxserverPortlessRoutePreview {
  hostname: string;
  kind: GxserverPortlessRoutePreviewKind;
  port: number;
  projectId: GxserverProjectId;
  protocol: GxserverPortlessProtocol;
  sessionId: GxserverSessionId;
}

export interface GxserverPortlessAssignedDomain {
  hostname: string;
  kind: GxserverPortlessAssignedDomainKind;
  parentProjectId?: GxserverProjectId;
  projectId: GxserverProjectId;
}

export interface GxserverPortlessPresentation {
  assignedDomains: readonly GxserverPortlessAssignedDomain[];
  liveListenerCount: number;
  routePreviewStatus: GxserverPortlessRoutePreviewStatus;
  routePreviews: readonly GxserverPortlessRoutePreview[];
  status: GxserverPortlessStatus;
}

/**
 * tailcat is the control-plane-free remote-access sidecar gxserver supervises.
 * `token` is the address blob clients dial; it is derived from the daemon-owned
 * server key at runtime and is null until the running sidecar has published it.
 */
export interface GxserverTailcatStatus {
  enabled: boolean;
  running: boolean;
  binaryFound: boolean;
  binaryPath: string | null;
  binaryVersion: string | null;
  token: string | null;
  ports: readonly number[];
  allowedClientKeys: readonly string[];
  lastError: string | null;
}

export interface GxserverTailcatStatusResult {
  status: GxserverTailcatStatus;
}

export type GxserverTailcatStateUpdate =
  | { kind: 'setEnabled'; enabled: boolean }
  | { kind: 'setPorts'; ports: readonly number[] }
  | { kind: 'setAllowedClientKeys'; allowedClientKeys: readonly string[] };

/*
CDXC:RemotePairing 2026-09-03:
Remote access status (SSH access + Tailscale + this computer's identity) and
the pairing codes shown in Settings → Remote. The structured pairing payloads
come from `/api/remotePairingCode`, never from the raw sidecar token, so the
QR can carry the user, ports, and the one-time pairing secret.
*/
export type GxserverRemoteAccessPlatform = 'macos' | 'windows' | 'linux';

export interface GxserverRemoteSshAccessStatus {
  enabled: boolean;
  /** The SSH port that was probed (the one Easy Connect serves). */
  port: number;
  /** ISO timestamp of the probe. */
  checkedAt: string;
  /** Per-OS service detail (launchd / systemd / Windows service state). */
  detail: string | null;
}

export interface GxserverRemoteTailscaleStatus {
  installed: boolean;
  running: boolean;
  /** Signed-in login name, or the tailnet name when the login is unknown. */
  account: string | null;
  /** MagicDNS name without the trailing dot. */
  magicDnsName: string | null;
  /** First Tailscale IP (100.x.y.z). */
  ip: string | null;
  /** Whether this node runs Tailscale SSH; null when Tailscale is not running. */
  sshEnabled: boolean | null;
}

export interface GxserverRemoteAccessStatus {
  computerName: string;
  username: string;
  platform: GxserverRemoteAccessPlatform;
  ssh: GxserverRemoteSshAccessStatus;
  tailscale: GxserverRemoteTailscaleStatus;
}

export type GxserverEnableSshAccessOutcome = 'enabled' | 'cancelled' | 'failed';

export interface GxserverEnableSshAccessResult {
  outcome: GxserverEnableSshAccessOutcome;
  message: string | null;
  /** SSH status re-read after the enable attempt. */
  ssh: GxserverRemoteSshAccessStatus;
}

export interface GxserverRemotePairingCodeResult {
  /** Present only while Easy Connect is running with a published address. */
  easyConnect?: { payload: string; code: EasyConnectCode };
  /** Present only while Tailscale is running with a MagicDNS name or IP. */
  tailscale?: { payload: string; code: TailscaleCode };
}

export interface GxserverPairedDevice {
  id: string;
  name: string;
  platform: string;
  pairedAt: string;
  lastSeenAt: string | null;
  sshKeyFingerprint: string;
}

// M2 (paired devices + pairing registration): served by
// `server/src/remote_access/{paired_devices,pair_device}.rs`.
export interface GxserverPairedDevicesResult {
  devices: readonly GxserverPairedDevice[];
}

export interface GxserverRemovePairedDeviceParams {
  deviceId: string;
}

export interface GxserverRemovePairedDeviceResult {
  devices: readonly GxserverPairedDevice[];
}

export interface GxserverPairDeviceParams {
  secret: string;
  deviceName: string;
  platform: string;
  sshPublicKey: string;
  tailcatClientKey?: string;
}

export interface GxserverPairDeviceResult {
  deviceId: string;
  user: string;
  computerName: string;
}

export interface GxserverPairedDeviceSeenParams {
  deviceId: string;
}

export interface GxserverPairedDeviceSeenResult {
  device: GxserverPairedDevice;
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
  portless?: GxserverPortlessStatus;
  port: typeof GXSERVER_LOCAL_API_PORT;
  serverId: GxserverServerId;
  startedAt: string;
  tools: readonly GxserverToolCapabilityStatus[];
}

export type GxserverToolName = 'zmx' | 'bd';
export type GxserverToolAvailability = 'available' | 'missing' | 'notExecutable' | 'unsupported';
export type GxserverToolResolutionSource = 'devSubmodule' | 'appResource' | 'gxserverBundle';

export interface GxserverToolCapabilityStatus {
  availability: GxserverToolAvailability;
  candidatePaths?: readonly string[];
  capability: 'zmxLifecycle' | 'beadsProjectBoard' | 'deferred';
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
  defaultPromptAgentId: string;
}

export interface GxserverReadAgentSettingsResult {
  isPersisted: boolean;
  settings: GxserverAgentSettings;
}

export interface GxserverUpdateAgentSettingsParams {
  agentAcceptAllEnabled?: boolean;
  defaultPromptAgentId?: string;
}

export type GxserverAgentSkillSourceKind = 'global' | 'pluginCache' | 'repository';

export interface GxserverAgentSkillLocation {
  directoryPath: string;
  providers: readonly string[];
  rootPath: string;
  skillFilePath: string;
  sourceKind: GxserverAgentSkillSourceKind;
}

export interface GxserverAgentSkillStatusRow {
  installed: boolean;
  locations: readonly GxserverAgentSkillLocation[];
  skillName: string;
}

export interface GxserverReadAgentSkillStatusParams {
  repositoryPaths?: readonly string[];
  skillNames?: readonly string[];
}

export interface GxserverReadAgentSkillStatusResult {
  generatedAt: string;
  homeDir: string;
  roots: readonly GxserverAgentSkillDiscoveryRoot[];
  skills: readonly GxserverAgentSkillStatusRow[];
  type: 'agentSkillStatus';
}

export interface GxserverAgentSkillDiscoveryRoot {
  path: string;
  providers: readonly string[];
  sourceKind: GxserverAgentSkillSourceKind;
}

export interface GxserverInstallAgentSkillsParams extends GxserverReadAgentSkillStatusParams {
  agentIds?: readonly string[];
  packageSource?: string;
}

export interface GxserverInstallAgentSkillsResult extends GxserverReadAgentSkillStatusResult {
  installCommand: readonly string[];
  packageSource: string;
  stderr: string;
  stdout: string;
}

export type GxserverAgentHookStatus = 'cliMissing' | 'installed' | 'missing' | 'updateRequired';

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
  type: 'agentHookStatus';
}

export interface GxserverInstallAgentHooksParams {
  agentIds?: readonly string[];
}

export interface GxserverInstallAgentHooksResult extends GxserverReadAgentHookStatusResult {
  installedPaths: readonly string[];
}

export interface GxserverUninstallAgentHooksResult extends GxserverReadAgentHookStatusResult {
  removedPaths: readonly string[];
}

export interface GxserverIngestAgentHookEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  eventName?: string;
  firstUserMessage?: string;
  rawEventName?: string;
  status?: GxserverAgentActivityState['activity'];
  statusUpdatedAt?: string;
  title?: string;
}

export interface GxserverIngestAgentHookEventResult {
  activity?: GxserverAgentActivityState;
  changed: boolean;
  enteredAttention: boolean;
  previousActivity?: GxserverAgentActivityState['activity'];
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
}

export interface GxserverEndpointDescriptor {
  path: GxserverEndpointPath;
  permission: GxserverApiPermission;
  requiresAuth: boolean;
  requiresProtocolVersion: boolean;
  transport: 'http' | 'webSocket';
}

/*
CDXC:ServerDaemon 2026-06-24-13:30:
Pinned Prompts use shared React hydrate fields on every client, but persistence
belongs to gxserver instead of platform-local storage. These RPC payloads can
include user-authored prompt bodies, so clients must not log request params or
daemon response bodies.
*/
export interface GxserverPinnedPrompt {
  content: string;
  createdAt: string;
  promptId: string;
  title: string;
  updatedAt: string;
}

export interface GxserverAppUserData {
  pinnedPrompts: readonly GxserverPinnedPrompt[];
}

export interface GxserverSavePinnedPromptParams {
  content: string;
  promptId?: string;
  title: string;
}

/*
CDXC:SavedPrompts 2026-07-29-00:00:
Stashed prompts are captured server-side on every prompt-editor save-and-close
and recalled from the session "Prompts" modal. projectId/sessionId/cwd are
soft references describing where the prompt was composed; a stash outlives the
project or session it came from. These payloads carry user-authored prompt
bodies, so clients must not log request params or daemon response bodies.
*/
export interface GxserverStashedPrompt {
  content: string;
  createdAt: string;
  cwd: string | null;
  /**
   * Tags filed on this prompt, in rail order (built-ins first). Always present
   * on rows read from gxserver; a prompt with no tags carries an empty array.
   */
  tagIds?: readonly string[];
  /** Origin project's identity icon, shaped for `WorkspaceProjectIconSource`. */
  projectIcon?: unknown;
  projectIconDataUrl?: string | null;
  /** Repository icon discovered by gxserver, matching the active sidebar project icon. */
  projectDiscoveredIconDataUrl?: string | null;
  /**
   * RAW gxserver ids, never the sidebar's combined `combined-session:` key:
   * gxserver normalizes whatever a writer stored as of migration 0026. On a
   * list result these name the session that currently owns the prompt's
   * conversation, which can differ from the session it was stashed from once
   * that one has been resumed or forked.
   */
  projectId: string | null;
  projectName: string | null;
  promptId: string;
  sessionId: string | null;
  /**
   * The agent conversation this prompt was stashed from (migration 0026),
   * absent when there is none to resolve. It follows Claude/Codex
   * compaction-resume rewrites, so it stays valid after the provider mints a
   * successor conversation id.
   */
  agentSessionId?: string;
  /** Current title of the session that owns `agentSessionId`, when resolvable. */
  sessionTitle?: string;
  updatedAt: string;
}

export interface GxserverSaveStashedPromptParams {
  content: string;
  cwd?: string;
  /** When present, updates this saved prompt in place. */
  promptId?: string;
  projectId?: string;
  sessionId?: string;
  /**
   * Explicit filing for a manually saved prompt. Omit this only for a real
   * stash action, which assigns the builtin Stashed tag; an empty array means
   * the user deliberately chose No tag.
   */
  tagIds?: readonly string[];
}

export interface GxserverSaveStashedPromptResult {
  /** True only when this save inserted a new stash row. */
  created: boolean;
  prompt: GxserverStashedPrompt;
}

export interface GxserverListStashedPromptsParams {
  /** When present, results are limited to this project plus its worktree family. */
  projectId?: string;
}

export interface GxserverListStashedPromptsResult {
  prompts: readonly GxserverStashedPrompt[];
  /** The tag catalogue, so the modal paints its rail and its rows together. */
  tags?: readonly GxserverStashedPromptTag[];
}

/*
CDXC:SavedPrompts 2026-08-23:
Saved Prompts are filed under daemon-owned tags. Favorites is not a separate
flag but a seeded builtin tag, so the star control and a user tag write the same
link table. Colors are stored as literal `#rrggbb` because every client
interpolates them into CSS.
*/
export interface GxserverStashedPromptTag {
  color: string;
  createdAt: string;
  /** Builtin tags have stable app-owned behavior and cannot be deleted. */
  isBuiltin: boolean;
  name: string;
  tagId: string;
  updatedAt: string;
}

/** The tagId of the seeded builtin Favorites tag. */
export const GXSERVER_FAVORITE_PROMPT_TAG_ID = 'favorite';
/** The tagId automatically assigned whenever a prompt is stashed. */
export const GXSERVER_STASHED_PROMPT_TAG_ID = 'stashed';

export interface GxserverListStashedPromptTagsResult {
  tags: readonly GxserverStashedPromptTag[];
}

export interface GxserverSaveStashedPromptTagParams {
  color?: string;
  name: string;
  /** When present, renames or recolors this tag instead of creating one. */
  tagId?: string;
}

export interface GxserverSaveStashedPromptTagResult {
  tag: GxserverStashedPromptTag;
  tags: readonly GxserverStashedPromptTag[];
}

export interface GxserverDeleteStashedPromptTagParams {
  tagId: string;
}

export interface GxserverDeleteStashedPromptTagResult {
  deleted: boolean;
  tags: readonly GxserverStashedPromptTag[];
}

export interface GxserverSetStashedPromptTagsParams {
  promptId: string;
  tagIds: readonly string[];
}

export interface GxserverSetStashedPromptTagsResult {
  prompt: GxserverStashedPrompt;
}

export interface GxserverDeleteStashedPromptParams {
  promptId: string;
}

export interface GxserverDeleteStashedPromptResult {
  deleted: boolean;
}

/*
CDXC:SessionNotes 2026-08-24:
A session note is keyed by the PROVIDER conversation id (`agentSessionId`), not
by the ghostex session id, so "what to do next here" survives closing the
ghostex session and resuming the same agent conversation later. Clients address
the note by (projectId, sessionId) and gxserver resolves the agent session id
itself; a session that has no provider conversation yet cannot hold a note.
These payloads carry user-authored note bodies, so clients must not log request
params or daemon response bodies.
*/
export interface GxserverSaveSessionAgentNoteParams {
  /** Trimmed server-side; an empty note deletes the stored note. */
  note: string;
  projectId: string;
  sessionId: string;
}

export interface GxserverSaveSessionAgentNoteResult {
  /** The provider conversation id the note was filed under. */
  agentSessionId: string;
  /** The stored note after trimming; empty string when the note was cleared. */
  note: string;
  /** Canonical session-row project id the note was saved through. */
  projectId?: string;
  /** Canonical session-row session id the note was saved through. */
  sessionId?: string;
}

export interface GxserverReadSessionAgentNoteParams {
  projectId: string;
  sessionId: string;
}

export interface GxserverReadSessionAgentNoteResult {
  /** Absent when the session has no provider conversation id yet. */
  agentSessionId?: string;
  /** Absent when no note is stored; never an empty string. */
  note?: string;
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

/**
 * CDXC:AddProject 2026-08-18:
 * The Add Project dialog can create a destination folder before it adds or
 * clones into it. `name` is a single path segment validated by the daemon (no
 * separators, no `.`/`..`), so the caller names a child of a directory it just
 * browsed rather than an arbitrary path.
 */
export interface GxserverCreateProjectDirectoryParams {
  name: string;
  parentPath: string;
}

export interface GxserverCreateProjectDirectoryResult {
  name: string;
  parentPath: string;
  path: string;
}

export interface GxserverAddProjectPathParams {
  /**
   * Creates the workspace root (`mkdir -p`) when it does not exist yet, which
   * is what the Add Project dialog's "Create & Add" affordance submits. When
   * the flag is absent a missing path is still rejected with `notFound`.
   */
  createIfMissing?: boolean;
  name?: string;
  path: string;
  systemKind?: GxserverProjectDomainState['systemKind'];
  visibility?: GxserverProjectDomainState['visibility'];
}

export type GxserverSourceControlProviderKind = 'azure-devops' | 'bitbucket' | 'github' | 'gitlab';

/**
 * `unsupported` means gxserver itself has no implementation for the provider
 * (Bitbucket / Azure DevOps today), as opposed to `missing`, which means the
 * provider's CLI is simply not installed on that machine.
 */
export type GxserverSourceControlDiscoveryStatus = 'available' | 'missing' | 'unsupported';

export type GxserverSourceControlAuthStatus = 'authenticated' | 'unauthenticated' | 'unknown';

export interface GxserverSourceControlProviderAuth {
  account?: string;
  detail?: string;
  host?: string;
  status: GxserverSourceControlAuthStatus;
}

export interface GxserverSourceControlProviderDiscovery {
  auth: GxserverSourceControlProviderAuth;
  detail?: string;
  executable?: string;
  installHint: string;
  label: string;
  provider: GxserverSourceControlProviderKind;
  status: GxserverSourceControlDiscoveryStatus;
  version?: string;
}

export interface GxserverSourceControlDiscovery {
  checkedAt: string;
  providers: GxserverSourceControlProviderDiscovery[];
}

export interface GxserverDiscoverSourceControlParams {
  cwd?: string;
}

export interface GxserverDiscoverSourceControlResult {
  discovery: GxserverSourceControlDiscovery;
}

export interface GxserverLookupRepositoryParams {
  cwd?: string;
  provider: GxserverSourceControlProviderKind;
  repository: string;
}

export interface GxserverSourceControlRepositoryInfo {
  nameWithOwner: string;
  provider: GxserverSourceControlProviderKind;
  sshUrl: string;
  url: string;
}

export interface GxserverLookupRepositoryResult {
  repository: GxserverSourceControlRepositoryInfo;
}

export interface GxserverStoragePaths {
  authToken: string;
  config: string;
  identity: string;
  logs: string;
  migrations: string;
  root: string;
  runtime: string;
  stateDb: string;
  zmx: string;
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
  truncatedReason?: 'fileWindowExceeded';
}

export type GxserverGitAction =
  | 'branch'
  | 'addAll'
  | 'checkout'
  | 'checkoutNewBranch'
  | 'commit'
  | 'countFileLines'
  | 'deleteLocalBranch'
  | 'deleteRemoteBranch'
  | 'diff'
  | 'diffCached'
  | 'diffCachedFiles'
  | 'diffCachedStatFiles'
  | 'diffCachedNoExt'
  | 'diffCachedStat'
  | 'diffNoExt'
  | 'diffNoIndexAgainstNull'
  | 'diffNumstat'
  | 'getOriginRemoteUrl'
  | 'isInsideWorkTree'
  | 'isUntrackedFile'
  | 'list'
  | 'listBranches'
  | 'listRemotes'
  | 'listUntracked'
  | 'merge'
  | 'pullFastForward'
  | 'push'
  | 'pushSetUpstreamCurrent'
  | 'pushSetUpstream'
  | 'remoteBranchExists'
  | 'status'
  | 'statusPorcelain'
  | 'statusPorcelainZ'
  | 'upstreamCounts'
  | 'verifyRef';
export type GxserverWorktreeAction =
  'create' | 'ensureBeadsHooks' | 'list' | 'pathExists' | 'prune' | 'remove' | 'switch';
export type GxserverBeadsAction =
  | 'addLabel'
  | 'board'
  | 'close'
  | 'comment'
  | 'configGet'
  | 'configGetIssuePrefix'
  | 'configSet'
  | 'create'
  | 'delete'
  | 'depAdd'
  | 'depRemove'
  | 'list'
  | 'listAllLabels'
  | 'renamePrefix'
  | 'removeLabel'
  | 'search'
  | 'setLabels'
  | 'show'
  | 'status'
  | 'storageExists'
  | 'update'
  | 'updateDescription'
  | 'updateEstimate'
  | 'updatePriority'
  | 'updateStatus'
  | 'updateTitle';
export type GxserverBeadsStatus = 'backlog' | 'closed' | 'in_progress' | 'open' | 'review' | 'test';
export type GxserverGitHubAction = 'prCreateFill' | 'prView' | 'version';
export type GxserverProjectSetupAction = 'worktreeSetupCommand';

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

/*
CDXC:Git 2026-06-24-16:11:
Blank GPUI commit messages are generated by gxserver from a registered project and
the review-approved file set. The renderer sends only project id, selected
project-relative paths, and the chosen prompt-agent id; gxserver owns staging,
diff extraction, prompt construction, and generated subject/body parsing.
Remote GPUI may use this only through the saved-machine Rust tunnel, where
Rust validates the machine endpoint and returns only subject/body to CEF.
*/
export interface GxserverGenerateCommitMessageParams extends GxserverProjectOperationScope {
  agentId?: string;
  filePaths: readonly string[];
}

export interface GxserverGenerateCommitMessageResult {
  body: string;
  subject: string;
}

export type GxserverPullRequestState = 'open' | 'closed' | 'merged';

export interface GxserverPullRequestSummary {
  number?: number;
  state: GxserverPullRequestState;
  url: string;
}

/*
CDXC:Git 2026-06-24-16:28:
Direct GPUI PR creation needs a gxserver-owned completion signal before opening the PR or deleting a finished worktree. The RPC accepts only trusted project scope, runs fixed GitHub CLI actions in gxserver, and returns sanitized PR state/URL metadata without raw command output, branch names, titles, commit messages, or shell text.

CDXC:RemoteMachines 2026-06-24-19:25:
Remote GPUI may use this completion signal only as gxserver-confirmed PR state; actual remote browser opens are a separate Rust-owned native side effect that re-runs `prView` through the saved-machine tunnel and opens only a validated HTTPS GitHub PR URL.
*/
export type GxserverCreatePullRequestParams = GxserverProjectOperationScope;

export interface GxserverCreatePullRequestResult {
  created: boolean;
  ok: boolean;
  pr?: GxserverPullRequestSummary;
  reason?: 'createFailed' | 'githubCliUnavailable' | 'invalidResult' | 'viewFailed';
}

export interface GxserverRunWorktreeActionParams extends GxserverProjectOperationScope {
  action: GxserverWorktreeAction;
  baseRef?: string;
  branch?: string;
  force?: boolean;
  worktreePath?: string;
}

/*
CDXC:RemoteMachines 2026-06-24-18:40:
Remote GPUI Add Worktree and Git branch parity must not trust renderer-provided
absolute paths or branch targets. These project-id-scoped RPCs let the owning
gxserver derive worktree paths, opaque open-existing keys, merge branches, and
new commit branches from registered project rows plus bounded user labels.
*/
export interface GxserverProjectWorktreeOption {
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
  worktreeKey: string;
}

export interface GxserverProjectWorktreeListParams {
  projectId: GxserverProjectId;
}

export interface GxserverProjectWorktreeListResult {
  branches: readonly GxserverBranchListEntry[];
  parentProjectId: GxserverProjectId;
  sourceProjectId: GxserverProjectId;
  worktrees: readonly GxserverProjectWorktreeOption[];
}

export interface GxserverCreateProjectWorktreeParams {
  baseRef: string;
  nameHint: string;
  projectId: GxserverProjectId;
}

export interface GxserverOpenProjectWorktreeParams {
  projectId: GxserverProjectId;
  worktreeKey: string;
}

export interface GxserverProjectWorktreeMutationResult {
  project: GxserverProjectDomainState;
}

export interface GxserverMergeWorktreeIntoMainParams {
  projectId: GxserverProjectId;
}

export interface GxserverMergeWorktreeIntoMainResult {
  parentProjectId: GxserverProjectId;
  status: 'conflicts' | 'merged';
}

export interface GxserverCheckoutProjectNewBranchParams {
  branchLabel: string;
  projectId: GxserverProjectId;
}

export interface GxserverCheckoutProjectNewBranchResult {
  checkedOut: true;
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
  | 'localBranchDeleteFailed'
  | 'localBranchNotResolved'
  | 'pruneFailed'
  | 'remoteBranchDeleteFailed'
  | 'remoteBranchNotResolved';

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
  /**
   * CDXC:ProjectBoard 2026-06-13:
   * True only for Project Board originated Beads calls (set by the macOS board bridge). Gates the
   * per-project configurable Beads launch directory (projectBoardConfig.beadsDirectory): board
   * calls opt in, while native Git commit gating probes (storageExists/status on the same endpoint)
   * omit it and stay scoped to the project root.
   */
  projectBoardScope?: boolean;
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

/**
 * Two destination shapes are accepted:
 *
 * - `parentPath` + `destinationFolderName` — the Clone Repository modal's
 *   shape. The parent must already exist and ANY existing destination blocks
 *   the clone.
 * - `destinationPath` — the Add Project dialog's shape: one absolute (or `~/`)
 *   path. Missing parents are created by the clone job, an existing empty
 *   directory is cloned into directly, and an existing non-empty directory is
 *   treated as the parent for a new folder named after the repository.
 *
 * `remoteUrl` is an alias for `repositoryInput`; exactly one of them is
 * required.
 */
export interface GxserverRepositoryClonePreviewParams extends GxserverRepositoryCloneOptions {
  destinationFolderName?: string;
  destinationPath?: string;
  folderPath?: string;
  newFolderName?: string;
  parentPath?: string;
  remoteUrl?: string;
  repositoryInput?: string;
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
  /**
   * Whether the resolved destination refuses the clone. This is what
   * `/api/startRepositoryClone` enforces: it is `destinationExists` for the
   * `parentPath` shape. For the `destinationPath` shape, a non-empty selected
   * directory first resolves to its repository-named child, and only that
   * resolved target can block the clone.
   */
  destinationBlocked: boolean;
  destinationExists: boolean;
  destinationExistsKind?: 'directory' | 'file' | 'other';
  destinationFolderName: string;
  destinationIsEmpty?: boolean;
  destinationPath: string;
  parentPath: string;
  repositoryName: string;
  shallowClone: boolean;
  warning?: string;
}

export type GxserverRepositoryCloneJobState = 'running' | 'completed' | 'failed' | 'canceled';

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
  'aborted' | 'stderrLimitExceeded' | 'stdinFailed' | 'stdoutLimitExceeded' | 'timeout';

export interface GxserverTypedOperationFailure {
  capturedBytes?: number;
  code: GxserverTypedOperationFailureCode;
  limitBytes?: number;
  message: string;
  stream?: 'stderr' | 'stdout';
  timeoutMs?: number;
}

export interface GxserverWorktreeListEntry {
  bare: boolean;
  branch: string;
  detached: boolean;
  path: string;
}

export interface GxserverBranchListEntry {
  current: boolean;
  name: string;
  remote: boolean;
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
  branches?: readonly GxserverBranchListEntry[];
  issue?: Record<string, unknown>;
  worktrees?: readonly GxserverWorktreeListEntry[];
}

export interface GxserverBeadsBoardResult extends GxserverTypedOperationResult {
  issues: readonly Record<string, unknown>[];
}

export type GxserverSharedStateArea =
  | 'projects'
  | 'sessions'
  | 'zmxLifecycle'
  | 'sleepWakePolicy'
  | 'agentStatus'
  | 'remoteControl'
  | 'pinnedFavorite'
  | 'customAgentsCommands'
  | 'launchRuntimeSettings'
  | 'previousSessionHistory'
  | 'worktreeGitActions'
  | 'beadsProjectBoard';

export type GxserverClientLocalStateArea =
  | 'sidebarGroups'
  | 'splitTabLayout'
  | 'visibleSessionCount'
  | 'browserEditorCodeServerPanes'
  | 'cefBrowserProfiles'
  | 'popOutWindows'
  | 'visualSettings';

export type GxserverMixedStateArea = 'notificationRules' | 'commandDefinitions' | 'projectIcons' | 'theme';

export type GxserverSessionKind = 'terminal' | 'agent';
export type GxserverSessionSurface = 'workspace' | 'commands';
export type GxserverSessionTag =
  | 'favorite'
  | 'high-priority'
  | 'research'
  | 'todo'
  | 'in-progress'
  | 'testing'
  | 'blocked'
  | 'low-priority'
  | 'on-hold'
  | 'done'
  | 'bug'
  | 'feature'
  | 'design';
export type GxserverSessionTagFilter = GxserverSessionTag | 'untagged';
export type GxserverDomainLifecycleState = 'running' | 'sleeping' | 'stopped' | 'missing' | 'unknown';
export type GxserverProviderLifecycleState = 'exists' | 'missing' | 'unknown';
export type GxserverStartupTextDisposition =
  'discardExistingProvider' | 'discardUnknownProvider' | 'none' | 'queueAfterTerminalReady';
export type GxserverRestoreBlockReason = 'missingCwd';

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
  /*
  CDXC:Projects 2026-06-24-12:27:
  GPUI Recent Projects must be gxserver-owned project state, not a label/session
  inference. Parked projects stay in the project domain table with explicit
  recent fields so `/api/listRecentProjects` can return only trusted,
  path-bearing rows and presentation can omit them from active groups.
  */
  isRecentProject: boolean;
  launchSettings: Record<string, unknown>;
  name: string;
  notificationRules: Record<string, unknown>;
  path?: string;
  previousSessionHistory: readonly Record<string, unknown>[];
  projectBoardConfig: Record<string, unknown>;
  projectId: GxserverProjectId;
  recentClosedAt?: string;
  runtimeSettings: Record<string, unknown>;
  /*
  CDXC:Projects 2026-06-30-21:23:
  Active project visibility is gxserver-owned so mobile, CLI, GPUI, and macOS omit Remote Attach carrier projects and other hidden containers through the shared daemon contract instead of each client filtering macOS sidebar details.
  */
  systemKind?: 'remoteAttachCarrier';
  updatedAt: string;
  visibility?: 'visible' | 'hidden';
  worktree?: Record<string, unknown>;
}

export interface GxserverRecentProjectDomainState {
  icon?: Record<string, unknown>;
  iconDataUrl?: string;
  path: string;
  projectId: GxserverProjectId;
  recentClosedAt?: string;
  sessionCount: number;
  theme?: string;
  themeColor?: string;
  title: string;
}

/*
CDXC:AgentLauncher 2026-06-24-20:34:
GPUI sidebar and app-modal clients should read normalized launcher/action HUD
rows from gxserver instead of reimplementing project custom-agent/action
projection in host-specific Rust. The endpoint returns the shared Sidebar HUD
JSON shape only; gxserver owns default rows, hidden built-ins, custom row
validation, icon allowlists, display order, deleted default actions, and
active-project command ownership.

CDXC:AgentLauncher 2026-06-24-20:54:
Settings save/delete/order mutations for custom agents and actions use a
narrow gxserver contract instead of renderer-owned `/api/updateProject` field
patches. The payload carries only the explicit Settings intent; gxserver owns
validation, hidden/default semantics, worktree parent command ownership, and
the refreshed HUD/project rows returned after persistence.
*/
export interface GxserverReadSidebarHudParams {
  activeProjectId?: string;
  includeAllProjectCommands?: boolean;
}

export interface GxserverSidebarHudAgentButton {
  acceptAllMode?: 'inherit' | 'enabled' | 'disabled';
  agentId: string;
  command?: string;
  icon?: string;
  isDefault: boolean;
  name: string;
}

export interface GxserverSidebarHudCommandButton {
  actionType: 'browser' | 'terminal';
  closeTerminalOnExit: boolean;
  command?: string;
  commandId: string;
  icon?: string;
  isDefault: boolean;
  links?: readonly GxserverSidebarHudCommandLink[];
  name: string;
  playCompletionSound: boolean;
  showOnProjectRow?: boolean;
  url?: string;
}

export interface GxserverSidebarHudCommandLink {
  target: 'integrated' | 'external';
  url: string;
}

export interface GxserverSidebarHudResponse {
  agents: readonly GxserverSidebarHudAgentButton[];
  /**
   * CDXC:Projects 2026-08-01:
   * Present only when the caller asked for `includeAllProjectCommands`.
   * Keyed by project id with worktrees already resolved to their parent's
   * Actions, mirroring the active-project `commands` resolution per project.
   */
  commandsByProject?: Readonly<Record<string, readonly GxserverSidebarHudCommandButton[]>>;
  commands: readonly GxserverSidebarHudCommandButton[];
  /**
   * CDXC:AgentLauncher 2026-08-01:
   * Global Actions apply to every project and are stored daemon-side rather
   * than in project metadata, so they arrive as their own list instead of
   * inside `commands`. Optional because a gxserver older than the app drops
   * fields it does not know; surfaces normalize the gap to an empty list.
   */
  globalCommands?: readonly GxserverSidebarHudCommandButton[];
}

/**
 * CDXC:Projects 2026-08-01:
 * Any HUD settings mutation returns a full replacement HUD snapshot, so
 * clients that render per-project quick actions ask the mutation for the same
 * opt-in commandsByProject block readSidebarHud serves. The flag rides beside
 * every mutation variant instead of inside one so agent mutations cannot
 * silently drop the per-project rows.
 */
export type GxserverSidebarHudSettingsMutationParams = {
  includeAllProjectCommands?: boolean;
} & GxserverSidebarHudSettingsMutationIntent;

type GxserverSidebarHudSettingsMutationIntent =
  | {
      acceptAllMode?: 'inherit' | 'enabled' | 'disabled';
      activeProjectId?: string;
      agentId?: string;
      command: string;
      icon?: string;
      name: string;
      operation: 'save';
      target: 'agent';
    }
  | {
      activeProjectId?: string;
      agentId: string;
      operation: 'delete';
      target: 'agent';
    }
  | {
      activeProjectId?: string;
      agentIds: readonly string[];
      operation: 'order';
      target: 'agent';
    }
  | {
      actionType: 'browser' | 'terminal';
      activeProjectId?: string;
      closeTerminalOnExit?: boolean;
      command?: string;
      commandId?: string;
      icon?: string;
      links?: readonly GxserverSidebarHudCommandLink[];
      name: string;
      operation: 'save';
      playCompletionSound?: boolean;
      showOnProjectRow?: boolean;
      /**
       * CDXC:AgentLauncher 2026-08-01:
       * Global and Project Actions accept the identical action definition and
       * differ only in ownership, so the target selects the list rather than
       * the payload shape. gxserver validates both through one path, which is
       * what keeps the two lists from drifting into different action shapes.
       */
      target: 'command' | 'globalCommand';
      url?: string;
    }
  | {
      activeProjectId?: string;
      commandId: string;
      operation: 'delete';
      target: 'command' | 'globalCommand';
    }
  | {
      activeProjectId?: string;
      commandIds: readonly string[];
      operation: 'order';
      target: 'command' | 'globalCommand';
    };

export interface GxserverSidebarHudSettingsMutationResult {
  hud: GxserverSidebarHudResponse;
  itemIds?: readonly string[];
  projects: readonly GxserverProjectDomainState[];
}

export type GxserverConnectionTransport = 'local' | 'tailscale' | 'direct' | 'ssh' | 'tailcat';
export type GxserverConnectionProfileId = string & { readonly __gxserverConnectionProfileId: unique symbol };

export interface GxserverCredentialSecretRef {
  account: string;
  service: 'ghostex.gxserver';
}

export interface GxserverConnectionProfile {
  baseUrl?: string;
  createdAt: string;
  id: string;
  name: string;
  /**
   * CDXC:RemotePairing 2026-09-01:
   * The tailcat transport reaches the remote gxserver's own API port through a
   * client-side pipe, so the profile carries the peer's address blob and the
   * port it serves instead of a baseUrl. `remotePort` defaults to the gxserver
   * API port when absent.
   */
  remotePort?: number;
  serverId?: GxserverServerId;
  sshUrl?: string;
  tailcatToken?: string;
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
  icon: 'cloud';
  profileId: string;
  serverId: GxserverServerId;
  transport: Exclude<GxserverConnectionTransport, 'local'>;
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
  provider: 'zmx';
  serverId?: GxserverServerId;
  transport: 'ssh';
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
  isParked?: boolean;
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
  /**
   * CDXC:StateSync 2026-07-29-00:00:
   * Durable Sidebar V2 lifecycle. Writable only through the guarded
   * settle/snooze RPCs — `/api/updateSession` deliberately ignores these keys —
   * and absent when the session has no lifecycle state. The server-internal
   * override stamp gxserver uses to decide when activity has outrun an override
   * is not part of the wire contract.
   */
  settledAt?: string;
  settledOverride?: GxserverPresentationSettledOverride;
  sidebarOrder?: number;
  snoozedAt?: string;
  snoozedUntil?: string;
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
  /**
   * CDXC:Drafts 2026-08-28:
   * Create this agent session as a DRAFT: a real durable row whose agent CLI is
   * started in the background, but which has not received a first prompt yet.
   * gxserver records it as `runtimeSettings.draftStatus = 'draft'` and REMOVES
   * that marker the moment a first user prompt actually reaches the agent, so a
   * promoted draft is indistinguishable from an ordinary session. Never combine
   * with a first prompt (`firstUserMessage` / `firstUserInputDraft`): a flow
   * that already carries a prompt is not a draft.
   */
  draft?: boolean;
  isFavorite?: boolean;
  isParked?: boolean;
  isPinned?: boolean;
  kind?: GxserverSessionKind;
  lastActiveAt?: string;
  launchSettings?: Record<string, unknown>;
  lifecycleState?: GxserverDomainLifecycleState;
  notificationRules?: Record<string, unknown>;
  projectId?: GxserverProjectId;
  projectName?: string;
  projectPath?: string;
  providerState?: Partial<GxserverSessionDomainState['providerState']>;
  /**
   * CDXC:RemoteMachines 2026-06-24-17:19:
   * Remote agent starts can ask gxserver to reject unknown custom/default agent ids instead of creating an inert row with no launch command. This lets clients avoid sending renderer-owned command text while still failing honestly when remote project metadata cannot resolve the selected agent.
   */
  requireLaunchCommand?: boolean;
  restoredFromHistoryId?: string;
  restoredFromSessionId?: GxserverSessionId;
  runtimeSettings?: Record<string, unknown>;
  sessionTag?: GxserverSessionTag | null;
  sidebarOrder?: number;
  surface?: GxserverSessionSurface;
  title?: string;
  worktree?: Record<string, unknown>;
}

export type GxserverUpdateSessionParams = Partial<Omit<GxserverCreateSessionParams, 'projectId'>> & {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
};

/*
CDXC:StateSync 2026-07-29-00:00:
Sidebar V2's settle/snooze commands. gxserver enforces the guards its client
twin (`packages/shared/sidebar-v2-lifecycle.ts`) mirrors: a working or blocked-on-you
session cannot be settled, a blocked-on-you session cannot be snoozed, and a
wake time that is not strictly in the future is rejected rather than silently
normalized. Every command is idempotent — `changed: false` marks a no-op
(double click, bulk settle, waking an awake session), which emits no
presentation delta.
*/
export interface GxserverSettleSessionParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

export type GxserverUnsettleSessionParams = GxserverSettleSessionParams;

export interface GxserverSnoozeSessionParams extends GxserverSettleSessionParams {
  /** ISO wake time; must be strictly in the future. */
  snoozedUntil: string;
}

export type GxserverUnsnoozeSessionParams = GxserverSettleSessionParams;

export interface GxserverSessionLifecycleResult {
  changed: boolean;
  session: GxserverSessionDomainState;
}

/*
CDXC:Worktrees 2026-07-29:
Sidebar V2's worktree flow. A worktree is an ATTRIBUTE of a session (its cwd
plus branch), not a registered sibling project, so ONE call creates the
checkout and the session that lives in it, atomically, server-side.

Contract rules the emitter and every client agree on:
- `projectId` is the PARENT project the worktree is cut from. gxserver derives
  the checkout path, the temp branch (`ghostex/<8hex>`), and the setup command
  from that project; the client never sends paths it invented.
- `baseBranch` omitted means the repository's default branch. `startFromOrigin`
  asks gxserver to fetch first and branch from `origin/<baseBranch>` instead of
  the local ref, so a stale local branch cannot silently seed the worktree.
- `existingWorktree.path` SKIPS creation entirely and spawns the session inside
  that checkout. The path must come from gxserver's own worktree list (or an
  existing session's cwd); gxserver re-validates and normalizes it.
- `firstPrompt` is optional. Without it the session starts idle in the agent,
  exactly like a plain agent launch with no prompt.
- The whole sequence rolls back (worktree removed) if any step fails, so a
  failed call leaves no half-made checkout behind.
*/
export interface GxserverCreateWorktreeSessionExistingWorktree {
  /** Absolute path to an existing checkout on the daemon's machine. */
  path: string;
}

export interface GxserverCreateWorktreeSessionParams {
  agentId?: string;
  baseBranch?: string;
  existingWorktree?: GxserverCreateWorktreeSessionExistingWorktree;
  firstPrompt?: string;
  projectId: GxserverProjectId;
  startFromOrigin?: boolean;
}

export interface GxserverCreateWorktreeSessionResult {
  /** The branch the session's checkout is on — the temp `ghostex/<8hex>` for a
      fresh worktree, or whatever the existing checkout was already on. */
  branch: string;
  sessionId: GxserverSessionId;
  worktreePath: string;
}

/*
CDXC:Worktrees 2026-07-29:
Cleanup for a worktree whose last session just closed. gxserver checks the
checkout for uncommitted work FIRST: `dirty: true` with `removed: false` means
it refused and the client must re-ask with `force`. `warnings` carries bounded,
user-safe notes (a branch that could not be deleted, for instance), never raw
git output.
*/
export interface GxserverRemoveSessionWorktreeParams {
  force?: boolean;
  projectId: GxserverProjectId;
  worktreePath: string;
}

export interface GxserverRemoveSessionWorktreeResult {
  dirty?: boolean;
  removed: boolean;
  warnings?: readonly string[];
}

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

/*
CDXC:Drafts 2026-08-28:
Agent switching is DRAFTS ONLY. A draft has no conversation, so swapping its
agent needs no confirmation and loses nothing: gxserver kills the draft's
background CLI, clears the session's agent identity, rebuilds its launch plan
with the same resolution `/api/createAgentSession` uses, and starts the new
agent's CLI. The call is refused with `invalidState` once the session has been
promoted (its first user prompt reached the agent), because at that point the
transcript, the resume plan, and the session's agent identity all belong to the
agent that produced them.

`agentId` is a project agent id: one of the five chat-supported base families or
a project custom agent whose base family is chat-supported. Clients read the
allowed set from `availableAgents` on `/api/readSessionChat` rather than
building it themselves.
*/
export interface GxserverSwitchDraftAgentParams extends GxserverSessionLifecycleParams {
  agentId: string;
}

export interface GxserverSwitchDraftAgentResult {
  agentId: string;
  /** The provider start for the new agent's CLI, when one ran. */
  provider?: GxserverStartSessionProviderResult;
  session: GxserverSessionDomainState;
}

/*
CDXC:AgentProviders 2026-09-03:
`/api/switchSessionAgent` rewrites which agent configuration a PROMPTED session
launches with, keeping its provider conversation. It does not cycle the
provider: the caller runs Full Reload (sleep, then wake) afterwards, and the
wake's resume plan carries the new agent's command. `agentId` must be one of
the rows the daemon published in the session's `switchableAgents`.
*/
export interface GxserverSwitchSessionAgentParams extends GxserverSessionLifecycleParams {
  agentId: string;
}

export interface GxserverSwitchSessionAgentResult {
  agentId: string;
  plan: GxserverAgentResumePlan;
  session: GxserverSessionDomainState;
}

/** One account a session can be moved to; the same shape as chat's `availableAgents` rows. */
export interface GxserverSwitchableSessionAgent {
  agentId: string;
  baseAgentId: string;
  icon: string;
  name: string;
}

/*
CDXC:SessionChat 2026-08-26:
readSessionTerminalTail answers "is the agent CLI's input box on screen, and if
not, what IS on screen". The daemon reads the same capture every other
screen-state reader takes (a direct zmx socket read, single-digit milliseconds)
and returns the bottom of it.

`composerState` is deliberately three-valued and `unknown` is the common case,
not an error: the daemon has measured composer signatures for nine agent CLIs
and answers `unknown` for every other agent, and for any capture it could not
read. Nothing may treat `unknown` as "not ready" — the daemon itself fails open
on it, and a client that did otherwise would block sends the daemon allows.
*/
export type GxserverSessionComposerState = 'ready' | 'notReady' | 'unknown';

export interface GxserverReadSessionTerminalTailParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /** Overrides the agent the daemon resolves from the session row. */
  agentId?: string;
}

export interface GxserverReadSessionTerminalTailResult {
  /** The agent id the verdict was computed for, when one could be resolved. */
  agentId: string | null;
  /** False when no whole screen could be read; `lines` is then empty. */
  captured: boolean;
  composerState: GxserverSessionComposerState;
  /**
   * Up to 30 ANSI-stripped physical screen rows, OLDEST FIRST, preserving
   * indentation, blank rows, and box-drawing runs. Empty bottom padding is
   * omitted, and the newest painted row is last.
   */
  lines: readonly string[];
  /** User-facing sentence, present only for `notReady`. */
  reason: string | null;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

/*
CDXC:TranscriptExport 2026-08-20:
exportSessionTranscript renders the session's agent transcript into a markdown
file so a NEW agent conversation can be started with that file mentioned. The
transcript only exists on the machine that runs the agent, so clients call this
over their per-machine RPC and the returned path is absolute ON THAT MACHINE —
a remote session's export never lands on the client's disk. The daemon owns the
destination (`<app data dir>/exports`); the caller cannot name a path.

Failures are structured errors, not a degraded export: `unsupportedAgent` (the
session's agent has no transcript format Ghostex parses), `invalidParams` (the
session has not reported an agent session id yet), `transcriptNotFound`,
`transcriptUnreadable` and `transcriptEmpty`.
*/
export interface GxserverExportSessionTranscriptParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /**
   * CDXC:TranscriptExport 2026-08-24:
   * The export dialog's include-toggles. User and agent messages are never
   * optional; these govern the optional record families. Absent values keep
   * the daemon's historical defaults (commands and patches in, reasoning
   * out), which is also what daemons predating the toggles export.
   */
  includeCommands?: boolean;
  includePatches?: boolean;
  includeReasoning?: boolean;
}

export interface GxserverExportSessionTranscriptResult {
  /** Absolute path of the written markdown file, on the daemon's machine. */
  path: string;
  bytes: number;
  /** The transcript the export was parsed from, on the daemon's machine. */
  sourcePath: string;
  /** Records the export actually rendered, after the section selection. */
  renderedEntries: number;
  /** Records parsed out of the transcript, whether rendered or not. */
  parsedEntries: number;
  /** The session's agent (`claude`, `codex`, `grok`, `pi`, …). */
  agent?: string;
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
  /** Why the session is being removed. Free-form and ignored by gxserver. */
  reason?: string;
  sessionId: GxserverSessionId;
}

export interface GxserverRemoveSessionResult {
  session: GxserverSessionDomainState;
}

export interface GxserverSessionLifecycleParams {
  projectId: GxserverProjectId;
  reason?: string;
  sessionId: GxserverSessionId;
}

/**
 * CDXC:Drafts 2026-08-28:
 * One row per session with unsent composer text, from `/api/listSessionChatDrafts`.
 * Clients reconcile their per-keystroke draft cache from this at boot, because
 * that cache does not survive a kill that skips a clean Chromium shutdown —
 * the daemon's SQLite copy does. `updatedAt` is the daemon's ISO stamp of the
 * last synced push, which is what the reconcile compares against.
 */
export interface GxserverSessionChatDraftListEntry {
  content: string;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  updatedAt: string;
}

export interface GxserverListSessionChatDraftsResult {
  drafts: GxserverSessionChatDraftListEntry[];
}

/*
CDXC:KeepAwake 2026-08-19:
A sleep request says WHO asked. `"automatic"` marks a client's "Sleep inactive
agents" sweep; anything else (including an absent field, which is what every
caller sent before this existed) is a user action.

Automatic sleeps can additionally be declined by a live keep-awake lease (see
`GxserverHoldSessionsAwakeParams`) or a never-active session. Every sleep is
declined when the target is no longer running, because stopped history must not
be promoted into the active sleeping lifecycle.
*/
export type GxserverSleepTrigger = 'automatic' | 'user';

export interface GxserverSleepSessionParams extends GxserverSessionLifecycleParams {
  sleepTrigger?: GxserverSleepTrigger;
}

export interface GxserverSleepSessionResult {
  /*
  Present ONLY when the daemon refused the request. `"keptAwake"` means an
  automatic sweep hit a session another client is attached to; `"neverActive"`
  means it hit a session nobody has prompted yet, which has no idle time to
  measure and no conversation to resume; `"notRunning"` means a stale client
  targeted sleeping or stopped history. In every case the session was not
  touched, so a client must not optimistically mark the row sleeping.
  */
  declined?: 'keptAwake' | 'neverActive' | 'notRunning';
  kill?: Record<string, unknown>;
  session: GxserverSessionDomainState;
}

/*
CDXC:KeepAwake 2026-08-19:
A client that is ATTACHED to sessions it does not own panes for — Ghostex mobile
over its SSH CLI bridge — renews a keep-awake lease so the machine's Auto Sleep
sweep cannot retire a terminal the user is looking at on another device.

Contract:
- Leases are in-memory and TTL-bounded on the daemon. Renew well inside `ttlMs`;
  stop renewing and the hold lapses on its own. There is no required release.
- `holderId` scopes the lease to one device, so two phones on one session cannot
  release each other's hold. Absent means a shared default holder.
- `release: true` drops this holder's leases for the listed sessions instead of
  extending them (used when a tab closes, so the session becomes sleepable again
  without waiting out the TTL).
- Ids that do not resolve on that daemon come back in `unknownSessions` instead
  of failing the call: one killed session must not stop the other tabs' holds.
*/
export interface GxserverHoldSessionsAwakeParams {
  holderId?: string;
  release?: boolean;
  sessions: readonly {
    projectId: GxserverProjectId;
    sessionId: GxserverSessionId;
  }[];
  ttlMs?: number;
}

export interface GxserverHoldSessionsAwakeResult {
  holderId: string;
  released: boolean;
  sessions: {
    keepAwakeUntil?: string;
    keptAwake: boolean;
    projectId: GxserverProjectId;
    sessionId: GxserverSessionId;
  }[];
  /** The TTL actually applied after the daemon clamped it. */
  ttlMs: number;
  unknownSessions: {
    projectId: GxserverProjectId;
    sessionId: GxserverSessionId;
  }[];
}

export type GxserverSessionTransitionAction = 'close' | 'sleep';
export interface GxserverSessionTransitionParams extends GxserverSessionLifecycleParams {
  /*
  CDXC:StateSync 2026-06-02-13:01:
  gxserver owns the shared lifecycle mutation for close/sleep, but macOS owns selected tab and local pane focus. Keep visual order and focus-target selection out of this protocol so pane-tab layout cannot become gxserver-owned state.
  */
  action: GxserverSessionTransitionAction;
}

export interface GxserverSessionTransitionResult {
  action: GxserverSessionTransitionAction;
  declined?: 'notRunning';
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

/**
 * CDXC:Drafts 2026-08-28:
 * `'draft'` is projection-only and never durable: gxserver publishes it for a
 * draft session whose synced composer content supplies the row's display title,
 * so a client can tell "the user's unsent text" apart from a real session title.
 */
export type GxserverSessionTitleSource =
  'browser-auto' | 'draft' | 'generated' | 'placeholder' | 'terminal-auto' | 'user';

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
export type GxserverPresentationSessionActivity = 'attention' | 'idle' | 'working';
/*
CDXC:SessionStatus 2026-06-07-00:30:
zmx title observation health is presentation metadata for working-status detection. Publish only coarse watcher states and timestamps so clients can avoid treating unavailable detection as idle without exposing terminal titles, commands, paths, or user content.
*/
export type GxserverTitleObservationStatus = 'active' | 'failed' | 'retrying' | 'starting';

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

/*
CDXC:StateSync 2026-06-15-17:32:
Presentation clients need provider liveness as a first-class field because domain lifecycle and native pane lifecycle are separate resources. A row can remain visible while its zmx provider is missing or persistence is disabled, and clients must not infer provider existence from `running` alone.
*/
export type GxserverPresentationProviderSessionState = 'exists' | 'missing' | 'persistence-disabled' | 'unknown';

export interface GxserverPresentationProject {
  createdAt: string;
  /*
  CDXC:Git 2026-06-24-18:22:
  Remote Sidebar Git preferences need current per-project settings in the same trusted presentation row that supplies the project id. Presentation exposes only sanitized Git preference keys so GPUI can preserve existing values while updating one preference without fetching path-bearing domain project lists through the remote bridge.
  */
  gitConfig?: Record<string, unknown>;
  /*
  CDXC:StateSync 2026-07-29:
  The project's `origin` remote URL, probed server-side with TTL caching like
  the worktree topology probe. Sidebar V2 normalizes it client-side into a
  repository identity so the SAME repo checked out on this Mac and on a remote
  machine reads as ONE logical project.

  Three distinct states, and clients must not collapse them:
  - ABSENT: not probed yet, or the project is not a git work tree at all.
  - `null`: probed, and the repository has no `origin` remote.
  - a string: the raw remote URL exactly as git reports it. Normalization
    (scp-style vs https, `.git` suffix, case) is the CLIENT's job so one
    machine's git version cannot change how another machine's projects group.

  Absent and `null` behave identically for grouping — a project with no usable
  remote never merges with anything — but they are kept apart on the wire so a
  daemon that has not finished probing is distinguishable from a non-git folder.
  */
  gitRemoteOriginUrl?: string | null;
  /*
  CDXC:StateSync 2026-07-29 (P5 fix round):
  The repository root the project sits in (`git rev-parse --show-toplevel`),
  resolved in the SAME server-side probe and cache entry as
  `gitRemoteOriginUrl` above, and — like the URL — keyed on a worktree family's
  ROOT project, so a registered worktree reports its parent checkout's root.

  Only TWO states here: a string, or ABSENT (not a git work tree, not probed
  yet, or a repository whose root git would not report). There is deliberately
  no `null`, because a missing root means only "cannot tell where in the
  repository this project sits" — a fact with no separate wire meaning.

  It exists because Sidebar V2's "Repository + path" grouping mode measures a
  project's path against this root: two sub-projects of one monorepo differ
  only in their path BELOW the root, and without it that mode has nothing to
  measure and degrades to plain repository merging.
  */
  gitRepositoryRootPath?: string;
  /*
  CDXC:Icons 2026-07-29 (discovered icons):
  The icon the PROJECT ITSELF ships, discovered server-side inside the checkout
  and shipped as a `data:` URL. Discovery checks well-known favicon and app-icon
  locations, then an icon declared by an HTML entry
  point's `<link rel="icon">`. Keyed on the worktree FAMILY ROOT like
  `gitRemoteOriginUrl`, so a worktree shows its parent checkout's icon.

  Two states only: a data URL string, or ABSENT (not probed yet, nothing
  discoverable, or an older daemon that does not publish it).

  This is NOT the icon a user attached to the project by hand — that one is
  host-owned and reaches the sidebar through the project overlay's `icon` /
  `iconDataUrl`. They stay SEPARATE wire fields so the client can rank them: a
  user-uploaded image outranks this, and this outranks a typed Tabler glyph.
  Merging them into one field would make that ordering impossible to express.
  */
  discoveredIconDataUrl?: string;
  groupIds: readonly string[];
  isFavorite: boolean;
  isPinned: boolean;
  path?: string;
  pathState?: 'available' | 'missing' | 'notDirectory' | 'unavailable';
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

export type GxserverPresentationSettledOverride = 'active' | 'settled';

/**
 * CDXC:Git 2026-07-29:
 * The state of the change request that owns a session's branch. `draft` is a
 * separate value rather than a flag on `open` because the sidebar paints it in
 * a different (deliberately quiet) hue: a draft is work in progress, not a
 * review waiting on anyone.
 */
export type GxserverPresentationSessionPrState = 'closed' | 'draft' | 'merged' | 'open';

/*
CDXC:Git 2026-07-29:
Per-session git/PR state, probed SERVER-side from the session's own cwd (a
worktree session's cwd is its worktree, so the session is the unit of git
truth). gxserver probes per unique cwd, caches (~60s git, ~5min PR), throttles,
and never blocks a snapshot on a git command.

Field rules the emitter and every client must agree on:
- `branch` is null for a detached HEAD or a cwd that is not a work tree. The
  whole object is simply ABSENT for a session gxserver could not probe (or a
  daemon that predates this feature) — absence is not an error state.
- `additions`/`deletions` are the session worktree measured against the
  merge-base with the repo's default branch, and include both committed-on-
  branch and uncommitted work. They are 0 when there is nothing to report,
  never negative, and the sidebar hides the pair entirely at 0/0.
- The `pr*` fields are present only when `gh` is installed AND authenticated
  AND a change request exists for the branch. No `gh` means no PR fields, not
  an error and not a stale badge.
- `updatedAt` stamps the probe, not the repository, so a client can tell a
  fresh answer from a cached one without inventing its own clock.
*/
export interface GxserverPresentationSessionGitStatus {
  additions: number;
  branch: string | null;
  deletions: number;
  prNumber?: number;
  prState?: GxserverPresentationSessionPrState;
  prUrl?: string;
  updatedAt: string;
}

/*
CDXC:StateSync 2026-07-29-00:00:
Machine-scoped capability flags. A GPUI sidebar merges snapshots from several
gxservers; an older daemon simply omits this object, and Sidebar V2 then hides
settle/snooze affordances and classifies nothing as settled for that machine
instead of inventing lifecycle out of derived data.

CDXC:Git 2026-07-29:
`sessionGitStatus` is optional on top of that, because a daemon can be new
enough to publish this block for settle/snooze and still predate the git probe.
A missing flag means "this machine has no git/PR data to give", and V2 renders
its cards exactly as it does for a session with no `gitStatus` at all.
*/
export interface GxserverPresentationCapabilities {
  sessionGitStatus?: boolean;
  sessionSettlement: boolean;
  sessionSnooze: boolean;
  /**
   * CDXC:Spaces 2026-08-27:
   * `/api/readSidebarSpaces` + `/api/updateSidebarSpaces` are served by this
   * daemon. Absent means the machine has no Spaces at all, and its sidebar
   * section renders its full unfiltered project list — no Space row, no Spaces
   * context submenu, not even the built-in Other view.
   */
  spaces?: boolean;
  /**
   * CDXC:Worktrees 2026-07-29:
   * `/api/createWorktreeSession` + `/api/removeSessionWorktree` are served by
   * this daemon. Optional for the same reason as `sessionGitStatus`: a machine
   * can be new enough for settle/snooze and still predate the worktree flow.
   * Absent means V2's split "+" collapses to the plain instant-session button
   * and the worktree affordances do not render at all.
   */
  worktreeSessions?: boolean;
}

export interface GxserverPresentationSession {
  actions: GxserverPresentationSessionActions;
  activity: GxserverPresentationSessionActivity;
  agentIcon?: string;
  agentId?: string;
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  /**
   * CDXC:SessionFork 2026-08-28:
   * Fork lineage derived by the daemon from its own registry, never from the
   * transcript files: the parent session this conversation branched off, how
   * many VISIBLE branches share its earlier history (present only at two or
   * more), and who those branches are. A daemon that predates fork awareness
   * publishes none of the three.
   */
  forkedFromSessionId?: GxserverSessionId;
  forkBranchCount?: number;
  forkFamilySessionIds?: GxserverSessionId[];
  /** Stable Action identity used to reuse an existing command-surface session. */
  commandId?: string;
  attention?: GxserverPresentationAttentionState;
  createdAt: string;
  cwd?: string;
  /** Daemon-owned Delayed Send state; absent when no send is armed. */
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:Git 2026-07-29:
   * Branch, diff stats, and change-request state for this session's cwd.
   * Absent whenever gxserver has nothing to publish: no probe yet, not a git
   * work tree, or a daemon that predates the probe entirely.
   */
  gitStatus?: GxserverPresentationSessionGitStatus;
  groupId: string;
  /**
   * CDXC:SessionSleep 2026-08-22:
   * Whether this session has EVER entered working or attention, i.e. whether
   * anybody has prompted it. `lastActiveAt` below cannot answer that: the
   * projection falls back to `createdAt` so labels and sorting always have a
   * timestamp, which makes a never-prompted session read as "idle since it was
   * created". Auto Sleep needs the difference, because an agent terminal with
   * no conversation yet cannot be resumed after its provider is killed.
   * Absent from daemons that predate this field; treat that as not-yet-active.
   */
  hasEverBeenActive?: boolean;
  /**
   * CDXC:Drafts 2026-08-28:
   * This session was created from the sidebar and has not received its first
   * user prompt yet. Present ONLY while the session is a draft (never `false`),
   * which is also what a daemon that predates drafts publishes. Sidebars render
   * a draft inline in its normal position with a pencil glyph instead of the
   * agent logo and a dimmed title; `displayTitle` carries the first line of the
   * user's unsent composer text once any exists (`titleSource: 'draft'`).
   */
  isDraft?: true;
  isFavorite: boolean;
  isGeneratingFirstPromptTitle: boolean;
  isParked?: boolean;
  isPinned: boolean;
  kind: GxserverSessionKind;
  lastActiveAt?: string;
  lifecycleState: GxserverDomainLifecycleState;
  /**
   * CDXC:AgentScreenDetection 2026-07-29-12:00:
   * `meaningfulActivityAt` is the recency clients sort by: working blips
   * shorter than gxserver's meaningful threshold never advance it, while a
   * meaningfully working session's value advances live with each snapshot.
   * `workingStartedAt` is published while the session is effectively working
   * so sorters can tell whether the current stint has qualified yet.
   * `lastActiveAt` stays raw (any working/attention entry) for auto-sleep and
   * Last Active labels. Both fields are optional for older remote daemons.
   */
  meaningfulActivityAt?: string;
  workingStartedAt?: string;
  providerSessionState: GxserverPresentationProviderSessionState;
  projectId: GxserverProjectId;
  /**
   * CDXC:SessionChat 2026-08-21-b:
   * How many Ghostex-owned chat prompts are held for this session, so the
   * sidebar can badge the agent icon without subscribing to every session's
   * chat. EVERY row counts, `failed` included: a queue stalled behind a failed
   * row is precisely the state that needs the user, and leaving those rows out
   * made a dead queue look identical to no queue. The key is ABSENT at zero —
   * never `0` — which is also what a daemon that predates the queue publishes,
   * so both mean the same thing to a client: no badge.
   */
  queuedPromptCount?: number;
  /**
   * CDXC:SessionChat 2026-08-21-b:
   * How many of those rows are `failed` (delivery attempted, held for the user
   * to retry or delete). Non-zero turns the badge red instead of yellow, and
   * `queuedPromptCount - queuedPromptFailedCount` is what still counts as work
   * the agent is going to receive. ABSENT means none failed.
   */
  queuedPromptFailedCount?: number;
  sessionId: GxserverSessionId;
  /**
   * CDXC:SessionNotes 2026-08-24:
   * The full note text the user filed against this session's provider
   * conversation (`agentSessionId`), so sidebar rows can show the note in their
   * tooltip and mark the row without a per-session read. The key is ABSENT when
   * there is no note — never an empty string — which is also what a daemon that
   * predates session notes publishes.
   */
  sessionNote?: string;
  /**
   * Prompts saved from this provider conversation, including legacy rows that
   * still carry only the raw Ghostex session id. Absent at zero and on daemons
   * that predate the terminal action-bar badge.
   */
  stashedPromptCount?: number;
  /**
   * CDXC:AgentProviders 2026-09-03:
   * The same-family agent configurations (accounts) this prompted session can
   * be resumed under, resolved by the owning daemon. ABSENT when there is
   * nothing to switch to and on daemons that predate the feature.
   */
  switchableAgents?: readonly GxserverSwitchableSessionAgent[];
  sessionPersistenceProvider?: 'tmux' | 'zmx' | 'zellij';
  sessionTag?: GxserverSessionTag;
  sendWhenAllProjectSessionsStopActive?: boolean;
  sendWhenAgentStopsActive?: boolean;
  /**
   * CDXC:StateSync 2026-07-29-00:00:
   * Server-owned Sidebar V2 inbox lifecycle. `settledOverride` is the explicit
   * user pin — "settled" forces the settled shelf, "active" pins the session
   * into the inbox and suppresses auto-settle — and gxserver clears it once
   * real activity outruns it. `settledAt` is stamped only by an explicit
   * settle; an inactivity auto-settle deliberately leaves it absent so the
   * settled shelf sorts the row by when its work ended. `snoozedUntil` is the
   * wake time and `snoozedAt` the moment the snooze was set; the wake itself is
   * derived from `snoozedUntil` (no event fires when it passes), and a snoozed
   * session that raises its hand stays snoozed here while clients surface it.
   * All four are absent when the session has no lifecycle state, which is also
   * what an older remote daemon publishes.
   */
  settledAt?: string;
  settledOverride?: GxserverPresentationSettledOverride;
  sidebarOrder?: number;
  snoozedAt?: string;
  snoozedUntil?: string;
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

/*
CDXC:Projects 2026-07-18-00:00:
Colored "Group N" project collections are server-owned structure shared by the
desktop sidebar and React Native Android. Expansion is client-local UI state.
The wire state is fully normalized by
gxserver: `order` is the authoritative collection ordering, `collections` is
keyed by collectionId, a project id appears in at most one collection, and
collections with no project ids are dropped. Clients write-through-sync the
whole state via /api/updateSidebarProjectCollections and read it back from the
same endpoint, the presentation snapshot, or the mobile session summary.
*/
export interface GxserverSidebarProjectCollection {
  collectionId: string;
  color: string;
  projectIds: readonly string[];
  title: string;
}

export interface GxserverSidebarProjectCollectionsState {
  collections: Readonly<Record<string, GxserverSidebarProjectCollection>>;
  nextCollectionNumber: number;
  order: readonly string[];
}

/*
CDXC:Spaces 2026-08-27:
A Space is a server-owned saved sidebar filter: a name, an icon id, a color, a
manual position, and the sidebar members it shows. Members are sidebar project
collections ("groups") and ungrouped projects, and a member may belong to any
number of Spaces. gxserver owns the document so every client on that daemon
shares one Space set, and a remote daemon's Spaces stay that daemon's own.

The wire state is fully normalized by gxserver:
  - `order` is the authoritative Space ordering; `spaces` is keyed by spaceId.
  - A project held by a collection can never carry direct membership, so
    gxserver strips grouped project ids from `memberProjectIds`.
  - Member collection ids that no longer exist are dropped; a collection
    disappears from the collections document as soon as it empties.
  - Member ids are deduped, and ids/names/icon ids are bounded (256 chars,
    512 ids per list, 256 Spaces).
  - `color` is normalized to lowercase `#rrggbb`, falling back to the shared
    sidebar palette.
  - An EMPTY Space is valid and kept, unlike an empty project collection.
Member project ids for a deleted project may linger as soft references, so
clients must tolerate member ids they cannot resolve. Worktree inheritance and
the built-in "Other" view (packages/shared/sidebar-spaces-other.ts) are pure
client concerns and never stored.
Clients write-through-sync the whole state via /api/updateSidebarSpaces and read
it back from the same endpoint, the presentation snapshot, or the
`sidebarSpacesChanged` event.
*/
export interface GxserverSidebarSpace {
  color: string;
  icon: string;
  memberCollectionIds: readonly string[];
  memberProjectIds: readonly string[];
  name: string;
  spaceId: string;
}

export interface GxserverSidebarSpacesState {
  order: readonly string[];
  spaces: Readonly<Record<string, GxserverSidebarSpace>>;
}

export interface GxserverWorkspaceSessionGroup {
  groupId: string;
  sessionIds: readonly string[];
  title: string;
}

export interface GxserverWorkspaceProjectGroups {
  groups: readonly GxserverWorkspaceSessionGroup[];
  nextGroupNumber?: number;
}

export interface GxserverWorkspaceSessionGroupsState {
  projectOrder: readonly string[];
  projects: Readonly<Record<string, GxserverWorkspaceProjectGroups>>;
}

export interface GxserverReadSidebarProjectCollectionsResult {
  sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
}

export interface GxserverUpdateSidebarProjectCollectionsParams {
  state: GxserverSidebarProjectCollectionsState;
}

export interface GxserverUpdateSidebarProjectCollectionsResult {
  sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
}

export interface GxserverReadSidebarSpacesResult {
  sidebarSpaces: GxserverSidebarSpacesState;
}

export interface GxserverUpdateSidebarSpacesParams {
  state: GxserverSidebarSpacesState;
}

export interface GxserverUpdateSidebarSpacesResult {
  sidebarSpaces: GxserverSidebarSpacesState;
}

export interface GxserverPresentationSnapshot {
  /*
  CDXC:StateSync 2026-07-29:
  The inactivity window THIS daemon actually applies in its auto-settle sweep,
  in days. One sidebar renders rows from several daemons, and each daemon reads
  its OWN `sidebarAutoSettleAfterDays`, so a client that applied the local
  window to every machine would park remote sessions the remote daemon still
  considers active (the recorded P2 minor).

  - ABSENT: this daemon predates the field. The client then keeps the P2
    behavior for LOCAL rows (the local settings value, which is the same file
    the local daemon reads) and applies NO client-side inactivity settle to
    remote rows — the remote server's own `settledOverride` is the only truth
    for a machine that cannot state its window.
  - `null`: this daemon has inactivity auto-settle disabled.
  - a number: that daemon's window in days.
  */
  autoSettleAfterDays?: number | null;
  capabilities?: GxserverPresentationCapabilities;
  generatedAt: string;
  groups: readonly GxserverPresentationGroup[];
  portless?: GxserverPortlessPresentation;
  projects: readonly GxserverPresentationProject[];
  revision: GxserverPresentationRevision;
  sessions: readonly GxserverPresentationSession[];
  sidebarProjectCollections?: GxserverSidebarProjectCollectionsState;
  sidebarSpaces?: GxserverSidebarSpacesState;
  workspaceGroups?: GxserverWorkspaceSessionGroupsState;
}

export type GxserverPresentationDelta =
  | {
      domainProject?: GxserverProjectDomainState;
      project: GxserverPresentationProject;
      type: 'projectAdded' | 'projectUpdated';
    }
  | {
      projectId: GxserverProjectId;
      type: 'projectRemoved';
    }
  | {
      group: GxserverPresentationGroup;
      type: 'groupAdded' | 'groupUpdated' | 'groupOrderChanged';
    }
  | {
      groupId: string;
      projectId: GxserverProjectId;
      type: 'groupRemoved';
    }
  | {
      session: GxserverPresentationSession;
      type:
        | 'sessionAdded'
        | 'sessionUpdated'
        | 'sessionMoved'
        | 'sessionTitleChanged'
        | 'sessionActivityChanged'
        | 'sessionLifecycleChanged'
        | 'sessionSurfaceChanged'
        | 'sessionPresentationChanged';
    }
  | {
      projectId: GxserverProjectId;
      sessionId: GxserverSessionId;
      type: 'sessionRemoved';
    };

export interface GxserverPresentationDeltaEvent {
  delta: GxserverPresentationDelta;
  revision: GxserverPresentationRevision;
}

export interface GxserverPresentationSubscribeMessage {
  clientId?: string;
  lastRevision?: GxserverPresentationRevision;
  type: 'subscribePresentation';
}

export interface GxserverPresentationSearchParams {
  cursor?: string;
  includeActive?: boolean;
  includePrevious?: boolean;
  limit?: number;
  projectId?: GxserverProjectId;
  query?: string;
  sessionTags?: readonly GxserverSessionTagFilter[];
}

export interface GxserverPresentationSearchResult {
  agentIcon?: string;
  agentId?: string;
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  /**
   * CDXC:Sessions 2026-06-17-17:06:
   * Previous Sessions list/search responses expose close time separately from lastActiveAt so clients can group and sort restore rows by when the session was closed while still rendering Last Active from actual user activity.
   */
  closedAt?: string;
  createdAt: string;
  cwd?: string;
  displayTitle?: string;
  displayTitleTooltip?: string;
  isFavorite: boolean;
  isParked?: boolean;
  isPinned: boolean;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  lastActiveAt?: string;
  lifecycleState: GxserverDomainLifecycleState;
  match?: {
    field: 'agent' | 'command' | 'cwd' | 'id' | 'project' | 'timestamp' | 'title';
    snippet?: string;
  };
  projectId: GxserverProjectId;
  projectTitle: string;
  primaryTitle?: string;
  sessionId: GxserverSessionId;
  /**
   * CDXC:Sessions 2026-06-13-15:36:
   * Previous Sessions search results must carry the same identity and provider metadata needed to render and restore stopped agent rows without rehydrating native sidebar history. Keep raw prompt/user text out of list/search responses; restore-specific command construction stays behind readAgentResumePlan for the selected session.
   */
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: 'tmux' | 'zmx' | 'zellij';
  sessionTag?: GxserverSessionTag;
  sidebarOrder?: number;
  subtitle?: string;
  surface: GxserverSessionSurface;
  terminalTitle?: string;
  title: string;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
  updatedAt: string;
  zmxName?: GxserverZmxSessionName;
}

export interface GxserverPresentationSearchResponse {
  cursor?: string;
  results: readonly GxserverPresentationSearchResult[];
}

/**
 * CDXC:SessionFork 2026-08-28:
 * `/api/sessionForkBranches` answers "what else shares this conversation's
 * history". The daemon derives the family from its own registry, so the reply
 * includes the ancestors Previous Sessions hides once something continues from
 * them, flagged `ancestor`. A session with no relatives answers with just
 * itself, which is why a caller can ask unconditionally and gate its UI on the
 * branch count instead of on an error.
 */
export interface GxserverSessionForkBranchesParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

export interface GxserverSessionForkBranch {
  /** Present only for a superseded row: a branch with no card of its own. */
  ancestor?: boolean;
  agentSessionId?: string;
  /** True for the session that asked. */
  current: boolean;
  lastActiveMs: number;
  lifecycleState: GxserverDomainLifecycleState;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  title: string;
}

export interface GxserverSessionForkBranchesResult {
  /** Newest activity first. */
  branches: readonly GxserverSessionForkBranch[];
}

/**
 * CDXC:SessionChat 2026-09-02:
 * `/api/rewindSessionChat` drives the agent's own rewind flow in its terminal
 * (Claude: `/rewind`, pick the prompt, "Restore conversation") to the point
 * before `messageId`, a user prompt row of the active conversation. The daemon
 * verifies every dialog step against the screen and cancels on any mismatch.
 * On success the chat readers hide the rewound rows immediately; the transcript
 * confirms the branch when the next prompt is sent. Claude only for now.
 */
export interface GxserverRewindSessionChatParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /** Id of the user prompt row to rewind to (the point before it was sent). */
  messageId: string;
}

export interface GxserverRewindSessionChatResult {
  ok: true;
  /** Row id the conversation is now positioned before. */
  targetMessageId: string;
  /** `uuid` of the new active leaf; null when rewound to before the first prompt. */
  leafId: string | null;
}

/**
 * CDXC:AgentScreenDetection 2026-09-03 WHY:
 * `/api/selectSessionChatModel` drives Codex's own `/model` picker in the
 * session's terminal (the numbered model list, then the numbered reasoning
 * list) because Codex has no command form for it: `/model <name>` is sent to
 * the model as a prompt. The daemon reads the row digits off the screen and
 * aborts on any mismatch. Codex only.
 */
export interface GxserverSelectSessionChatModelParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /** Codex model id as its picker lists it (`gpt-5.6-sol`). */
  model: string;
  /** Effort id the reasoning list must offer for that model (`high`). */
  effort: string;
}

export interface GxserverSelectSessionChatModelResult {
  ok: true;
  model: string;
  effort: string;
}

export interface GxserverTerminalTitleEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  previousTerminalTitle?: string;
  protectStoredTitleFromAutomation?: boolean;
  rawTitle?: string;
  sessionPersistenceProvider?: 'off' | 'tmux' | 'zellij' | 'zmx';
}

export interface GxserverTerminalTitleEventResult {
  agentSessionId?: string;
  activity: GxserverAgentActivityState;
  changed: boolean;
  enteredAttention: boolean;
  previousActivity: GxserverAgentActivityState['activity'];
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
  visibleTitle?: string;
}

export type GxserverFirstPromptTitleGenerationAgent = 'codex' | 'cursor' | 'claude' | 'grok' | 'custom';

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
  titleSource?: Extract<GxserverSessionTitleSource, 'generated' | 'user'>;
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
  promptEditor?: 'code-server' | 'monaco';
  startupText?: string;
}

export type GxserverAgentStartupTextDisposition = 'none' | 'queueAfterTerminalReady';

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
    disposition: 'scheduled';
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
  | 'acknowledge'
  | 'agentDetected'
  | 'bell'
  | 'escape'
  | 'launch'
  | 'resume'
  | 'terminalError'
  | 'terminalExited'
  | 'title'
  | 'wake';

export interface GxserverAgentActivityState {
  activity: 'attention' | 'idle' | 'working';
  agentName?: 'antigravity' | 'claude' | 'codex' | 'copilot' | 'cursor' | 'gemini' | 'opencode' | 'pi';
  attentionEventId?: string;
  attentionSuppressedUntil?: string;
  hasSeenWorking?: boolean;
  isAcknowledged?: boolean;
  lastChangedAt?: string;
  lastMeaningfulActivityAt?: string;
  lastTitle?: string;
  lastTitleChangeAt?: string;
  suppressedUntil?: string;
  workingSource?: 'explicit' | 'title';
  workingStartedAt?: string;
}

export interface GxserverAgentActivityInput {
  activity?: GxserverAgentActivityState['activity'];
  agentId?: string;
  event?: GxserverAgentActivityEvent;
  nowIso?: string;
  nowMs?: number;
  settledTitle?: string;
  previous?: unknown;
  title?: string;
}

export interface GxserverUpdateAgentActivityParams extends GxserverSessionLifecycleParams {
  activity?: GxserverAgentActivityState['activity'];
  agentName?: string;
  event?: GxserverAgentActivityEvent;
  nowMs?: number;
  settledTitle?: string;
  title?: string;
}

export interface GxserverUpdateAgentActivityResult {
  activity: GxserverAgentActivityState;
  enteredAttention: boolean;
  previousActivity: GxserverAgentActivityState['activity'];
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
  provider: 'zmx';
  providerState: GxserverProviderProbeResult;
  restoreBlocked?: GxserverSessionRestoreBlocked;
  session: GxserverSessionDomainState;
  startupText?: string;
  startupTextDisposition: GxserverStartupTextDisposition;
  zmxName: GxserverZmxSessionName;
}

export type GxserverTerminalWsErrorCode = 'unauthorized' | 'protocolMismatch' | 'notFound' | 'providerNotRunning';

export interface GxserverTerminalWsReadyMessage {
  cols: number;
  rows: number;
  type: 'ready';
  zmxName: GxserverZmxSessionName;
}

export interface GxserverTerminalWsExitMessage {
  code: number | null;
  type: 'exit';
}

export interface GxserverTerminalWsErrorMessage {
  code: GxserverTerminalWsErrorCode;
  message: string;
  type: 'error';
}

export interface GxserverTerminalWsResizeMessage {
  cols: number;
  rows: number;
  type: 'resize';
}

export type GxserverTerminalWsClientControlMessage = GxserverTerminalWsResizeMessage;
export type GxserverTerminalWsServerControlMessage =
  GxserverTerminalWsReadyMessage | GxserverTerminalWsExitMessage | GxserverTerminalWsErrorMessage;
export type GxserverTerminalWsControlMessage =
  GxserverTerminalWsClientControlMessage | GxserverTerminalWsServerControlMessage;

export interface GxserverStartSessionProviderParams extends GxserverSessionLifecycleParams {
  promptEditor?: 'code-server' | 'monaco';
  startupText?: string;
}

export interface GxserverStartSessionProviderResult {
  exitCode?: number;
  provider: 'zmx';
  providerState: GxserverProviderProbeResult;
  session: GxserverSessionDomainState;
  started: boolean;
  startupTextDisposition: GxserverStartupTextDisposition;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionProviderProbeResponse {
  provider: 'zmx';
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
      type: 'eventStreamReady';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'serverStarted';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'serverStopping';
    }
  | {
      path: GxserverEndpointPath;
      protocolVersion: GxserverProtocolVersion;
      requestId: string;
      serverId: GxserverServerId;
      type: 'apiRequestHandled';
    }
  | {
      clientId?: string;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      snapshot: GxserverPresentationSnapshot;
      type: 'presentationSnapshot';
    }
  /*
   * CDXC:StateSync 2026-09-01:
   * The reply to a `subscribePresentation` whose `lastRevision` already names
   * the daemon's current revision. It carries no snapshot on purpose: the
   * subscriber has missed nothing, so re-sending the projection it just applied
   * over HTTP would be pure duplicate work on every reconnect. Any other
   * `lastRevision` — missing, behind (deltas were missed), or ahead — still
   * gets the full `presentationSnapshot`.
   */
  | {
      clientId?: string;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'presentationSnapshotCurrent';
    }
  | {
      delta: GxserverPresentationDelta;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'presentationDelta';
    }
  | {
      command: GxserverRendererCommand;
      protocolVersion: GxserverProtocolVersion;
      serverId: GxserverServerId;
      type: 'rendererCommand';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
      type: 'sidebarProjectCollectionsChanged';
    }
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      sidebarSpaces: GxserverSidebarSpacesState;
      type: 'sidebarSpacesChanged';
    }
  | {
      groups: GxserverWorkspaceSessionGroupsState;
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'workspaceGroupsChanged';
    }
  /*
   * CDXC:AgentLauncher 2026-08-07:
   * A Global Action write is not a project write, so it produces no
   * presentation delta and live surfaces would otherwise keep a stale list.
   * The event announces the change and bumps the presentation revision; it
   * carries no commands, because `/api/readSidebarHud` stays the single
   * projection of the Global Actions list.
   */
  | {
      protocolVersion: GxserverProtocolVersion;
      revision: GxserverPresentationRevision;
      serverId: GxserverServerId;
      type: 'globalSidebarCommandsChanged';
    }
  | GxserverSessionChatSnapshotEvent
  | GxserverSessionChatAppendedEvent
  | GxserverSessionChatReplacedEvent
  | GxserverSessionChatStateEvent;
