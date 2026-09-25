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
  GxserverCustomSessionTagsState,
  GxserverPresentationDelta,
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverSidebarHudResponse,
  GxserverSidebarProjectCollectionsState,
  GxserverSidebarSpacesState,
  GxserverWorkspaceSessionGroupsState,
} from '@/packages/shared/gxserver-protocol';
import type {
  ExtensionToSidebarMessage,
  SidebarRemoteMachineStatusMessage,
  SidebarToExtensionMessage,
} from '@/packages/shared/session-grid-contract';
import type { ModelPickerProvider } from '@/packages/shared/session-chat-presentation/model-picker';
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

export type GhostexGpuiSidebarBridge = {
  gxserverBootstrap?: GpuiGxserverBootstrap;
  onGxserverBootstrapChanged?: (bootstrap: GpuiGxserverBootstrap) => void;
  /**
   * CDXC:Sidebar 2026-09-21 WHY:
   * The Rust store's direct route for a sidebar command this runtime still owns (a remote
   * machine's project collections and Spaces). It replaces the hop through the sidebar
   * page's `onNativeSidebarCommand`, which is being deleted with that page; anything Rust sends
   * before this is installed is parked on `pendingSidebarCommands` and drained here.
   */
  onSidebarCommand?: (payload: unknown) => void;
  /**
   * CDXC:Sidebar 2026-08-02:
   * Close every open sidebar context menu because a native mouse-down landed
   * outside the sidebar's frame. Installed by the sidebar entry point, called
   * by Rust's AppKit pointer observer.
   */
  dismissSidebarContextMenus?: () => void;
  dismissSidebarTooltips?: () => void;
  pendingSidebarCommands?: unknown[];
  runtimeSettings?: GpuiSidebarRuntimeSettings;
};

declare global {
  interface Window {
    ghostexGpui?: GhostexGpuiSidebarBridge;
  }
}

export type GpuiSidebarRuntimeSnapshotKind = 'hydrate' | 'patch';

export type GpuiValidatedGxserverBootstrap = {
  authToken: string;
  baseUrl: string;
  clientId: string;
  focusedSessionId?: string;
  initialActiveProjectId?: string;
  visibleSessionIds?: readonly string[];
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
        customSessionTags: GxserverCustomSessionTagsState;
        revision: number;
        type: 'customSessionTagsChanged';
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
  /** The Rust store's focus stamp of this selection; echoed in every focus state posted afterwards. */
  focusStamp?: number;
  localRuntimeMissing?: true;
  localWasSleeping?: true;
  projectId: string;
  /** Sessions selected in other projects since the last message; remembered, never focused. */
  rememberedSessions?: readonly { projectId: string; sessionId: string }[];
  sessionId: string;
  visibleSessionIds?: readonly string[];
};

export type GpuiActiveWorkspaceTabSessionPayload = {
  activity: 'idle' | 'working' | 'attention';
  workingDirectory?: string;
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
  | 'revealSidebarGitChangedFile'
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
    /** The created row's agent, so a restore can resolve its Default Agent View before anything is woken. */
    agentId?: string;
    projectId?: string;
    sessionId?: string;
  };
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
  sessionTitle: string;
  /** The session's agent when known upfront (local sessions). */
  agentId?: string;
  /** Absent for the local daemon; set for a remote machine's own daemon. */
  machineId?: string;
  projectId: string;
  /** Identifies this exact dialog open across close/reopen races. */
  requestId: string;
  sessionId: string;
  /** Set when the chat model picker opened the dialog by picking another agent's model. */
  handoffTarget?: GpuiHandoffModelTarget;
};

/** The model the chat model picker chose for the follow-up session, and the agent family that offers it. */
export type GpuiHandoffModelTarget = {
  provider: ModelPickerProvider;
  model: string;
  effort: string;
};

export type GpuiExportedTranscriptResult = {
  sessionTitle: string;
  /** The exported session's agent, so the dialog can preselect the same one. */
  agentId?: string;
  /** Absolute path of the markdown file, on `machineId`'s disk. */
  path: string;
  projectId: string;
  /** The dialog request that produced this export. */
  requestId: string;
  /** Absent for the local daemon; set for a remote machine's own daemon. */
  machineId?: string;
  handoffTarget?: GpuiHandoffModelTarget;
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

export type GpuiPresentationSubscription = {
  close: () => void;
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
  | Extract<SidebarToExtensionMessage, { type: 'openSidebarGitChangedFile' }>
  | Extract<SidebarToExtensionMessage, { type: 'cancelSidebarGitCommit' }>;

export type GpuiCreatedProjectAgentSessionRecord = {
  agentSessionId?: string;
  agentSessionPath?: string;
  projectId: string;
  sessionId: string;
  zmxName?: string;
};

export type GpuiWorkspaceTerminalEscapePressedPayload = {
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

/**
 * CDXC:Workarea 2026-09-04 DECISION:
 * User: Advanced > Split Right opens a sidebar session in a pane to the right
 * of the focused agents pane. The workspace focus bridge carries it as an
 * optional `placement`; absent means the ordinary tab placement.
 * SEE-ALSO: `gpui_sidebar_workspace_terminal_focus_from_value` in
 * apps/desktop/src/app/helpers/sidebar/workspace_terminal_actions.rs.
 */
export type GpuiWorkspaceTerminalFocusPlacement = 'splitRight';

export type GpuiWorkspaceTerminalRuntimeActionPayload =
  | {
      action: 'exportTranscript';
      projectId: string;
      sessionId: string;
    }
  | {
      /** The chat model picker chose a model that belongs to another agent than the session's. */
      action: 'handoffToModel';
      projectId: string;
      sessionId: string;
      target: GpuiHandoffModelTarget;
    };

export type GpuiPresentationProjectProjectionMetadata = {
  chatProjectIds: ReadonlySet<string>;
  hiddenProjectIds: ReadonlySet<string>;
  projectOverlays: readonly GxserverPresentationSidebarProjectOverlay[];
};
