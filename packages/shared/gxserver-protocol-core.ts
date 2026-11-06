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
export type GxserverAuthToken = string & {
  readonly __gxserverAuthToken: unique symbol;
};
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
  | 'composerNotCleared'
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
  /*
  The send was cancelled by the user's own Escape before its Enter was written
  (`/api/interruptSessionChat` bumps the queue generation under it). Nothing
  reached the agent, so the composer restores the text silently instead of
  announcing a delivery failure.
  */
  | 'sendCancelled'
  | 'internalError'
  | 'methodNotAllowed'
  | 'messageNotFound'
  | 'notFound'
  | 'notImplemented'
  | 'protocolMismatch'
  | 'projectPathUnavailable'
  | 'unauthorized';

export const GXSERVER_RENDERER_COMMAND_ACTIONS = [
  'clickButton',
  'focusGroup',
  'focusSession',
  'fullReloadSession',
  'moveProject',
  'openBrowser',
  'openBrowserPane',
  'openPaths',
  'openSettings',
  'readFocusedSession',
  'readResourcesSnapshot',
  'restartSession',
  'renameCommand',
  'runCommand',
  'switchProject',
  'toggleCloseAfterDone',
  'toggleSidebarCollapsed',
  'updateSettingsPatch',
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
  | '/api/saveStashedPromptTag'
  | '/api/deleteStashedPromptTag'
  | '/api/setStashedPromptTags'
  | '/api/saveSessionAgentNote'
  | '/api/readSessionAgentNote'
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
  /**
   * CDXC:AgentBox 2026-10-01:
   * Reads, and with `runLocation` changes, where a DRAFT will run: this computer or an agentbox
   * box. A box picked here starts with the draft's first message.
   */
  | '/api/draftRunLocation'
  | '/api/draftWorktree'
  /*
   * CDXC:AgentProviders 2026-09-03:
   * Moves a PROMPTED session onto another agent configuration of the same CLI
   * family (another account), so the client's Full Reload resumes the same
   * conversation under that agent's command.
   */
  | '/api/switchSessionAgent'
  | '/api/agentCliMaintenance'
  | '/api/managedTools'
  /**
   * CDXC:RemoteMachines 2026-10-26 SEE-ALSO:
   * A machine's platform, Ghostex CLI and bundled agent skills, with skill install and uninstall
   * (server/src/machine_integrations.rs), for Settings > Integrations with that machine picked.
   */
  | '/api/machineIntegrations'
  /**
   * CDXC:AgentBox 2026-10-01 SEE-ALSO:
   * agentbox readiness, the box list, box web app / screen URLs, stop/destroy, and the setup
   * commands the Cloud Boxes page runs (server/src/agentbox/endpoint.rs).
   */
  | '/api/agentbox'
  /**
   * CDXC:CloudBoxes 2026-11-02 SEE-ALSO:
   * Cloud Boxes: providers, connect, the box registry, create/resume/stop/delete, snapshots, spend
   * and the cap (server/src/cloud_boxes/, contract in gxserver-protocol-cloud-boxes.ts).
   */
  | '/api/cloudBoxes'
  | '/api/agentAccounts'
  | '/api/readAgentResumePlan'
  | '/api/requestSessionRename'
  | '/api/generateSessionTitle'
  | '/api/previewSessionTitle'
  | '/api/cancelFirstPromptAutoTitle'
  | '/api/ingestTerminalTitleEvent'
  | '/api/updateAgentActivity'
  | '/api/readPresentationSnapshot'
  | '/api/readSidebarHud'
  | '/api/mutateSidebarHudSettings'
  | '/api/updateWorkspaceSessionGroups'
  | '/api/readWorkspaceSessionGroups'
  /*
   * CDXC:Navigation 2026-08-19:
   * Titlebar Back/Forward walks a daemon-owned trail of previously active
   * sessions and projects, shared by the gpui desktop titlebar and the web
   * titlebar. See packages/shared/navigation-history for the entry/state contract.
   */
  | '/api/readNavigationHistory'
  | '/api/recordNavigationVisit'
  | '/api/navigateHistory'
  /*
   * CDXC:Notifications 2026-09-11:
   * The notification feed is daemon-owned so the desktop titlebar bell, the web
   * app, and mobile read one list with one read state. See
   * packages/shared/notification-feed for the row/state contract.
   */
  | '/api/readNotificationFeed'
  | '/api/updateNotificationFeed'
  | '/api/createNotification'
  | '/api/readSidebarProjectCollections'
  | '/api/updateSidebarProjectCollections'
  | '/api/assignProjectToSidebarCollection'
  | '/api/readSidebarSpaces'
  | '/api/updateSidebarSpaces'
  | '/api/readCustomSessionTags'
  | '/api/updateCustomSessionTags'
  | '/api/scheduleDelayedSend'
  | '/api/cancelDelayedSend'
  | '/api/postponeDelayedSend'
  | '/api/readAutomationState'
  | '/api/saveAutomation'
  | '/api/deleteAutomation'
  | '/api/runAutomationNow'
  | '/api/setAutomationEnabled'
  | '/api/archiveAutomationRun'
  | '/api/markAutomationRunRead'
  | '/api/listPreviousSessions'
  | '/api/sessionForkBranches'
  | '/api/rewindSessionChat'
  | '/api/selectSessionChatModel'
  | '/api/readSessionTranscriptSizes'
  | '/api/transitionSession'
  | '/api/holdSessionsAwake'
  | '/api/sleepSession'
  | '/api/wakeSession'
  | '/api/reloadSessionAgent'
  | '/api/startSessionProvider'
  | '/api/killSession'
  | '/api/readResourceSessionOwners'
  | '/api/listSessions'
  | '/api/removeSession'
  | '/api/readSessionText'
  | '/api/searchAgentPrompts'
  | '/api/readAgentPromptText'
  | '/api/toggleAgentPromptFavorite'
  | '/api/resolveAgentPromptLaunch'
  | '/api/readSessionChat'
  | '/api/readSessionTerminalTail'
  | '/api/readSessionChatReport'
  | '/api/catchUpSession'
  | '/api/readSessionFilesLinks'
  | '/api/readSessionChatSkills'
  | '/api/readSessionChatFiles'
  | '/api/sendSessionChatMessage'
  | '/api/saveSessionChatImage'
  | '/api/saveSessionChatAttachment'
  | '/api/readSessionChatImage'
  | '/api/answerSessionChatPrompt'
  | '/api/interruptSessionChat'
  | '/api/handoffSessionChatDraft'
  | '/api/replaceSessionChatDraft'
  | '/api/takeBackSessionChatQueuedMessage'
  | '/api/claimSessionChatLaunchDraft'
  | '/api/readSessionChatQueue'
  | '/api/queueSessionChatPrompt'
  | '/api/updateSessionChatQueuedPrompt'
  | '/api/removeSessionChatQueuedPrompt'
  | '/api/reorderSessionChatQueue'
  | '/api/sendSessionChatQueuedPrompt'
  | '/api/setSessionChatDraft'
  | '/api/acknowledgeSessionChatDraftHandoff'
  | '/api/exportSessionTranscript'
  | '/api/countSessionTranscriptTurns'
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
  | '/api/runProjectDocsAction'
  | '/api/searchProjectFiles'
  | '/api/addProjectPath'
  | '/api/syncBotProjects'
  | '/api/listBotFeed'
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
  | '/api/runBeadsAction'
  | '/api/previewRepositoryClone'
  | '/api/startRepositoryClone'
  | '/api/readRepositoryCloneJob'
  | '/api/cancelRepositoryCloneJob'
  | '/api/browseProjectDirectories'
  | '/api/createProjectDirectory'
  | '/api/discoverSourceControl'
  | '/api/lookupRepository'
  | '/api/queryLogs'
  | '/api/draftFeedback'
  | '/api/sendFeedback'
  | '/api/updateAuth'
  | '/api/updateListenerConfig'
  | '/api/appUpdateStatus'
  | '/api/appUpdateCheck'
  | '/api/appUpdateInstall'
  | '/api/appUpdateSetAuto'
  | '/api/tailcatStatus'
  | '/api/installTailcat'
  | '/api/updateTailcatState'
  | '/api/remoteAccessStatus'
  | '/api/enableSshAccess'
  | '/api/remotePairingCode'
  | '/api/pairedDevices'
  | '/api/removePairedDevice'
  | '/api/pairDevice'
  | '/api/pairedDeviceSeen'
  | '/api/easyConnectAddresses'
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

/**
 * The reason a failed gxserver reply gives. `error` is only the code; the text a person can act on ("Install npm
 * on this computer first.") is the top-level `message`.
 */
export function gxserverRpcErrorMessage(body: unknown): string | undefined {
  const message = (body as Partial<GxserverRpcErrorResponse> | undefined)?.message;
  return typeof message === 'string' && message.trim() ? message : undefined;
}

export interface GxserverEndpointDescriptor {
  path: GxserverEndpointPath;
  permission: GxserverApiPermission;
  requiresAuth: boolean;
  requiresProtocolVersion: boolean;
  transport: 'http' | 'webSocket';
}

export interface GxserverRendererCommand {
  action: GxserverRendererCommandAction;
  commandId: string;
  createdAt: string;
  payload: Record<string, unknown>;
  timeoutMs: number;
}
