/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE,
  GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION,
  GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE,
  GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION,
} from './constants';
import type { GxserverPresentationSidebarProjectOverlay } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverFirstPromptTitleGenerationAgent,
  GxserverPresentationDelta,
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRendererCommand,
  GxserverSidebarHudResponse,
  GxserverSidebarProjectCollectionsState,
  GxserverSidebarSpacesState,
  GxserverWorkspaceSessionGroupsState,
} from '@/packages/shared/gxserver-protocol';
import type {
  ExtensionToSidebarMessage,
  SidebarCommandSessionIndicator,
  SidebarRemoteMachineStatusMessage,
  SidebarSessionGroup,
  SidebarToExtensionMessage,
} from '@/packages/shared/session-grid-contract';
import type { SidebarGitAction, SidebarGitChangedFile, SidebarGitState } from '@/packages/shared/sidebar-git';

export type GpuiGxserverBootstrap = {
  authToken?: string;
  baseUrl?: string;
  clientId?: string;
  focusedSessionId?: string;
  initialActiveProjectId?: string;
  protocolVersion?: number;
  visibleSessionIds?: readonly string[];
};

/*
CDXC:RemoteMachines 2026-08-29:
A project's Actions are stored by the daemon that owns it, so a remote
project's Actions come from that machine's own `/api/readSidebarHud`. The Rust
bridge cuts that answer down to the Action button lists before the renderer
sees it, so a remote HUD is deliberately narrower than the local one: no
agents and no project settings rows cross the machine boundary.
*/
export type GpuiRemoteSidebarHud = Pick<
  GxserverSidebarHudResponse,
  'commands' | 'commandsByProject' | 'globalCommands'
>;

export type GpuiCommandPaneSessionSummary = {
  commandId?: string;
  closeAfterDone?: boolean;
  closeAfterDoneDeadlineAt?: string;
  closeAfterDoneRemainingLabel?: string;
  closeAfterDoneRemainingMs?: number;
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  isActive?: boolean;
  /*
  CDXC:SessionSleep 2026-06-27-06:54:
  Rust forwards this true-only bit for native-shaped external `G...` command-panel split pane owners so GPUI Auto Sleep can protect every active command leaf while keeping `isActive` scoped to HUD/responder focus. Rust shell internals may still use numeric ids, but those ids must not cross this TypeScript bridge as command-pane owners.
  */
  isPaneOwner?: true;
  sessionId: string;
  status: SidebarCommandSessionIndicator['status'];
  title?: string;
};

export type GpuiWorkspaceSessionDelayedSendSummary = {
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  sendWhenAllProjectSessionsStopActive?: boolean;
  sendWhenAgentStopsActive?: boolean;
  sessionId: string;
};

export type GpuiFirstPromptTitleRuntimeSettings = {
  firstPromptTitleGenerationAgent: GxserverFirstPromptTitleGenerationAgent;
  firstPromptTitleGenerationCommand?: string;
  firstUserInputDraft?: string;
  firstUserMessage?: string;
};

export type GpuiSidebarRuntimeSettings = {
  debuggingMode?: unknown;
  settings?: unknown;
  showBetaFeatures?: unknown;
};

export type GpuiSidebarRuntimeSettingsSnapshot = {
  debuggingMode: boolean;
  settings?: unknown;
  showBetaFeatures: boolean;
};

/**
 * Everything Rust's `dispatch_gpui_sidebar_host_message` can hand to
 * `onSidebarHostMessage`. Beside the extension-to-sidebar messages the React
 * app consumes, Rust also forwards exactly these sidebar-owned commands, which
 * the runtime answers itself through `handleSidebarMessage` — the React app has
 * no inbound branch for them, so relaying one into the message source would
 * silently drop it. Keep this union in step with the Rust dispatch sites.
 */
export type GpuiSidebarHostMessage =
  | ExtensionToSidebarMessage
  | Extract<
      SidebarToExtensionMessage,
      {
        /*
         * CDXC:SessionNotes 2026-08-24:
         * `setSessionNote` joins this list for the same reason `renameSession`
         * is on it: the note editor is an app-modal window, so its confirm
         * arrives through Rust rather than from the sidebar page itself.
         */
        type:
          | 'cancelDelayedSend'
          | 'confirmAgentHookLaunch'
          | 'removeProject'
          | 'renameSession'
          | 'scheduleDelayedSend'
          | 'setSessionNote'
          | 'toggleCloseAfterDone';
      }
    >;

export type GhostexGpuiSidebarBridge = {
  browserTabs?: readonly GpuiBrowserTabSummary[];
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  /**
   * CDXC:SessionSleep 2026-08-20:
   * The local gxserver sessions the shell is rendering right now, terminal body
   * or chat surface alike. Auto Sleep protects these instead of guessing
   * visibility from the rows this runtime last saw selected.
   */
  displayedWorkspaceSessionIds?: readonly string[];
  onDisplayedWorkspaceSessionIdsChanged?: (sessionIds: readonly string[]) => void;
  workspaceSessionDelayedSends?: readonly GpuiWorkspaceSessionDelayedSendSummary[];
  onBrowserTabsChanged?: (tabs: readonly GpuiBrowserTabSummary[]) => void;
  /**
   * CDXC:Browser 2026-08-18:
   * Rust asks the sidebar to reveal one Browser tab row after the user opened
   * it. Rust owns tab identity (project id + tab id); the session id the
   * sidebar rows are keyed by is derived here, in the same place that builds
   * those rows, so Rust never has to know the sidebar's id format.
   */
  onRevealBrowserTab?: (payload: unknown) => void;
  gxserverBootstrap?: GpuiGxserverBootstrap;
  onCommandPaletteRunSidebarCommand?: (payload: unknown) => void;
  onCommandPaletteSessionFocus?: (payload: unknown) => void;
  onCommandPaneSessionsChanged?: (sessions: readonly GpuiCommandPaneSessionSummary[]) => void;
  onWorkspaceSessionDelayedSendsChanged?: (sessions: readonly GpuiWorkspaceSessionDelayedSendSummary[]) => void;
  onGxserverBootstrapChanged?: (bootstrap: GpuiGxserverBootstrap) => void;
  onExportTranscriptModalCommand?: (payload: unknown) => void;
  onGitCommitModalCommand?: (payload: unknown) => void;
  onMenuBarProjectActivation?: (payload: unknown) => void;
  onMenuBarSessionActivation?: (payload: unknown) => void;
  onNativeAppShotCaptured?: (payload: unknown) => void;
  onNativeAppShotPromptResult?: (payload: unknown) => void;
  onOsIntegrationCommand?: (payload: unknown) => void;
  onProjectBoardConversationRequest?: (payload: unknown) => void;
  onRuntimeSettingsChanged?: (runtimeSettings: GpuiSidebarRuntimeSettingsSnapshot) => void;
  onSidebarHostMessage?: (message: GpuiSidebarHostMessage) => void;
  /**
   * CDXC:SavedPrompts 2026-08-24:
   * A Saved Prompts row asked to be taken back to the session it was stashed
   * from. Rust forwards the row's raw gxserver ids plus the durable provider
   * conversation id; this runtime resolves the best available target.
   */
  onStashedPromptSessionJump?: (payload: unknown) => void;
  onStatusPetActivation?: (payload: unknown) => void;
  onTitlebarGitAction?: (payload: unknown) => void;
  onWorktreeModalCommand?: (payload: unknown) => void;
  /**
   * CDXC:Sidebar 2026-08-02:
   * Close every open sidebar context menu because a native mouse-down landed
   * outside the sidebar's frame. Installed by the sidebar entry point, called
   * by Rust's AppKit pointer observer.
   */
  dismissSidebarContextMenus?: () => void;
  dismissSidebarTooltips?: () => void;
  /**
   * CDXC:Spaces 2026-08-29:
   * A finger scroll gesture began (NSEventPhaseBegan) inside the sidebar's
   * native frame. Installed by the sidebar entry point, called by Rust's
   * AppKit observer; the Space-swipe handler resets its gesture lock on it
   * because DOM wheel events cannot tell a new physical swipe from the
   * previous swipe's momentum tail.
   */
  onNativeScrollGestureBegan?: () => void;
  onWorkspaceFirstPromptTitleGenerationCancel?: (payload: unknown) => void;
  onWorkspaceFolderPicked?: (payload: unknown) => void;
  onWorkspaceSessionAttentionAcknowledge?: (payload: unknown) => void;
  onWorkspaceTabSessionSelected?: (payload: unknown) => void;
  onWorkspaceTerminalBell?: (payload: unknown) => void;
  onWorkspaceTerminalTitleChanged?: (payload: unknown) => void;
  onWorkspaceTerminalEscapePressed?: (payload: unknown) => void;
  onWorkspaceTerminalLifecycleRequest?: (payload: unknown) => void;
  onWorkspaceTerminalRuntimeAction?: (payload: unknown) => void;
  pendingCommandPaletteRunSidebarCommands?: unknown[];
  pendingCommandPaletteSessionFocusRequests?: unknown[];
  pendingExportTranscriptModalCommands?: unknown[];
  pendingGitCommitModalCommands?: unknown[];
  pendingMenuBarProjectActivations?: unknown[];
  pendingMenuBarSessionActivations?: unknown[];
  pendingNativeAppShotPromptResults?: unknown[];
  pendingNativeAppShots?: unknown[];
  pendingOsIntegrationCommands?: unknown[];
  pendingProjectBoardConversationRequests?: unknown[];
  pendingStashedPromptSessionJumps?: unknown[];
  pendingStatusPetActivations?: unknown[];
  pendingTitlebarGitActions?: unknown[];
  pendingWorktreeModalCommands?: unknown[];
  pendingWorkspaceFirstPromptTitleGenerationCancels?: unknown[];
  pendingWorkspaceFolderPicks?: unknown[];
  pendingWorkspaceSessionAttentionAcknowledgements?: unknown[];
  pendingWorkspaceTabSessionSelections?: unknown[];
  pendingWorkspaceTerminalBells?: unknown[];
  pendingWorkspaceTerminalTitleChanges?: unknown[];
  pendingWorkspaceTerminalEscapePresses?: unknown[];
  pendingWorkspaceTerminalLifecycleRequests?: unknown[];
  pendingWorkspaceTerminalRuntimeActions?: unknown[];
  postActiveProjectContext?: (payload: string) => boolean;
  postBrowserTabFocus?: (payload: string) => boolean;
  postCreateProjectAgent?: (payload: string) => boolean;
  postCreateProjectTerminal?: (payload: string) => boolean;
  postGxserverPresentationFocusState?: (payload: string) => boolean;
  postGhostexHotkeyAction?: (payload: string) => boolean;
  postNativeAppShotPromptToSession?: (payload: string) => boolean;
  postNativeProjectPathAction?: (payload: string) => boolean;
  postOpenBrowserUrl?: (payload: string) => boolean;
  postPetOverlayState?: (payload: string) => boolean;
  postProjectBoardConversationResponse?: (payload: string) => boolean;
  postSidebarCommandAction?: (payload: string) => boolean;
  postSidebarCommandRunEnd?: (payload: string) => boolean;
  postSidebarEditableFocus?: (payload: string) => boolean;
  postSessionCompletionSound?: (payload: string) => boolean;
  postGlobalActions?: (payload: string) => boolean;
  postSessionStatusIndicators?: (payload: string) => boolean;
  postTitlebarGitMenuState?: (payload: string) => boolean;
  postWorkspaceTerminalEnter?: (payload: string) => boolean;
  postWorkspaceTerminalFocus?: (payload: string) => boolean;
  postWorkspaceTerminalLifecycleResult?: (payload: string) => boolean;
  postWorkspaceTerminalRenameCommand?: (payload: string) => boolean;
  runtimeSettings?: GpuiSidebarRuntimeSettings;
};

declare global {
  interface Window {
    ghostexGpui?: GhostexGpuiSidebarBridge;
  }
}

export type GpuiSidebarRuntimeSnapshotKind = 'hydrate' | 'patch';

export type GpuiWorkspaceTerminalLifecycleRequest = {
  action: 'close' | 'sleep' | 'wake';
  projectId: string;
  replacementProjectId?: string;
  replacementSessionId?: string;
  requestId: number;
  sessionId: string;
  skipReplacementFallback: boolean;
};

export type GpuiValidatedGxserverBootstrap = {
  authToken: string;
  baseUrl: string;
  clientId: string;
  focusedSessionId?: string;
  initialActiveProjectId?: string;
  visibleSessionIds?: readonly string[];
};

export type GpuiSidebarGroupsPatch = {
  groupOrder: string[];
  groups: SidebarSessionGroup[];
  removedGroupIds: string[];
  removedSessionIds: string[];
};

export type GpuiGxserverRpcSuccess<TResult> = {
  ok: true;
  product: 'gxserver';
  protocolVersion: number;
  result: TResult;
};

export type GpuiProjectWorktreesResultMessage = {
  branches?: unknown;
  error?: string;
  ok: boolean;
  requestId: string;
  type: 'projectWorktreesResult';
  worktrees?: unknown;
};

export type GpuiSidebarRemotePresentationEvent = {
  payload:
    | {
        snapshot: GxserverPresentationSnapshot;
        type: 'presentationSnapshot';
      }
    | {
        delta: GxserverPresentationDelta;
        revision: number;
        type: 'presentationDelta';
      }
    | {
        revision: number;
        sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
        type: 'sidebarProjectCollectionsChanged';
      }
    | {
        revision: number;
        sidebarSpaces: GxserverSidebarSpacesState;
        type: 'sidebarSpacesChanged';
      }
    | {
        groups: GxserverWorkspaceSessionGroupsState;
        revision: number;
        type: 'workspaceGroupsChanged';
      };
  remoteMachineId: string;
  type: 'remoteGxserverPresentation';
};

export type GpuiSidebarRemoteGxserverResponseEvent = {
  error?: string;
  ok: boolean;
  remoteMachineId: string;
  requestId: string;
  result?: unknown;
  type: 'remoteGxserverResponse';
};

export type GpuiSidebarRemoteEvent =
  SidebarRemoteMachineStatusMessage | GpuiSidebarRemoteGxserverResponseEvent | GpuiSidebarRemotePresentationEvent;

export type GpuiSessionStatusIndicatorStatus = 'attention' | 'working' | 'available';

export type GpuiSessionStatusIndicatorCandidate = {
  hasRunningZmxBacking: boolean;
  iconDataUrl?: string;
  lastInteractionAt?: string;
  order: number;
  projectId: string;
  projectTitle: string;
  sessionId: string;
  status: GpuiSessionStatusIndicatorStatus;
  title: string;
};

export type GpuiSessionStatusIndicatorProject = {
  iconDataUrl?: string;
  projectId: string;
  sessions: Array<{
    lastActiveAt?: string;
    sessionId: string;
    sidebarOrder: number;
    status: GpuiSessionStatusIndicatorStatus;
    title: string;
  }>;
  title: string;
};

export type GpuiSessionStatusIndicatorsPayload = {
  attentionCount: number;
  availableCount: number;
  hideMenuBarIndicators: boolean;
  projects: GpuiSessionStatusIndicatorProject[];
  type: typeof GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE;
  version: typeof GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION;
  workingCount: number;
};

export type GpuiPetOverlayStatePayload = {
  activities: Array<{
    id: string;
    projectId: string;
    state: GpuiSessionStatusIndicatorStatus;
    title: string;
  }>;
  enabled: boolean;
  selectedPetId: string;
  statusItems: Array<{
    count: number;
    status: GpuiSessionStatusIndicatorStatus;
  }>;
  type: typeof GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE;
  version: typeof GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION;
};

export type GpuiStatusPetActivationPayload = {
  sessionId: string;
};

export type GpuiMenuBarProjectActivationPayload = {
  projectId: string;
};

export type GpuiMenuBarSessionActivationPayload = {
  projectId: string;
  sessionId: string;
};

export type GpuiWorkspaceTabSessionSelectionPayload = {
  localRuntimeMissing?: true;
  localWasSleeping?: true;
  projectId: string;
  sessionId: string;
  visibleSessionIds?: readonly string[];
};

export type GpuiActiveWorkspaceTabSessionPayload = {
  activity: 'idle' | 'working' | 'attention';
  agentIcon?: string;
  agentName?: string;
  agentSessionId?: string;
  hasSessionNote?: boolean;
  stashedPromptCount?: number;
  /*
  CDXC:AgentProviders 2026-09-03:
  The daemon-resolved accounts this session can be resumed under, so the native
  terminal action bar can render the "Switch Account" submenu without any Rust
  knowledge of project agent configuration. PRESENT-ONLY: absent means none.
  */
  switchableAgents?: readonly { agentId: string; icon: string; name: string }[];
  /*
  CDXC:Drafts 2026-08-28:
  The session is a draft (no first prompt yet), copied from the projected
  sidebar row. Rust needs it because a draft is chat-eligible WITHOUT an
  `agentSessionId`: its CLI publishes one only once it has booted, and an agent
  switch takes it away again for the length of the swap. PRESENT-ONLY, like
  every other draft marker on the way here — absence means "not a draft".
  */
  isDraft?: boolean;
  isGeneratingFirstPromptTitle: boolean;
  isSleeping: boolean;
  kind: GxserverPresentationSession['kind'];
  lifecycleState?: string;
  projectId: string;
  sessionId: string;
  title: string;
};

export type GpuiBrowserTabSummary = {
  faviconUrl?: string;
  isActive: boolean;
  isSleeping: boolean;
  isVisible: boolean;
  projectId: string;
  tabId: string;
  title: string;
  url: string;
};

export type GpuiRendererCommandResolvedSession = {
  projectId: string;
  sessionId: string;
  sidebarSessionId: string;
};

/*
CDXC:Git 2026-07-29:
The two GitHub-CLI derived fields of `SidebarGitState`, memoized as one unit so
they are always published together (a `pr` from one probe can never pair with a
`hasGitHubCli` from another).
*/
export type GpuiSidebarGitHubState = {
  hasGitHubCli: boolean;
  pr: SidebarGitState['pr'];
};

export type GpuiSidebarNativeProjectPathAction =
  | 'copyRecentProjectPath'
  | 'openRecentProjectInFinder'
  | 'copyWorkspaceProjectPath'
  | 'openWorkspaceProjectInFinder'
  | 'openWorkspaceProjectInIde'
  | 'openActiveWorkspaceProjectInFinder'
  | 'openActiveWorkspaceProjectInVscode'
  | 'openActiveWorkspaceProjectInZed'
  | 'openExistingPullRequestInBrowser'
  | 'openSidebarGitChangedFileInIde'
  | 'copyRemoteProjectPath'
  | 'openRemoteProjectTerminal'
  | 'openRemoteWorkspaceProjectInIde'
  | 'openRemoteWorkspaceProjectInVscode'
  | 'openRemoteWorkspaceProjectInZed'
  | 'openRemoteExistingPullRequestInBrowser'
  | 'openRemoteSidebarGitChangedFileInIde'
  | 'openRemoteProjectPortsBrowser'
  | 'openRemoteSessionTerminal'
  | 'copyRemoteAttachCommand'
  | 'copyRemoteResumeCommand';

export type GpuiTrustedExistingWorktreeList = {
  parentProjectId: string;
  paths: Set<string>;
  remoteMachineId?: string;
  sourceProjectId: string;
  worktreeKeys?: Set<string>;
};

export type GpuiPendingGitCommitRequest = {
  action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>;
  files: SidebarGitChangedFile[];
  hasCommit: boolean;
  projectId: string;
  remoteReference?: GpuiRemoteProjectReference;
  remoteTitle?: string;
  subject: string;
};

export type GpuiPendingNativeAppShotPromptInsertion = {
  resolve: (ok: boolean) => void;
  sessionId: string;
  timeoutId: number;
};

export type GpuiTrustedGitReviewFileSelection = {
  explicit: boolean;
  filePaths: string[];
};

export type GpuiPendingRemoteGxserverRequest = {
  reject: (error: Error) => void;
  resolve: (result: unknown) => void;
  timeoutId: number;
};

export type GpuiGxserverCreatedSessionResult = {
  session?: {
    projectId?: string;
    sessionId?: string;
  };
};

export type GpuiNativeAppShotCapture = {
  appName: string;
  bundleIdentifier?: string;
  imagePath: string;
  trigger?: string;
  windowHeight?: number;
  windowTitle?: string;
  windowWidth?: number;
};

export type GpuiWorktreeMetadata = {
  branch?: string;
  name?: string;
  parentProjectId: string;
  parentProjectName?: string;
};

export type GpuiProjectWorktreeParentCandidate = {
  name?: string;
  path?: string;
  projectId: string;
  worktree?: Record<string, unknown>;
};

export type GpuiGitPreferences = {
  confirmCommit: boolean;
  generateCommitBody: boolean;
  primaryAction: SidebarGitAction;
};

export type GpuiRemoteProjectReference = {
  machineId: string;
  projectId: string;
};

/*
CDXC:TranscriptExport 2026-08-24:
Which session the open Export Transcript dialog is about, parked in the
runtime while the user chooses the include-toggles. The dialog is a separate
child window with no gxserver client, so its export request comes back with
only the toggles and the runtime resolves everything else from this context.
*/
export type GpuiExportTranscriptRequestContext = {
  /** The session's agent when known upfront (local sessions). */
  agentId?: string;
  /** Absent for the local daemon; set for a remote machine's own daemon. */
  machineId?: string;
  projectId: string;
  /** Identifies this exact dialog open across close/reopen races. */
  requestId: string;
  sessionId: string;
};

export type GpuiExportedTranscriptResult = {
  /** The exported session's agent, so the dialog can preselect the same one. */
  agentId?: string;
  /** Absolute path of the markdown file, on `machineId`'s disk. */
  path: string;
  projectId: string;
  /** The dialog request that produced this export. */
  requestId: string;
  /** Absent for the local daemon; set for a remote machine's own daemon. */
  machineId?: string;
};

export type GpuiProjectDiffStatsRefreshTarget =
  | { key: string; kind: 'local'; project: GxserverProjectDomainState }
  | { key: string; kind: 'remote'; reference: GpuiRemoteProjectReference };

export type GpuiRemoteProjectScope = GpuiRemoteProjectReference & {
  machineName?: string;
  project: GxserverPresentationProject;
};

export type GpuiRemoteCreatePullRequestResult = {
  created?: boolean;
  ok?: boolean;
  pr?: {
    number?: number;
    state?: string;
  };
  reason?: string;
};

export type GpuiRendererCommandHandler = (
  command: GxserverRendererCommand
) => Promise<Record<string, unknown> | void> | Record<string, unknown> | void;

export type GpuiPresentationSubscription = {
  close: () => void;
};

export type GpuiSidebarCommandSessionIndicatorScope = {
  activeProjectId?: string;
  presentation?: GxserverPresentationSnapshot;
};

export type GpuiWorktreeDeleteBranchMetadata = {
  branch: string | null;
  canDeleteLocalBranch: boolean;
  localBranchName?: string;
  remoteBranchDisabledReason?: string;
  remoteBranchExists: boolean;
  remoteBranchName?: string;
  remoteName: string;
};

export type GpuiWorktreeModalCommand =
  | Extract<SidebarToExtensionMessage, { type: 'requestProjectWorktrees' }>
  | Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>
  | Extract<SidebarToExtensionMessage, { type: 'confirmDeleteWorktree' }>
  | Extract<SidebarToExtensionMessage, { type: 'confirmRenameWorktree' }>
  | Extract<SidebarToExtensionMessage, { type: 'commitWorktreeBeforeDelete' }>;

export type GpuiGitCommitModalCommand =
  | Extract<SidebarToExtensionMessage, { type: 'confirmSidebarGitCommit' }>
  | Extract<SidebarToExtensionMessage, { type: 'confirmSidebarGitDirectMerge' }>
  | Extract<SidebarToExtensionMessage, { type: 'runSidebarGitMultipleCommits' }>
  | Extract<SidebarToExtensionMessage, { type: 'openSidebarGitChangedFileDiff' }>
  | Extract<SidebarToExtensionMessage, { type: 'cancelSidebarGitCommit' }>;

export type GpuiCreatedProjectAgentSessionRecord = {
  agentSessionId?: string;
  agentSessionPath?: string;
  projectId: string;
  sessionId: string;
  zmxName?: string;
};

export type GpuiProjectBoardConversationRequest = {
  action:
    | 'appendDebugLog'
    | 'associateFocusedSession'
    | 'getState'
    | 'jumpToConversation'
    | 'showToast'
    | 'startWork'
    | 'unlinkConversation';
  agentId?: string;
  beadDisplayId?: string;
  beadId?: string;
  projectId?: string;
  projectPath?: string;
  prompt?: string;
  requestId: string;
  sessionId?: string;
  startLocation?: string;
  toastDescription?: string;
  toastLevel?: string;
  toastTitle?: string;
};

export type GpuiWorkspaceTerminalBellPayload = {
  projectId: string;
  sessionId: string;
};

export type GpuiWorkspaceTerminalTitleChangedPayload = {
  projectId: string;
  rawTitle: string;
  sessionId: string;
};

export type GpuiWorkspaceTerminalEscapePressedPayload = {
  projectId: string;
  sessionId: string;
};

export type GpuiWorkspaceFirstPromptTitleGenerationCancelPayload = {
  projectId: string;
  sessionId: string;
};

export type GpuiWorkspaceSessionAttentionAcknowledgePayload = {
  projectId: string;
  sessionId: string;
};

export type GpuiSessionAttentionAcknowledgeReason = 'native-focus' | 'sidebar-focus' | 'terminal-escape';

export type GpuiSessionAttentionTarget =
  | {
      kind: 'local';
      projectId: string;
      sessionId: string;
    }
  | {
      kind: 'remote';
      machineId: string;
      projectId: string;
      sessionId: string;
    };

export type GpuiWorkspaceTerminalRuntimeActionPayload =
  | {
      action: 'exportTranscript' | 'forkSession' | 'fullReloadSession' | 'openSessionNote';
      projectId: string;
      sessionId: string;
    }
  | {
      /**
       * CDXC:AgentProviders 2026-09-03:
       * Rust-origin Switch Account (terminal action bar, chat composer). The
       * agent id is one of the rows the runtime itself forwarded to Rust on the
       * tab session's `switchableAgents`.
       */
      action: 'switchSessionAgent';
      agentId: string;
      projectId: string;
      sessionId: string;
    }
  | { action: 'sleepAllDaemonSessions' }
  | { action: 'sleepInactiveSessions' };

export type GpuiPresentationProjectProjectionMetadata = {
  chatProjectIds: ReadonlySet<string>;
  hiddenProjectIds: ReadonlySet<string>;
  projectOverlays: readonly GxserverPresentationSidebarProjectOverlay[];
};

export type GpuiCloseAfterDoneTimer = {
  deadlineAtMs?: number;
  doneSinceAtMs?: number;
  timeoutId?: number;
};
