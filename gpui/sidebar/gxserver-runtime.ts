import {
  GXSERVER_PROTOCOL_VERSION,
  type GxserverAgentResumePlan,
  type GxserverAppUserData,
  type GxserverCheckoutProjectNewBranchResult,
  type GxserverCreatePullRequestResult,
  type GxserverCreateWorktreeSessionResult,
  type GxserverDeleteWorktreeProjectResult,
  type GxserverEndpointPath,
  type GxserverFirstPromptTitleGenerationAgent,
  type GxserverForkSessionResult,
  type GxserverGenerateCommitMessageResult,
  type GxserverGitAction,
  type GxserverMergeWorktreeIntoMainResult,
  type GxserverPresentationDelta,
  type GxserverPresentationProject,
  type GxserverPresentationSearchResponse,
  type GxserverPresentationSearchResult,
  type GxserverPresentationSession,
  type GxserverPresentationSnapshot,
  type GxserverProjectDomainState,
  type GxserverProjectId,
  type GxserverRpcErrorCode,
  type GxserverProjectWorktreeListResult,
  type GxserverRecentProjectDomainState,
  type GxserverRemoveSessionWorktreeResult,
  type GxserverRendererCommand,
  type GxserverSessionId,
  type GxserverSessionRenameRequestResult,
  type GxserverSessionTransitionResult,
  type GxserverSidebarHudResponse,
  type GxserverSidebarHudSettingsMutationParams,
  type GxserverSidebarHudSettingsMutationResult,
  type GxserverSidebarProjectCollectionsState,
  type GxserverTypedOperationResult,
} from "../../shared/gxserver-protocol";
import {
  reduceGxserverPresentationDelta,
  reorderPresentationProjectSessions,
} from "../../shared/gxserver-presentation-cache";
import {
  isSessionChatEventType,
  type GxserverSessionChatEvent,
} from "../../shared/session-chat";
import { createDisplaySessionLayout } from "../../shared/active-sessions-sort";
import { T3CODE_ENABLED } from "../../shared/feature-flags";
import {
  createEmptyGpuiWorkspaceSessionGroupsState,
  createGpuiWorkspaceSessionSubgroup,
  createGpuiWorkspaceSessionSubgroupId,
  findGpuiWorkspaceSessionSubgroupForSession,
  getGpuiWorkspaceSessionSubgroups,
  isEmptyGpuiWorkspaceSessionGroupsState,
  moveGpuiWorkspaceSessionToSubgroup,
  parseGpuiWorkspaceSessionGroupsState,
  parseGpuiWorkspaceSessionSubgroupId,
  pruneGpuiWorkspaceSessionSubgroups,
  readStoredGpuiWorkspaceSessionGroupsState,
  removeGpuiWorkspaceSessionSubgroup,
  renameGpuiWorkspaceSessionSubgroup,
  syncGpuiWorkspaceProjectOrder,
  syncGpuiWorkspaceSessionOrderInSubgroup,
  syncGpuiWorkspaceSessionSubgroupOrder,
  writeStoredGpuiWorkspaceSessionGroupsState,
  type GpuiWorkspaceSessionGroupsState,
} from "./workspace-session-groups";
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarGroups,
  createGxserverPresentationSidebarSessionKey,
  createGxserverPresentationSessionsByProjectFromGroups,
  gxserverPresentationSidebarAutoSettleAfterDays,
  gxserverPresentationSidebarLifecycleCapabilities,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
  visibleCountForGxserverPresentationSidebarSessions,
  type GxserverPresentationCloseAfterDoneProjection,
  type GxserverPresentationDelayedSendProjection,
  type GxserverPresentationSidebarProjectOverlay,
} from "../../shared/gxserver-presentation-sidebar-projection";
import { orderProjectsWithWorktrees } from "../../shared/project-worktree-order";
import {
  createAgentSessionDefaultTitle,
  DEFAULT_TERMINAL_SESSION_TITLE,
  GRID_COLUMN_COUNT,
  resolveSidebarTheme,
  type ExtensionToSidebarMessage,
  type SidebarCommandSessionIndicator,
  type SidebarGroupsChangedMessage,
  type SidebarHudChangedMessage,
  type SidebarHudState,
  type SidebarHydrateMessage,
  type SidebarOrderSyncResultMessage,
  type SidebarPreviousSessionsResultMessage,
  type SidebarPreviousSessionItem,
  type SidebarPromptGitCommitMessage,
  type SidebarProjectSettingsItem,
  type SidebarProjectWorktreeMetadata,
  type SidebarRemoteMachineStatusMessage,
  type SidebarRecentProject,
  type SidebarSessionGroup,
  type SidebarSessionItem,
  type SidebarTheme,
  type SidebarToExtensionMessage,
} from "../../shared/session-grid-contract";
import {
  createSidebarAgentButtons,
  DEFAULT_SIDEBAR_AGENTS,
  getSidebarAgentIconById,
  isDefaultSidebarAgentId,
  type SidebarAgentButton,
} from "../../shared/sidebar-agents";
import {
  createSidebarCommandButtons,
  DEFAULT_BROWSER_LAUNCH_URL,
  isSidebarCommandRunMode,
  isSidebarCommandConfigured,
  isSidebarCommandScope,
  normalizeSidebarCommandLinks,
  type SidebarCommandButton,
  type SidebarCommandScope,
} from "../../shared/sidebar-commands";
import { getCompletionSoundLabel } from "../../shared/completion-sound";
import type { CompletionSoundSetting } from "../../shared/completion-sound";
import { createAppToastRequest, type AppToastLevel } from "../../shared/app-toast-contract";
import {
  beadConversationLinkMatchKey,
  canonicalizeBeadConversationLinksForBoard,
  createBeadConversationLinkId,
  normalizeBeadConversationLinks,
  resolveBeadConversationLinkBoardSessionId,
  selectBeadConversationLinkStoreProjects,
  type BeadConversationLink,
  type ProjectBoardAgentOption,
  type ProjectBoardConversationLinkView,
  type ProjectBoardConversationState,
  type ProjectBoardSessionOption,
} from "../../shared/bead-conversation-links";
import { normalizeghostexSettings, type ghostexSettings } from "../../shared/ghostex-settings";
import {
  buildSidebarGitMenuItems,
  createDefaultSidebarGitState,
  getSidebarGitDisabledReason,
  hasSidebarGitRemoteCommitDelta,
  normalizeSidebarGitAction,
  resolveSidebarGitPrimaryActionState,
  type SidebarGitAction,
  type SidebarGitChangedFile,
  type SidebarGitFileDiffDraft,
  type SidebarGitState,
} from "../../shared/sidebar-git";
import {
  SIDEBAR_GIT_HUB_MEMO_TTL_MS,
  SIDEBAR_GIT_STATE_MEMO_TTL_MS,
  SidebarGitTtlMemo,
} from "../../shared/sidebar-git-state-memo";
import {
  createDefaultSidebarProjectDiffStats,
  parseGitNumstatDiffStats,
  parseGitZeroDelimitedPaths,
  resolveSidebarProjectDiffStats,
  type SidebarProjectDiffStats,
} from "../../shared/project-diff-stats";
import {
  normalizeWorkspaceProjectIcon,
  normalizeWorkspaceProjectIconDataUrl,
  normalizeWorkspaceThemeColor,
  type WorkspaceProjectIcon,
} from "../../shared/workspace-project-appearance";
import type { SidebarSessionTag } from "../../shared/session-tags";
import { openAppModal, postAppModalHostMessage } from "../../sidebar/app-modal-host-bridge";
import type { WebviewApi } from "../../sidebar/webview-api";
import { createGpuiSidebarActiveProjectContextPayloadFromGroups } from "./active-project-context";
import { runGpuiSidebarBulkSleepPaced } from "./bulk-sleep-pacing";
import {
  createRemoteProjectId,
  createRemoteTerminalSessionId,
  parseRemoteProjectId,
  parseRemoteTerminalSessionId,
  resolveActiveTerminalSelection,
} from "../../shared/remote-terminal-selection";

export type GpuiGxserverBootstrap = {
  authToken?: string;
  baseUrl?: string;
  clientId?: string;
  focusedSessionId?: string;
  initialActiveProjectId?: string;
  protocolVersion?: number;
  visibleSessionIds?: readonly string[];
};

class GpuiGxserverRpcError extends Error {
  readonly code?: GxserverRpcErrorCode;

  constructor(message: string, code?: GxserverRpcErrorCode) {
    super(message);
    this.name = "GpuiGxserverRpcError";
    this.code = code;
  }
}

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
  CDXC:GPUISidebarAutoSleep 2026-06-27-06:54:
  Rust forwards this true-only bit for native-shaped external `G...` command-panel split pane owners so GPUI Auto Sleep can protect every active command leaf while keeping `isActive` scoped to HUD/responder focus. Rust shell internals may still use numeric ids, but those ids must not cross this TypeScript bridge as command-pane owners.
  */
  isPaneOwner?: true;
  sessionId: string;
  status: SidebarCommandSessionIndicator["status"];
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

type GpuiFirstPromptTitleRuntimeSettings = {
  firstPromptTitleGenerationAgent: GxserverFirstPromptTitleGenerationAgent;
  firstPromptTitleGenerationCommand?: string;
  firstUserMessage?: string;
};

type GpuiSidebarRuntimeSettings = {
  debuggingMode?: unknown;
  settings?: unknown;
  showBetaFeatures?: unknown;
};

type GpuiSidebarRuntimeSettingsSnapshot = {
  debuggingMode: boolean;
  settings?: unknown;
  showBetaFeatures: boolean;
};

export type GhostexGpuiSidebarBridge = {
  browserTabs?: readonly GpuiBrowserTabSummary[];
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  workspaceSessionDelayedSends?: readonly GpuiWorkspaceSessionDelayedSendSummary[];
  onBrowserTabsChanged?: (tabs: readonly GpuiBrowserTabSummary[]) => void;
  gxserverBootstrap?: GpuiGxserverBootstrap;
  onCommandPaletteRunSidebarCommand?: (payload: unknown) => void;
  onCommandPaletteSessionFocus?: (payload: unknown) => void;
  onT3SessionBrowserAccessResult?: (payload: unknown) => void;
  onCommandPaneSessionsChanged?: (sessions: readonly GpuiCommandPaneSessionSummary[]) => void;
  onWorkspaceSessionDelayedSendsChanged?: (
    sessions: readonly GpuiWorkspaceSessionDelayedSendSummary[],
  ) => void;
  onGxserverBootstrapChanged?: (bootstrap: GpuiGxserverBootstrap) => void;
  onGitCommitModalCommand?: (payload: unknown) => void;
  onMenuBarProjectActivation?: (payload: unknown) => void;
  onMenuBarSessionActivation?: (payload: unknown) => void;
  onNativeAppShotCaptured?: (payload: unknown) => void;
  onNativeAppShotPromptResult?: (payload: unknown) => void;
  onOsIntegrationCommand?: (payload: unknown) => void;
  onProjectBoardConversationRequest?: (payload: unknown) => void;
  onRuntimeSettingsChanged?: (runtimeSettings: GpuiSidebarRuntimeSettingsSnapshot) => void;
  onSidebarHostMessage?: (message: ExtensionToSidebarMessage | SidebarToExtensionMessage) => void;
  onStatusPetActivation?: (payload: unknown) => void;
  onTitlebarGitAction?: (payload: unknown) => void;
  onWorktreeModalCommand?: (payload: unknown) => void;
  /**
   * CDXC:GPUISidebarPointerTracking 2026-08-02:
   * Close every open sidebar context menu because a native mouse-down landed
   * outside the sidebar's frame. Installed by the sidebar entry point, called
   * by Rust's AppKit pointer observer.
   */
  dismissSidebarContextMenus?: () => void;
  onWorkspaceFirstPromptTitleGenerationCancel?: (payload: unknown) => void;
  onWorkspaceFolderPicked?: (payload: unknown) => void;
  onWorkspaceSessionAttentionAcknowledge?: (payload: unknown) => void;
  onWorkspaceTabSessionSelected?: (payload: unknown) => void;
  onWorkspaceTerminalBell?: (payload: unknown) => void;
  onWorkspaceTerminalEscapePressed?: (payload: unknown) => void;
  onWorkspaceTerminalLifecycleRequest?: (payload: unknown) => void;
  onWorkspaceTerminalRuntimeAction?: (payload: unknown) => void;
  pendingCommandPaletteRunSidebarCommands?: unknown[];
  pendingCommandPaletteSessionFocusRequests?: unknown[];
  pendingGitCommitModalCommands?: unknown[];
  pendingT3SessionBrowserAccessResults?: unknown[];
  pendingMenuBarProjectActivations?: unknown[];
  pendingMenuBarSessionActivations?: unknown[];
  pendingNativeAppShotPromptResults?: unknown[];
  pendingNativeAppShots?: unknown[];
  pendingOsIntegrationCommands?: unknown[];
  pendingProjectBoardConversationRequests?: unknown[];
  pendingStatusPetActivations?: unknown[];
  pendingTitlebarGitActions?: unknown[];
  pendingWorktreeModalCommands?: unknown[];
  pendingWorkspaceFirstPromptTitleGenerationCancels?: unknown[];
  pendingWorkspaceFolderPicks?: unknown[];
  pendingWorkspaceSessionAttentionAcknowledgements?: unknown[];
  pendingWorkspaceTabSessionSelections?: unknown[];
  pendingWorkspaceTerminalBells?: unknown[];
  pendingWorkspaceTerminalEscapePresses?: unknown[];
  pendingWorkspaceTerminalLifecycleRequests?: unknown[];
  pendingWorkspaceTerminalRuntimeActions?: unknown[];
  postActiveProjectContext?: (payload: string) => boolean;
  postBrowserTabFocus?: (payload: string) => boolean;
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
  postT3SessionBrowserAccessRequest?: (payload: string) => boolean;
  postT3SessionCreate?: (payload: string) => boolean;
  postT3SessionFocus?: (payload: string) => boolean;
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

type GpuiSidebarRuntimeSnapshotKind = "hydrate" | "patch";

type GpuiWorkspaceTerminalLifecycleRequest = {
  action: "close" | "sleep" | "wake";
  projectId: string;
  replacementProjectId?: string;
  replacementSessionId?: string;
  requestId: number;
  sessionId: string;
  skipReplacementFallback: boolean;
};

type GpuiValidatedGxserverBootstrap = {
  authToken: string;
  baseUrl: string;
  clientId: string;
  focusedSessionId?: string;
  initialActiveProjectId?: string;
  visibleSessionIds?: readonly string[];
};

type GpuiSidebarGroupsPatch = {
  groupOrder: string[];
  groups: SidebarSessionGroup[];
  removedGroupIds: string[];
  removedSessionIds: string[];
};

type GpuiGxserverRpcSuccess<TResult> = {
  ok: true;
  product: "gxserver";
  protocolVersion: number;
  result: TResult;
};

type GpuiProjectWorktreesResultMessage = {
  branches?: unknown;
  error?: string;
  ok: boolean;
  requestId: string;
  type: "projectWorktreesResult";
  worktrees?: unknown;
};

type GpuiSidebarRemotePresentationEvent = {
  payload:
    | {
        snapshot: GxserverPresentationSnapshot;
        type: "presentationSnapshot";
      }
    | {
        delta: GxserverPresentationDelta;
        revision: number;
        type: "presentationDelta";
      };
  remoteMachineId: string;
  type: "remoteGxserverPresentation";
};

type GpuiSidebarRemoteGxserverResponseEvent = {
  error?: string;
  ok: boolean;
  remoteMachineId: string;
  requestId: string;
  result?: unknown;
  type: "remoteGxserverResponse";
};

type GpuiSidebarRemoteEvent =
  | SidebarRemoteMachineStatusMessage
  | GpuiSidebarRemoteGxserverResponseEvent
  | GpuiSidebarRemotePresentationEvent;

type GpuiSessionStatusIndicatorStatus = "attention" | "working" | "available";

type GpuiSessionStatusIndicatorCandidate = {
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

type GpuiSessionStatusIndicatorProject = {
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

type GpuiSessionStatusIndicatorsPayload = {
  attentionCount: number;
  availableCount: number;
  hideMenuBarIndicators: boolean;
  projects: GpuiSessionStatusIndicatorProject[];
  type: typeof GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE;
  version: typeof GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION;
  workingCount: number;
};

type GpuiPetOverlayStatePayload = {
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

type GpuiStatusPetActivationPayload = {
  sessionId: string;
};

type GpuiMenuBarProjectActivationPayload = {
  projectId: string;
};

type GpuiMenuBarSessionActivationPayload = {
  projectId: string;
  sessionId: string;
};

type GpuiWorkspaceTabSessionSelectionPayload = {
  localRuntimeMissing?: true;
  localWasSleeping?: true;
  projectId: string;
  sessionId: string;
  visibleSessionIds?: readonly string[];
};

type GpuiActiveWorkspaceTabSessionPayload = {
  activity: "idle" | "working" | "attention";
  agentIcon?: string;
  agentSessionId?: string;
  isGeneratingFirstPromptTitle: boolean;
  isSleeping: boolean;
  kind: GxserverPresentationSession["kind"];
  lifecycleState?: string;
  projectId: string;
  sessionId: string;
  title: string;
};

type GpuiBrowserTabSummary = {
  isActive: boolean;
  isSleeping: boolean;
  isVisible: boolean;
  projectId: string;
  tabId: string;
  title: string;
  url: string;
};

type GpuiRendererCommandResolvedSession = {
  projectId: string;
  sessionId: string;
  sidebarSessionId: string;
};

/*
CDXC:SidebarGitMemo 2026-07-29:
The two GitHub-CLI derived fields of `SidebarGitState`, memoized as one unit so
they are always published together (a `pr` from one probe can never pair with a
`hasGitHubCli` from another).
*/
type GpuiSidebarGitHubState = {
  hasGitHubCli: boolean;
  pr: SidebarGitState["pr"];
};

const GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS = 20;
const GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS = 250;
const GPUI_AUTO_SLEEP_MONITOR_INTERVAL_MS = 60 * 1000;
const GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS = 15 * 1000;
/*
CDXC:SidebarGitMemo 2026-07-29:
GitHub CLI probes (`gh --version`, `gh pr view`) are the only networked calls in
the sidebar Git fan-out, and `gh pr view` can hold a gxserver worker for many
seconds. Background/switch-driven Git refreshes therefore publish local Git
state first and run the GitHub probe on this delay, so the RPC burst at the
switch instant never competes with terminal attach traffic.
*/
const GPUI_SIDEBAR_GIT_HUB_DEFERRED_PROBE_DELAY_MS = 1500;
/*
CDXC:SidebarGitMemo 2026-07-29:
`GxserverGitAction` members that change the working tree, the index, or a ref.
Running any of them invalidates that project's memoized Git state, so the memo
can only ever serve a repository the sidebar itself has not touched since.
*/
const GPUI_MUTATING_GIT_ACTIONS: ReadonlySet<string> = new Set<GxserverGitAction>([
  "addAll",
  "checkout",
  "checkoutNewBranch",
  "commit",
  "deleteLocalBranch",
  "deleteRemoteBranch",
  "merge",
  "pullFastForward",
  "push",
  "pushSetUpstream",
  "pushSetUpstreamCurrent",
]);
const GPUI_AUTO_SLEEP_MINUTE_MS = 60 * 1000;
const GPUI_WORKSPACE_TERMINAL_LIFECYCLE_BRIDGE_RETRY_DELAY_MS = 25;
const GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS = 400;
const GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS = 5000;
const GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_DELAY_MS = 400;
const GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_RETRY_DELAY_MS = 5000;
const GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS = 512;
const GPUI_SIDEBAR_DEFAULT_CLIENT_ID = "ghostex-gpui-sidebar";
const GPUI_REMOTE_GXSERVER_PRESENTATION_RECOVERY_DELAY_MS = 500;
const GPUI_GXSERVER_UNAVAILABLE_GROUP_ID = "gxserver-unavailable";
const GPUI_GXSERVER_CHATS_GROUP_ID = "combined-chats";
const GPUI_DEFAULT_VISIBLE_COUNT = 1;
/*
CDXC:SidebarV2Lifecycle 2026-07-29:
Toast titles for a refused settle/snooze. Named per endpoint so the user learns
which action failed without the toast ever repeating a session title, project
path, or the daemon's response body.
*/
const SESSION_LIFECYCLE_FAILURE_TITLES: Record<string, string> = {
  "/api/settleSession": "Settle failed",
  "/api/snoozeSession": "Snooze failed",
  "/api/unsettleSession": "Un-settle failed",
  "/api/unsnoozeSession": "Wake failed",
};
const GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.nativeProjectPathAction";
const GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE = "ghostex.gpui.sidebar.commandAction";
const GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE = "ghostex.gpui.sidebar.commandRunEnd";
const GPUI_SIDEBAR_COMMAND_SELECTOR_MESSAGE_KEYS = new Set([
  "commandId",
  "groupId",
  "runMode",
  "scope",
  "type",
]);
const GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.gxserverPresentationFocusState";
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalFocus";
const GPUI_SIDEBAR_T3_SESSION_FOCUS_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_T3_SESSION_FOCUS_MESSAGE_TYPE = "ghostex.gpui.sidebar.t3SessionFocus";
const GPUI_SIDEBAR_T3_SESSION_CREATE_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_T3_SESSION_CREATE_MESSAGE_TYPE = "ghostex.gpui.sidebar.t3SessionCreate";
const GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE = "ghostex.gpui.sidebar.openBrowserUrl";
const GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS = 16 * 1024;
const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.projectBoardConversationRequest";
const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.projectBoardConversationResponse";
const GPUI_QUICK_AUTOMATIONS_PROJECT_ID = "quick-automations";
const GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE = "Automations Overview";
const GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID = "__quick-automations__";
const GPUI_AGENT_PROMPT_READY_DELAY_MS = 4_000;
const GPUI_AGENT_PROMPT_STEP_DELAY_MS = 1_000;
const GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_TTL_MS = 60_000;
const GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_CACHE_MAX = 512;
const GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY = 4;
/*
CDXC:ProjectBoardBeads 2026-08-07:
Resuming a bead's closed conversation runs through the daemon's fork plan,
which only knows how to continue Codex, Claude, and Pi conversations. gxserver
stays the authority and rejects anything else, so this set exists to keep the
board from offering a Resume the daemon would refuse.
*/
const GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS = new Set(["claude", "codex", "pi"]);
const GPUI_SIDEBAR_T3_BROWSER_ACCESS_REQUEST_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_T3_BROWSER_ACCESS_REQUEST_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.t3SessionBrowserAccessRequest";
const GPUI_SIDEBAR_T3_BROWSER_ACCESS_TITLE_MAX_CHARS = 160;
const GPUI_T3_BROWSER_ACCESS_MODES = new Set([
  "external",
  "local-network",
  "local-only",
  "tailscale",
]);
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalRenameCommand";

function gpuiWorkspaceTerminalTitleCommandForAgent(
  agentId: string,
): "name" | "rename" | "title" {
  const normalizedAgentId = agentId.trim().toLowerCase();
  if (normalizedAgentId === "pi" || normalizedAgentId === "π") {
    return "name";
  }
  if (
    normalizedAgentId === "hermes" ||
    normalizedAgentId === "hermes agent" ||
    normalizedAgentId === "hermes-agent"
  ) {
    return "title";
  }
  return "rename";
}
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalLifecycleRequest";
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalLifecycleResult";
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalBell";
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalEscapePressed";
const GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel";
const GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge";
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTerminalRuntimeAction";
const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.sessionCompletionSound";
const GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE = "ghostex.gpui.sidebar.globalActions";
/*
 * CDXC:GlobalActions 2026-08-01:
 * The tab strip is gpui-drawn, so it cannot read the HUD store the React
 * surfaces use. Cap what crosses the bridge at the number of buttons the strip
 * will actually draw; gpui rejects a longer list outright rather than
 * truncating it, so the two caps must agree.
 */
const GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS = 8;
const GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.sessionStatusIndicators";
const GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE = "ghostex.gpui.sidebar.petOverlayState";
const GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE = "ghostex.gpui.sidebar.statusPetActivation";
const GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.menuBarProjectActivation";
const GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.menuBarSessionActivation";
const GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.workspaceTabSessionSelected";
const GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.commandPaletteSessionFocus";
const GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.commandPaletteRunSidebarCommand";
const GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE = "ghostex.gpui.sidebar.nativeAppShotCaptured";
const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE = "ghostex.gpui.sidebar.nativeAppShotPrompt";
const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION = 1;
const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE =
  "ghostex.gpui.sidebar.nativeAppShotPromptResult";
const GPUI_SIDEBAR_REMOTE_EVENT_NAME = "ghostex-gpui-sidebar-remote-event";
const APP_SHOT_RECENT_TARGET_MS = 60_000;
const APP_SHOT_PROMPT_INSERT_RESULT_TIMEOUT_MS = 2_000;
const GPUI_STATUS_INDICATOR_MAX_CANDIDATES = 96;
const GPUI_STATUS_INDICATOR_MAX_PROJECTS = 32;
const GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT = 16;
const GPUI_STATUS_INDICATOR_ID_MAX_CHARS = 256;
const GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS = 120;
const GPUI_RENDERER_COMMAND_RENAME_TITLE_MAX_CHARS = 120;
const GPUI_RENDERER_COMMAND_RENAME_TITLE_CONTROL_PATTERN = /[\u0000-\u001f\u007f-\u009f]/u;
const DEFAULT_GPUI_PROMPT_AGENT_ID = "codex";

const GPUI_BACKGROUND_COMMIT_MESSAGE_DEFAULT_AGENT_IDS = new Set([
  "claude",
  "codex",
  "cursor",
  "gemini",
]);

type GpuiSidebarNativeProjectPathAction =
  | "copyRecentProjectPath"
  | "openRecentProjectInFinder"
  | "copyWorkspaceProjectPath"
  | "openWorkspaceProjectInFinder"
  | "openWorkspaceProjectInIde"
  | "openActiveWorkspaceProjectInFinder"
  | "openActiveWorkspaceProjectInVscode"
  | "openActiveWorkspaceProjectInZed"
  | "openExistingPullRequestInBrowser"
  | "openSidebarGitChangedFileInIde"
  | "copyRemoteProjectPath"
  | "copyRemoteProjectOpenFolderCommand"
  | "openRemoteWorkspaceProjectInIde"
  | "openRemoteWorkspaceProjectInVscode"
  | "openRemoteWorkspaceProjectInZed"
  | "openRemoteExistingPullRequestInBrowser"
  | "openRemoteSidebarGitChangedFileInIde"
  | "openRemoteProjectPortsBrowser"
  | "openRemoteSessionTerminal"
  | "copyRemoteAttachCommand"
  | "copyRemoteResumeCommand";

type GpuiTrustedExistingWorktreeList = {
  parentProjectId: string;
  paths: Set<string>;
  remoteMachineId?: string;
  sourceProjectId: string;
  worktreeKeys?: Set<string>;
};

type GpuiPendingGitCommitRequest = {
  action: Extract<SidebarGitAction, "commit" | "pr" | "push">;
  files: SidebarGitChangedFile[];
  hasCommit: boolean;
  projectId: string;
  remoteReference?: GpuiRemoteProjectReference;
  remoteTitle?: string;
  subject: string;
};

type GpuiPendingNativeAppShotPromptInsertion = {
  resolve: (ok: boolean) => void;
  sessionId: string;
  timeoutId: number;
};

type GpuiTrustedGitReviewFileSelection = {
  explicit: boolean;
  filePaths: string[];
};

type GpuiPendingRemoteGxserverRequest = {
  reject: (error: Error) => void;
  resolve: (result: unknown) => void;
  timeoutId: number;
};

type GpuiGxserverCreatedSessionResult = {
  session?: {
    projectId?: string;
    sessionId?: string;
  };
};

type GpuiNativeAppShotCapture = {
  appName: string;
  bundleIdentifier?: string;
  imagePath: string;
  trigger?: string;
  windowHeight?: number;
  windowTitle?: string;
  windowWidth?: number;
};

type GpuiWorktreeMetadata = {
  branch?: string;
  name?: string;
  parentProjectId: string;
  parentProjectName?: string;
};

type GpuiProjectWorktreeParentCandidate = {
  name?: string;
  path?: string;
  projectId: string;
  worktree?: Record<string, unknown>;
};

type GpuiGitPreferences = {
  confirmCommit: boolean;
  generateCommitBody: boolean;
  primaryAction: SidebarGitAction;
};

type GpuiRemoteProjectReference = {
  machineId: string;
  projectId: string;
};

type GpuiProjectDiffStatsRefreshTarget =
  | { key: string; kind: "local"; project: GxserverProjectDomainState }
  | { key: string; kind: "remote"; reference: GpuiRemoteProjectReference };

type GpuiRemoteProjectScope = GpuiRemoteProjectReference & {
  machineName?: string;
  project: GxserverPresentationProject;
};

type GpuiRemoteCreatePullRequestResult = {
  created?: boolean;
  ok?: boolean;
  pr?: {
    number?: number;
    state?: string;
  };
  reason?: string;
};

class GpuiUserVisibleGitError extends Error {}

const GPUI_GIT_MULTIPLE_COMMITS_PROMPT = `Please review my current changes and commit them as multiple focused commits.

Commit-splitting rules:
- Group changes by related feature, fix, or topic.
- Do not combine unrelated work in the same commit.
- Use file-based splitting only; do not split individual hunks.
- Make each commit easy to revert or cherry-pick later.
- Use clear, concise commit messages.`;

const GPUI_REMOTE_MERGE_CONFLICT_PROMPT =
  "A direct merge into main has conflicts in this remote project. Inspect the repository state, resolve the conflicts, and commit the merge when it is correct.";

const GPUI_GIT_RELEASE_STEPS_PROMPT = `1. Push any local commits to remote.
2. Review the commits since the last released version.
3. Update CHANGELOG.md to mention the new changes.
4. Publish the next minor version to the usual places we publish this app.`;

const GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT = `${GPUI_GIT_MULTIPLE_COMMITS_PROMPT}

After all focused commits are created:
${GPUI_GIT_RELEASE_STEPS_PROMPT}`;

const GPUI_GIT_RELEASE_ONLY_PROMPT = `Please release this app using the usual release workflow.

${GPUI_GIT_RELEASE_STEPS_PROMPT}`;

const GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY = "ghostex-gpui-remote-recent-projects";
const GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY = "ghostex-gpui-remote-group-order";
const GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY =
  "ghostex-gpui-remote-last-seen-presentations";
const GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_PERSIST_DELAY_MS = 2_000;

function createEmptyGpuiAppUserData(): GxserverAppUserData {
  return {
    pinnedPrompts: [],
    scratchPadContent: "",
  };
}

/*
CDXC:GPUISidebarGxserverRuntime 2026-06-24-11:00:
The production GPUI sidebar must mount the shared SidebarApp and hydrate it from gxserver presentation, never Storybook fixtures. Keep the renderer contract narrow: Rust/CEF installs baseUrl, authToken, protocolVersion, and optional active/focus ids on window.ghostexGpui.gxserverBootstrap; this adapter owns HTTP/WebSocket presentation flow, shared reducer/projection, active-project posting, and explicit unsupported handling for sidebar commands outside this slice.

CDXC:GPUISettingsMetadata 2026-06-24-11:59:
Settings project/worktree metadata in the GPUI SidebarApp still comes from real gxserver project domain rows, but read-side agent/action chrome now comes from `/api/readSidebarHud` so the renderer does not duplicate custom launcher/action normalization. Keep Beads/worktree metadata on project rows and never invent project paths when gxserver omits them.

CDXC:GPUISidebarProjectPathActions 2026-06-24-14:18:
Reused SidebarApp project path actions in GPUI may send only fixed action names plus trusted gxserver project ids to the sidebar-native bridge. The renderer must never send paths from DOM text, group labels, project titles, or cached project domain rows; Rust resolves ids through gxserver immediately before clipboard/Finder side effects.

CDXC:GPUISidebarProjectPathActions 2026-06-24-13:49:
Reused SidebarApp IDE-open messages in GPUI use the same pathless native project action bridge. The renderer maps group IDE opens to a Settings-owned fixed action and active workspace IDE opens to fixed VS Code/Zed action names plus gxserver project ids only; targetApp, editor commands, app names, paths, labels, URLs, and shell snippets stay out of the bridge payload so Rust owns editor selection and launch.

CDXC:GPUIWorktrees 2026-06-24-18:21:
The reused Add Worktree modal in GPUI must run local worktree create/open flows through gxserver typed endpoints instead of shelling from TypeScript or accepting arbitrary renderer paths. Remote worktree create/open must use id-scoped gxserver endpoints where the owning daemon derives target paths, branch refs, and Open Existing selections from project ids plus daemon-issued keys; do not route remote checkout paths or branch text through the renderer as authority.

CDXC:GPUIWorktrees 2026-06-24-14:06:
Open Existing prompt starts come from the reused modal's real prompt and
visible agent selector. Blank prompts keep the project-open-only behavior, but
a non-blank prompt must fail if the submitted agent is not configured instead
of silently opening the worktree without starting the requested session.

CDXC:GxserverAppUserData 2026-06-24-13:30:
Scratch Pad and Pinned Prompts in the reused GPUI SidebarApp must hydrate and
save through gxserver app-user-data, matching the app-modal host and macOS
sidebar. Keep note and prompt bodies inside authenticated RPC payloads only;
do not log them or persist them in a GPUI-only JSON file.

CDXC:GPUISidebarGit 2026-06-24-15:22:
GPUI Git controls may use gxserver-owned project ids and typed Git/GitHub/Beads endpoints for status, diffs, commit, push, and direct remote sync. Commit and PR creation paths must use the reused review modal or visible gxserver agent sessions, with remote-machine actions routed through the Rust-owned saved-machine tunnel and the owning remote gxserver.

CDXC:GPUISidebarGit 2026-06-24-15:43:
Existing pull-request browser open and changed-file IDE open are native GPUI side effects. React may send only fixed action names, gxserver project ids, and normalized project-relative file candidates from current HUD/review state; Rust must re-resolve PR URLs and changed-file membership through gxserver before launching a browser or editor.

CDXC:GPUISidebarGit 2026-06-24-15:55:
GPUI worktree completion may run direct merge-to-main and delete-after-cleanup only from a confirmed Git review request. The renderer uses the pending machine-scoped gxserver project id plus gxserver worktree parent metadata, fixed Git action names, and `/api/deleteWorktreeProject`; renderer paths, branch text, shell snippets, command output, and modal labels are never authority for side effects.

CDXC:GPUISidebarGit 2026-06-24-16:11:
Blank GPUI commit messages use a local gxserver generation endpoint after the reused commit modal validates the selected review files. The renderer sends only the trusted project id, review-approved relative paths, and selected prompt-agent id; gxserver stages/diffs the registered project and returns the subject/body used by the same commit pipeline.

CDXC:GPUISidebarGit 2026-06-24-16:28:
Direct/background GPUI PR creation must complete through gxserver before the UI opens a PR or removes a worktree. Reused review confirmations commit only validated review files, push with fixed Git action names, call the sanitized `/api/createPullRequest` project-id RPC, and run delete-after cleanup only after that result confirms an open PR; visible-agent PR workflows remain non-delete because they have no gxserver-owned PR completion signal.

CDXC:GPUISidebarGit 2026-06-24-16:45:
Visible PR-agent sessions expose gxserver lifecycle/activity only, not a trusted PR-created result. Preserve visible PR sessions for non-delete-after workflows, but route every delete-after PR request through the direct/background gxserver PR result before removing the original validated worktree.

CDXC:GPUIRemoteGit 2026-06-24-17:47:
Remote GPUI Git/GitHub/worktree actions must route through the Rust-owned saved-machine gxserver tunnel with machine-scoped project ids, reviewed file paths, fixed endpoint action names, and id-scoped worktree/branch operations only. Native side effects stay explicit: terminal focus uses remote attach, PR browser opens and copy-path use Rust revalidation, local Finder dereference remains unsupported for remote paths, and remote IDE opens require Rust-owned fixed editor support.

CDXC:GPUIRemoteAttach 2026-06-24-19:06:
Remote terminal focus and copy-attach commands may leave React only as fixed native action names plus machine-scoped remote presentation session ids. Rust owns saved-machine SSH details, gxserver attach/resume metadata, GPUI terminal launch payloads, and clipboard command construction so renderer state never carries tokens, hostnames, paths, or command text.

CDXC:GPUIRemoteNativeActions 2026-06-24-19:25:
Remote project copy-path, existing-PR browser open, Recent Projects Open Folder command-copy, and changed-file open intents may leave React only as fixed native action names plus machine-scoped project ids and review-approved relative file candidates. Rust must revalidate through the saved-machine gxserver tunnel before clipboard/browser/editor side effects; local Finder must never dereference remote paths, so Recent Projects Open Folder copies a saved-machine SSH command instead of opening Finder.

CDXC:GPUIRecentProjects 2026-06-25-19:30:
Remote Recent Projects Open Folder must follow the macOS sidebar source of truth by crossing the native bridge as `copyRemoteProjectOpenFolderCommand`, not by showing an unsupported GPUI toast or attempting local Finder. React may send only the machine-scoped project id; Rust owns path lookup, SSH command construction, clipboard write, and sanitized user feedback.

CDXC:GPUIRemoteNativeActions 2026-06-24-20:26:
Remote IDE project and changed-file opens are allowed only through Rust-owned fixed editor openers. React may request a fixed action for a machine-scoped project id, but it must never send remote paths, URI strings, SSH host/user/port/identity details, Settings custom commands, or editor command text.

CDXC:GPUIRemoteNativeActions 2026-06-24-21:33:
Zed remote opens are allowed through Rust-owned documented `zed ssh://[user@]host[:port]/path` argv only. React still sends only fixed action names and machine-scoped project ids; Cursor, Windsurf, VSCodium, Sublime, and custom remote editor commands remain unsupported without an equally reviewed native opener contract.

CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
Focused and visible session bootstrap state may use only gxserver presentation session ids the GPUI runtime already owns from create/focus/fork/restore results or machine-scoped remote presentation ids. Local ids stay raw gxserver session ids; remote ids use the existing `remote:<machine>:session:<project>:<session>` convention so React, Rust, and the CEF bootstrap never infer focus from labels, paths, terminal text, project names, or shell placeholder ids.

CDXC:GPUISidebarProjectClassification 2026-06-24-22:18:
GPUI must mirror the macOS sidebar projection rules for gxserver project domain metadata and canonical chat-folder paths. Legacy `isChat`/`isQuick`, `launchSettings.isChat`, `launchSettings.isQuick`, and projects under the Ghostex chats roots feed the synthetic Chats group instead of normal Project groups, `isRecentProject` rows stay out of active presentation groups, and automatic fallback focus must choose a visible non-chat project while explicit chat-session focus keeps the Chats group active.

CDXC:GPUISidebarProjectClassification 2026-06-24-22:51:
Generated Chat folders must not render as individual GPUI project groups, and clicking a chat session must not publish that chat folder as the active project to Rust. Treat host Ghostex-home chat roots, including dev `.active/chats` homes, as projectless Chats containers before building active-project context, Settings project rows, or Git HUD state.
*/
export function createGpuiSidebarRuntime(): {
  messageSource: GpuiSidebarLocalMessageSource;
  start: () => void;
  startLocalGxserver: () => void;
  vscode: WebviewApi;
} {
  const runtime = new GpuiSidebarRuntime();
  return {
    messageSource: runtime.messageSource,
    start: () => runtime.start(),
    startLocalGxserver: () => runtime.startLocalGxserver(),
    vscode: runtime.vscode,
  };
}

export class GpuiSidebarLocalMessageSource {
  private readonly eventTarget = new EventTarget();

  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: AddEventListenerOptions | boolean,
  ): void {
    this.eventTarget.addEventListener(type, listener, options);
  }

  removeEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: EventListenerOptions | boolean,
  ): void {
    this.eventTarget.removeEventListener(type, listener, options);
  }

  postMessage(
    message:
      | ExtensionToSidebarMessage
      | SidebarHydrateMessage
      | SidebarGroupsChangedMessage
      | SidebarHudChangedMessage
      | SidebarOrderSyncResultMessage
      | SidebarPreviousSessionsResultMessage
      | GpuiProjectWorktreesResultMessage,
  ): void {
    this.eventTarget.dispatchEvent(
      new MessageEvent("message", {
        data: message,
      }),
    );
  }
}

class GpuiSidebarRuntime {
  readonly messageSource = new GpuiSidebarLocalMessageSource();
  readonly vscode: WebviewApi = {
    postMessage: (message) => {
      void this.handleSidebarMessage(message);
    },
  };

  startLocalGxserver(): void {
    window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
      type: "startGxserverFromTitlebar",
    });
  }

  private notifyNativeGxserverPresentationReady(): void {
    window.requestAnimationFrame(() => {
      if (!this.presentation) {
        return;
      }
      window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
        type: "gxserverPresentationReady",
      });
    });
  }

  private activeProjectContextRetryId: number | undefined;
  private titlebarGitMenuStateRetryId: number | undefined;
  private lastTitlebarGitMenuStatePayload: string | undefined;
  private gitPollingIntervalId: number | undefined;
  private gitPollingTimeoutIds = new Set<number>();
  private pendingProjectDiffRefreshProjectIds = new Set<string>();
  private projectDiffStatsByProjectId = new Map<string, SidebarProjectDiffStats>();
  private activeGroupId: string | undefined;
  private activeProjectId: string | undefined;
  private appUserData: GxserverAppUserData = createEmptyGpuiAppUserData();
  private attentionAcknowledgementTimeoutsBySessionKey = new Map<string, number>();
  private attentionCompletionSoundEventKeys = new Set<string>();
  private attentionCompletionSoundEventKeyOrder: string[] = [];
  private attentionCompletionSoundSuppressedUntilBySessionKey = new Map<string, number>();
  private attentionEnteredAtBySessionKey = new Map<string, number>();
  private attentionEventIdBySessionKey = new Map<string, string>();
  private autoSleepMonitorIntervalId: number | undefined;
  private autoSleepMonitorRunning = false;
  private bootstrapPollTimeoutId: number | undefined;
  private browserTabs: GpuiBrowserTabSummary[] = [];
  private client: GpuiGxserverClient | undefined;
  private closeAfterDoneCountdownTickerId: number | undefined;
  private closeAfterDoneTimersBySessionId = new Map<string, GpuiCloseAfterDoneTimer>();
  private commandPaneSessions: GpuiCommandPaneSessionSummary[] = [];
  private workspaceSessionDelayedSends = new Map<string, GpuiWorkspaceSessionDelayedSendSummary>();
  private domainProjects: GxserverProjectDomainState[] = [];
  private focusedSessionId: string | undefined;
  private gxserverBootstrap: GpuiValidatedGxserverBootstrap | undefined;
  private gitState: SidebarGitState = createDefaultSidebarGitState();
  private hasHydrated = false;
  private latestGroups: SidebarSessionGroup[] = [];
  private latestHud: SidebarHudState = createGpuiSidebarHudState();
  private localFirstHiddenPresentationSessionKeys = new Set<string>();
  private lastAppShotTargetAt = 0;
  private lastAppShotTargetSessionId: string | undefined;
  /**
   * Which project the active Git HUD slot currently reflects. This is a
   * *presentation* marker, not a cache: it stops every republish of the same
   * project from re-entering the refresh path. Cross-project freshness lives in
   * `gitStateMemoByProjectId` below.
   */
  private lastGitRefreshProjectId: string | undefined;
  /*
  CDXC:SidebarGitMemo 2026-07-29:
  Per-project TTL memo for the local Git fan-out. Before this existed the
  runtime only remembered the last refreshed project, so switching A -> B -> A
  re-ran ~10 subprocess-spawning gxserver RPCs every time and starved terminal
  attach traffic. A switch back to a project with a fresh entry now publishes
  the memoized state and issues zero RPCs. Explicit and forced refreshes never
  read the memo, so manual refresh and every Git mutation still re-probe and
  then overwrite the entry.
  */
  private gitStateMemoByProjectId = new SidebarGitTtlMemo<SidebarGitState>({
    ttlMs: SIDEBAR_GIT_STATE_MEMO_TTL_MS,
  });
  /*
  CDXC:SidebarGitMemo 2026-07-29:
  GitHub CLI results get their own, much longer lease because `gh pr view` is a
  network round trip and pull-request state changes on a human timescale. Kept
  separate from the Git-state memo so a deferred probe landing later can be
  overlaid onto an already-published (or already-memoized) local Git state.
  */
  private gitHubStateMemoByProjectId = new SidebarGitTtlMemo<GpuiSidebarGitHubState>({
    ttlMs: SIDEBAR_GIT_HUB_MEMO_TTL_MS,
  });
  private pendingGitHubProbeProjectIds = new Set<string>();
  private gitHubProbeTimeoutIds = new Set<number>();
  private locallyAcknowledgedAttentionEventKeys = new Set<string>();
  private locallyAcknowledgedAttentionEventKeyOrder: string[] = [];
  private pendingNativeAppShotPromptInsertions: GpuiPendingNativeAppShotPromptInsertion[] = [];
  private pendingGitCommitRequests = new Map<string, GpuiPendingGitCommitRequest>();
  private pendingRemoteGxserverRequests = new Map<string, GpuiPendingRemoteGxserverRequest>();
  private presentation: GxserverPresentationSnapshot | undefined;
  private previousSessionsByHistoryId = new Map<string, SidebarPreviousSessionItem>();
  private projectBoardRestorableLinkChecks = new Map<
    string,
    { checkedAt: number; restorable: boolean; resumable: boolean; title?: string }
  >();
  private quickAutomationsOverviewOpen = false;
  private previousSessionsResult:
    | {
        cursor?: string;
        previousSessions: SidebarPreviousSessionItem[];
        query?: string;
        requestId: string;
      }
    | undefined;
  private recentProjects: GxserverRecentProjectDomainState[] = [];
  private remoteGxserverRequestSequence = 0;
  private remotePresentations = new Map<string, GxserverPresentationSnapshot>();
  private remoteLastSeenPresentations = new Map<string, GxserverPresentationSnapshot>();
  private remoteLastSeenPersistTimeoutId: number | undefined;
  private remotePresentationRecoveryTimeouts = new Map<string, number>();
  private remoteStartupReconnectAttempts = new Map<string, number>();
  private remoteStartupReconnectTimeouts = new Map<string, number>();
  private startupRemoteMachineIds = new Set<string>();
  private remoteRecentProjectsByMachineId = new Map<string, GxserverRecentProjectDomainState[]>();
  private remoteGroupOrderByMachineId = new Map<string, string[]>();
  private revision = 0;
  private runtimeSettings: GpuiSidebarRuntimeSettings | undefined;
  private sidebarHudState: GxserverSidebarHudResponse | undefined;
  private postedGlobalActionsPayload: string | undefined;

  /*
   * CDXC:GlobalActions 2026-08-01:
   * The gpui tab strip renders Global Actions natively and cannot read this
   * runtime's state, so every HUD change has to push the list across the
   * bridge. Routing all HUD writes through this accessor is what guarantees a
   * new assignment site cannot forget the push and leave the strip stale.
   */
  private get sidebarHud(): GxserverSidebarHudResponse | undefined {
    return this.sidebarHudState;
  }

  private set sidebarHud(hud: GxserverSidebarHudResponse | undefined) {
    this.sidebarHudState = hud;
    this.postGpuiGlobalActions();
  }
  private sleepingLocalSidebarSessionIds = new Set<string>();
  private subscription: GpuiPresentationSubscription | undefined;
  private trustedExistingWorktreeList: GpuiTrustedExistingWorktreeList | undefined;
  private visibleSessionIds = new Set<string>();
  private didAutoMaterializeStartupSession = false;
  private didConnectSavedRemoteMachinesOnStartup = false;
  private workspaceGroups: GpuiWorkspaceSessionGroupsState =
    createEmptyGpuiWorkspaceSessionGroupsState();
  private workspaceGroupsServerSyncTimeoutId: number | undefined;
  private workspaceGroupsServerSyncPending = false;
  private latestSidebarProjectCollectionsUpdate: GxserverSidebarProjectCollectionsState | undefined;
  private sidebarProjectCollectionsServerSyncTimeoutId: number | undefined;
  private sidebarProjectCollectionsServerSyncPending = false;
  private lastForwardedSidebarProjectCollectionsJson: string | undefined;
  private lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId = new Map<string, string>();
  private workspaceTerminalLifecycleBridgeRetryId: number | undefined;

  start(): void {
    this.installGpuiBridgeCallbacks();
    this.runtimeSettings = currentGpuiRuntimeSettings();
    this.remoteRecentProjectsByMachineId = readStoredGpuiRemoteRecentProjects();
    this.remoteGroupOrderByMachineId = readStoredGpuiRemoteGroupOrder();
    this.remoteLastSeenPresentations = readStoredGpuiRemoteLastSeenPresentations();
    this.workspaceGroups = readStoredGpuiWorkspaceSessionGroupsState();
    for (const sessionId of readStoredGpuiCloseAfterDoneSessionIds()) {
      this.closeAfterDoneTimersBySessionId.set(sessionId, {});
    }
    window.addEventListener(GPUI_SIDEBAR_REMOTE_EVENT_NAME, this.handleGpuiSidebarRemoteEvent);
    this.publishUnavailable("bootstrap-pending");
    this.tryStartFromInstalledBootstrap(0);
    this.startGpuiAutoSleepMonitor();
    this.startGitPollingDriver();
    window.setTimeout(() => this.connectSavedRemoteMachinesOnStartup(), 0);
  }

  private connectSavedRemoteMachinesOnStartup(): void {
    /*
    CDXC:GPUIRemoteStartupReconnect 2026-07-21:
    Rust-owned SSH tunnels are process-local and therefore never survive an
    app restart. Reconnect every saved machine after React has mounted its
    message-source listener so cached last-seen rows become live again and the
    header receives the normal connecting/connected status sequence. Reuse the
    explicit reconnect bridge; renderer code still sends only the saved id and
    never receives SSH details or tokens. A retryable native failure arms one
    per-machine retry after ten seconds, capped at three retries for this app
    launch. Connecting successfully, exhausting the retry budget, or removing
    the machine from Settings ends that launch's retry cycle.
    */
    if (
      this.didConnectSavedRemoteMachinesOnStartup ||
      this.runtimeSettings?.settings === undefined
    ) {
      return;
    }
    this.didConnectSavedRemoteMachinesOnStartup = true;
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    for (const machine of settings.remoteMachines) {
      this.startupRemoteMachineIds.add(machine.id);
      this.reconnectRemoteMachine(machine.id, false);
    }
  }

  private reconcileStartupRemoteMachineRetryTargets(): void {
    if (!this.didConnectSavedRemoteMachinesOnStartup) {
      return;
    }
    const savedRemoteMachineIds = new Set(
      createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.map((machine) => machine.id),
    );
    for (const machineId of this.startupRemoteMachineIds) {
      if (savedRemoteMachineIds.has(machineId)) {
        continue;
      }
      this.finishRemoteStartupReconnects(machineId);
    }
  }

  private installGpuiBridgeCallbacks(): void {
    const gpuiBridge = (window.ghostexGpui = window.ghostexGpui ?? {});
    gpuiBridge.onSidebarHostMessage = (message) => {
      /*
      CDXC:GPUICommandPane 2026-06-24-23:49:
      Rust-owned command-pane Action lifecycle feedback enters the reused SidebarApp through the same local message source as gxserver presentation patches. Keep this callback typed to existing sidebar messages so GPUI can update button run-state without exposing generic IPC, command text, paths, terminal output, or persisted state to React.

      CDXC:GPUISidebarRename 2026-07-29:
      Rust also forwards sidebar-owned app-modal commands (Rename Session
      confirm, focused-session Close After Done toggles) through this one
      bridge callback. Those are sidebar-to-extension commands: posting them
      into the inbound React message source silently dropped them because
      SidebarApp has no inbound branch for them, so a modal rename never
      reached `handleSidebarMessage`/gxserver and no `/rename` was staged in
      the terminal. Route exactly these known command types to the runtime's
      own sidebar-message handler instead.
      */
      if (message.type === "renameSession" || message.type === "toggleCloseAfterDone") {
        void this.handleSidebarMessage(message);
        return;
      }
      this.messageSource.postMessage(message);
    };
    const applyBrowserTabs = (tabs: readonly GpuiBrowserTabSummary[] | undefined) => {
      const next = normalizeGpuiBrowserTabs(tabs);
      gpuiBridge.browserTabs = next;
      if (JSON.stringify(this.browserTabs) === JSON.stringify(next)) {
        return;
      }
      this.browserTabs = next;
      if (this.presentation) {
        this.publishPresentation("patch");
      }
    };
    gpuiBridge.onBrowserTabsChanged = applyBrowserTabs;
    applyBrowserTabs(gpuiBridge.browserTabs);
    gpuiBridge.onWorkspaceTerminalLifecycleRequest = (payload) => {
      /*
      CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
      GPUI native workspace lifecycle must follow macOS ownership: Rust commits Close locally before this callback and uses the sidebar only for asynchronous provider cleanup, while Sleep/Wake still report transition success through the fixed result bridge. Payloads are bounded ids plus action/request enums only; no titles, paths, commands, terminal text, URLs, tokens, or daemon bodies cross this callback.

      CDXC:GPUIWorkspaceLifecycle 2026-06-26-05:23:
      The callback may be installed before CEF exposes `postWorkspaceTerminalLifecycleResult`. Queue normalized requests until that bridge exists so Close provider cleanup and acknowledged Sleep/Wake transitions are not lost during startup.
      */
      this.handleOrQueueWorkspaceTerminalLifecycleRequest(payload);
    };
    const applyCommandPaneSessions = (
      sessions: readonly GpuiCommandPaneSessionSummary[] | undefined,
    ) => {
      /*
      CDXC:GPUICommandPane 2026-06-25-10:50:
      Rust owns GPUI command-pane session identity, activity, and active-tab state. The external bridge uses native-shaped `G...` local command-pane ids even though Rust internal shell state may still use numeric ids; the sidebar runtime only matches those sanitized summaries to current gxserver HUD command buttons by command id first and normalized title second, mirroring macOS without exposing command text, cwd, output, status-file paths, or shell-state JSON to React.
      */
      const next = normalizeGpuiCommandPaneSessions(sessions);
      gpuiBridge.commandPaneSessions = next;
      if (hasSameGpuiCommandPaneSessions(this.commandPaneSessions, next)) {
        return;
      }
      this.commandPaneSessions = next;
      this.publishHudPatch();
    };
    gpuiBridge.onCommandPaneSessionsChanged = applyCommandPaneSessions;
    applyCommandPaneSessions(gpuiBridge.commandPaneSessions);
    const applyWorkspaceSessionDelayedSends = (
      sessions: readonly GpuiWorkspaceSessionDelayedSendSummary[] | undefined,
    ) => {
      const next = normalizeGpuiWorkspaceSessionDelayedSends(sessions);
      gpuiBridge.workspaceSessionDelayedSends = next;
      const nextBySessionId = new Map(next.map((session) => [session.sessionId, session]));
      if (
        JSON.stringify([...this.workspaceSessionDelayedSends.values()]) === JSON.stringify(next)
      ) {
        return;
      }
      this.workspaceSessionDelayedSends = nextBySessionId;
      if (this.presentation) {
        this.publishPresentation("patch");
      }
    };
    gpuiBridge.onWorkspaceSessionDelayedSendsChanged = applyWorkspaceSessionDelayedSends;
    applyWorkspaceSessionDelayedSends(gpuiBridge.workspaceSessionDelayedSends);
    gpuiBridge.onNativeAppShotCaptured = (payload) => {
      void this.handleNativeAppShotCaptured(payload);
    };
    gpuiBridge.onNativeAppShotPromptResult = (payload) => {
      this.handleNativeAppShotPromptResult(payload);
    };
    gpuiBridge.onStatusPetActivation = (payload) => {
      this.handleGpuiStatusPetActivation(payload);
    };
    gpuiBridge.onMenuBarProjectActivation = (payload) => {
      this.handleGpuiMenuBarProjectActivation(payload);
    };
    gpuiBridge.onMenuBarSessionActivation = (payload) => {
      void this.handleGpuiMenuBarSessionActivation(payload);
    };
    gpuiBridge.onCommandPaletteSessionFocus = (payload) => {
      void this.handleGpuiCommandPaletteSessionFocus(payload);
    };
    gpuiBridge.onCommandPaletteRunSidebarCommand = (payload) => {
      this.handleGpuiCommandPaletteRunSidebarCommand(payload);
    };
    gpuiBridge.onT3SessionBrowserAccessResult = (payload) => {
      this.handleGpuiT3SessionBrowserAccessResult(payload);
    };
    gpuiBridge.onProjectBoardConversationRequest = (payload) => {
      void this.handleGpuiProjectBoardConversationRequest(payload);
    };
    gpuiBridge.onWorkspaceTabSessionSelected = (payload) => {
      this.handleGpuiWorkspaceTabSessionSelected(payload);
    };
    gpuiBridge.onWorkspaceFolderPicked = (payload) => {
      void this.handleGpuiWorkspaceFolderPicked(payload);
    };
    gpuiBridge.onWorkspaceSessionAttentionAcknowledge = (payload) => {
      this.handleGpuiWorkspaceSessionAttentionAcknowledge(payload);
    };
    gpuiBridge.onWorkspaceTerminalBell = (payload) => {
      void this.handleGpuiWorkspaceTerminalBell(payload);
    };
    // Bridge handler for `ghostex.gpui.sidebar.workspaceTerminalEscapePressed`.
    gpuiBridge.onWorkspaceTerminalEscapePressed = (payload) => {
      this.handleGpuiWorkspaceTerminalEscapePressed(payload);
    };
    // Bridge handler for
    // `ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel`.
    gpuiBridge.onWorkspaceFirstPromptTitleGenerationCancel = (payload) => {
      void this.handleGpuiWorkspaceFirstPromptTitleGenerationCancel(payload);
    };
    gpuiBridge.onWorkspaceTerminalRuntimeAction = (payload) => {
      void this.handleGpuiWorkspaceTerminalRuntimeAction(payload);
    };
    gpuiBridge.onTitlebarGitAction = (payload) => {
      this.handleGpuiTitlebarGitAction(payload);
    };
    gpuiBridge.onGitCommitModalCommand = (payload) => {
      void this.handleGpuiGitCommitModalCommand(payload);
    };
    gpuiBridge.onWorktreeModalCommand = (payload) => {
      this.handleGpuiWorktreeModalCommand(payload);
    };
    gpuiBridge.onOsIntegrationCommand = (payload) => {
      void this.handleGpuiOsIntegrationCommand(payload);
    };
    const pendingOsIntegrationCommands = Array.isArray(gpuiBridge.pendingOsIntegrationCommands)
      ? gpuiBridge.pendingOsIntegrationCommands.splice(0)
      : [];
    for (const payload of pendingOsIntegrationCommands) {
      void this.handleGpuiOsIntegrationCommand(payload);
    }
    const pendingStatusPetActivations = Array.isArray(gpuiBridge.pendingStatusPetActivations)
      ? gpuiBridge.pendingStatusPetActivations.splice(0)
      : [];
    if (pendingStatusPetActivations.length > 0) {
      /*
      CDXC:GPUIStatusPetOverlay 2026-06-26-05:07:
      GPUI status clicks, and a later pet slice using the same fixed shape, can arrive before the runtime installs callbacks. Drain only first-party activation payloads carrying bounded session ids, then route through focusSession; do not persist payloads or expose paths, titles, commands, URLs, tokens, terminal text, or a generic native event bus.
      */
      for (const payload of pendingStatusPetActivations) {
        this.handleGpuiStatusPetActivation(payload);
      }
    }
    const pendingMenuBarProjectActivations = Array.isArray(
      gpuiBridge.pendingMenuBarProjectActivations,
    )
      ? gpuiBridge.pendingMenuBarProjectActivations.splice(0)
      : [];
    if (pendingMenuBarProjectActivations.length > 0) {
      /*
      CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
      GPUI menu-bar project clicks can arrive before the SidebarApp runtime installs callbacks. Drain only fixed first-party project activation payloads carrying one bounded project id, then route through focusProjectId; do not persist payloads or expose paths, titles, commands, URLs, tokens, terminal text, or a generic native event bus.
      */
      for (const payload of pendingMenuBarProjectActivations) {
        this.handleGpuiMenuBarProjectActivation(payload);
      }
    }
    const pendingMenuBarSessionActivations = Array.isArray(
      gpuiBridge.pendingMenuBarSessionActivations,
    )
      ? gpuiBridge.pendingMenuBarSessionActivations.splice(0)
      : [];
    if (pendingMenuBarSessionActivations.length > 0) {
      /*
      CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
      GPUI menu-bar session clicks use a fixed first-party payload with bounded project/session ids. Drain queued clicks into the existing focusSession path so local clicks still use WorkspaceTerminalFocus and remote-shaped ids stay within reviewed focus routing.
      */
      for (const payload of pendingMenuBarSessionActivations) {
        void this.handleGpuiMenuBarSessionActivation(payload);
      }
    }
    const pendingCommandPaletteSessionFocusRequests = Array.isArray(
      gpuiBridge.pendingCommandPaletteSessionFocusRequests,
    )
      ? gpuiBridge.pendingCommandPaletteSessionFocusRequests.splice(0)
      : [];
    for (const payload of pendingCommandPaletteSessionFocusRequests) {
      void this.handleGpuiCommandPaletteSessionFocus(payload);
    }
    const pendingCommandPaletteRunSidebarCommands = Array.isArray(
      gpuiBridge.pendingCommandPaletteRunSidebarCommands,
    )
      ? gpuiBridge.pendingCommandPaletteRunSidebarCommands.splice(0)
      : [];
    for (const payload of pendingCommandPaletteRunSidebarCommands) {
      this.handleGpuiCommandPaletteRunSidebarCommand(payload);
    }
    const pendingT3SessionBrowserAccessResults = Array.isArray(
      gpuiBridge.pendingT3SessionBrowserAccessResults,
    )
      ? gpuiBridge.pendingT3SessionBrowserAccessResults.splice(0)
      : [];
    for (const payload of pendingT3SessionBrowserAccessResults) {
      this.handleGpuiT3SessionBrowserAccessResult(payload);
    }
    const pendingProjectBoardConversationRequests = Array.isArray(
      gpuiBridge.pendingProjectBoardConversationRequests,
    )
      ? gpuiBridge.pendingProjectBoardConversationRequests.splice(0)
      : [];
    for (const payload of pendingProjectBoardConversationRequests) {
      /*
      Kanban board conversation requests (getState first of all) routinely
      arrive before the sidebar runtime installs callbacks at startup. Drain
      them in order so early board loads answer instead of timing out.
      */
      void this.handleGpuiProjectBoardConversationRequest(payload);
    }
    const pendingWorkspaceTabSessionSelections = Array.isArray(
      gpuiBridge.pendingWorkspaceTabSessionSelections,
    )
      ? gpuiBridge.pendingWorkspaceTabSessionSelections.splice(0)
      : [];
    if (pendingWorkspaceTabSessionSelections.length > 0) {
      /*
      CDXC:GPUIWorkspaceSessionFocus 2026-06-26-08:01:
      Workspace tab clicks originate from Rust after the local tab is already selected. Drain them into sidebar focus only so startup-time delivery cannot re-enter the Rust workspace materialization bridge or create a focus loop.
      */
      for (const payload of pendingWorkspaceTabSessionSelections) {
        this.handleGpuiWorkspaceTabSessionSelected(payload);
      }
    }
    const pendingWorkspaceTerminalLifecycleRequests = Array.isArray(
      gpuiBridge.pendingWorkspaceTerminalLifecycleRequests,
    )
      ? gpuiBridge.pendingWorkspaceTerminalLifecycleRequests.splice(0)
      : [];
    this.drainPendingWorkspaceTerminalLifecycleRequests(pendingWorkspaceTerminalLifecycleRequests);
    const pendingWorkspaceFolderPicks = Array.isArray(gpuiBridge.pendingWorkspaceFolderPicks)
      ? gpuiBridge.pendingWorkspaceFolderPicks.splice(0)
      : [];
    for (const payload of pendingWorkspaceFolderPicks) {
      void this.handleGpuiWorkspaceFolderPicked(payload);
    }
    const pendingWorkspaceSessionAttentionAcknowledgements = Array.isArray(
      gpuiBridge.pendingWorkspaceSessionAttentionAcknowledgements,
    )
      ? gpuiBridge.pendingWorkspaceSessionAttentionAcknowledgements.splice(0)
      : [];
    for (const payload of pendingWorkspaceSessionAttentionAcknowledgements) {
      this.handleGpuiWorkspaceSessionAttentionAcknowledge(payload);
    }
    const pendingWorkspaceTerminalBells = Array.isArray(gpuiBridge.pendingWorkspaceTerminalBells)
      ? gpuiBridge.pendingWorkspaceTerminalBells.splice(0)
      : [];
    for (const payload of pendingWorkspaceTerminalBells) {
      void this.handleGpuiWorkspaceTerminalBell(payload);
    }
    const pendingWorkspaceTerminalEscapePresses = Array.isArray(
      gpuiBridge.pendingWorkspaceTerminalEscapePresses,
    )
      ? gpuiBridge.pendingWorkspaceTerminalEscapePresses.splice(0)
      : [];
    for (const payload of pendingWorkspaceTerminalEscapePresses) {
      this.handleGpuiWorkspaceTerminalEscapePressed(payload);
    }
    const pendingWorkspaceFirstPromptTitleGenerationCancels = Array.isArray(
      gpuiBridge.pendingWorkspaceFirstPromptTitleGenerationCancels,
    )
      ? gpuiBridge.pendingWorkspaceFirstPromptTitleGenerationCancels.splice(0)
      : [];
    for (const payload of pendingWorkspaceFirstPromptTitleGenerationCancels) {
      void this.handleGpuiWorkspaceFirstPromptTitleGenerationCancel(payload);
    }
    const pendingWorkspaceTerminalRuntimeActions = Array.isArray(
      gpuiBridge.pendingWorkspaceTerminalRuntimeActions,
    )
      ? gpuiBridge.pendingWorkspaceTerminalRuntimeActions.splice(0)
      : [];
    for (const payload of pendingWorkspaceTerminalRuntimeActions) {
      void this.handleGpuiWorkspaceTerminalRuntimeAction(payload);
    }
    const pendingTitlebarGitActions = Array.isArray(gpuiBridge.pendingTitlebarGitActions)
      ? gpuiBridge.pendingTitlebarGitActions.splice(0)
      : [];
    for (const payload of pendingTitlebarGitActions) {
      this.handleGpuiTitlebarGitAction(payload);
    }
    const pendingGitCommitModalCommands = Array.isArray(gpuiBridge.pendingGitCommitModalCommands)
      ? gpuiBridge.pendingGitCommitModalCommands.splice(0)
      : [];
    for (const payload of pendingGitCommitModalCommands) {
      void this.handleGpuiGitCommitModalCommand(payload);
    }
    const pendingWorktreeModalCommands = Array.isArray(gpuiBridge.pendingWorktreeModalCommands)
      ? gpuiBridge.pendingWorktreeModalCommands.splice(0)
      : [];
    for (const payload of pendingWorktreeModalCommands) {
      this.handleGpuiWorktreeModalCommand(payload);
    }
    const pendingNativeAppShotPromptResults = Array.isArray(
      gpuiBridge.pendingNativeAppShotPromptResults,
    )
      ? gpuiBridge.pendingNativeAppShotPromptResults.splice(0)
      : [];
    for (const payload of pendingNativeAppShotPromptResults) {
      this.handleNativeAppShotPromptResult(payload);
    }
    const pendingNativeAppShots = Array.isArray(gpuiBridge.pendingNativeAppShots)
      ? gpuiBridge.pendingNativeAppShots.splice(0)
      : [];
    if (pendingNativeAppShots.length > 0) {
      /*
      CDXC:GPUIAppShots 2026-06-25-23:07:
      Rust may deliver a native App Shot before the SidebarApp runtime finishes installing callbacks. Drain only the first-party queued capture payloads and keep them transient; do not persist app names, window titles, image paths, command text, terminal content, URLs, or side-channel metadata from this bridge.
      */
      for (const payload of pendingNativeAppShots) {
        void this.handleNativeAppShotCaptured(payload);
      }
    }
    gpuiBridge.onRuntimeSettingsChanged = (runtimeSettings) => {
      const didChange = !hasSameGpuiRuntimeSettings(this.runtimeSettings, runtimeSettings);
      this.runtimeSettings = runtimeSettings;
      this.connectSavedRemoteMachinesOnStartup();
      this.reconcileStartupRemoteMachineRetryTargets();
      if (!didChange) {
        return;
      }
      this.publishHudPatch();
      this.postGpuiStatusPetState();
      this.postActiveProjectContext();
      void this.runGpuiAutoSleepMonitor("settings-change");
    };
    gpuiBridge.onGxserverBootstrapChanged = (bootstrap) => {
      this.applyGxserverBootstrapChanged(bootstrap);
    };
  }

  private startGpuiAutoSleepMonitor(): void {
    if (this.autoSleepMonitorIntervalId !== undefined) {
      return;
    }
    /*
    CDXC:GPUISidebarAutoSleep 2026-06-27-01:24:
    GPUI owns only the SidebarApp/gxserver runtime policy loop for agent terminal Auto Sleep. Run a small idempotent monitor from the runtime lifecycle, use the normalized shared settings snapshot, and route every sleep through the existing gxserver session lifecycle path instead of adding Browser, project-editor, native-pane, or renderer-local sleep behavior.
    */
    this.autoSleepMonitorIntervalId = window.setInterval(() => {
      void this.runGpuiAutoSleepMonitor("interval");
    }, GPUI_AUTO_SLEEP_MONITOR_INTERVAL_MS);
    void this.runGpuiAutoSleepMonitor("startup");
  }

  /**
   * One background Git polling driver owns both project diff stats (all
   * visible non-Quick projects, local and remote) and the full titlebar Git
   * state (active local project only). Individual project probes stagger
   * across the interval so large sidebars do not shell out for every repo at
   * once, matching the macOS refresh loop.
   */
  private startGitPollingDriver(): void {
    if (this.gitPollingIntervalId !== undefined) {
      return;
    }
    this.scheduleGitPollingCycle();
    this.gitPollingIntervalId = window.setInterval(() => {
      this.scheduleGitPollingCycle();
    }, GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS);
  }

  private scheduleGitPollingCycle(): void {
    for (const timeoutId of this.gitPollingTimeoutIds) {
      window.clearTimeout(timeoutId);
    }
    this.gitPollingTimeoutIds.clear();
    const targets = this.getVisibleProjectDiffStatsRefreshTargets();
    if (targets.length === 0) {
      return;
    }
    const staggerStepMs = GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS / targets.length;
    targets.forEach((target, index) => {
      const timeoutId = window.setTimeout(
        () => {
          this.gitPollingTimeoutIds.delete(timeoutId);
          this.refreshProjectDiffStatsTarget(target);
        },
        Math.floor(index * staggerStepMs),
      );
      this.gitPollingTimeoutIds.add(timeoutId);
    });
  }

  private getVisibleProjectDiffStatsRefreshTargets(): GpuiProjectDiffStatsRefreshTarget[] {
    const localTargets: GpuiProjectDiffStatsRefreshTarget[] = this.client
      ? this.domainProjects
          .filter(
            (project) =>
              !isGpuiPresentationQuickDomainProject(project) &&
              project.isRecentProject !== true &&
              Boolean(normalizeGpuiProjectPath(project.path)),
          )
          .map((project) => ({
            key: `local:${project.projectId}`,
            kind: "local",
            project,
          }))
      : [];
    const remoteTargets: GpuiProjectDiffStatsRefreshTarget[] = [];
    for (const [machineId, presentation] of this.remotePresentations.entries()) {
      for (const project of presentation.projects) {
        remoteTargets.push({
          key: `remote:${machineId}:${project.projectId}`,
          kind: "remote",
          reference: { machineId, projectId: project.projectId },
        });
      }
    }
    return [...localTargets, ...remoteTargets].sort((left, right) =>
      left.key.localeCompare(right.key),
    );
  }

  private refreshProjectDiffStatsTarget(target: GpuiProjectDiffStatsRefreshTarget): void {
    if (target.kind === "remote") {
      void this.refreshRemoteProjectDiffStats(target.reference);
      return;
    }
    void this.refreshProjectDiffStats(target.project);
    if (this.activeProjectId === target.project.projectId) {
      /*
      CDXC:SidebarGitMemo 2026-07-29:
      This background cycle runs every 15s and can land on the same instant as a
      project switch. Local Git probes stay on their 15s cadence, but the
      GitHub CLI probe defers so this loop cannot reintroduce a `gh pr view`
      network call into a switch-time RPC burst, and so PR state is re-fetched
      on its own (much slower) lease instead of four times a minute.
      */
      void this.refreshGitState({
        deferGitHub: true,
        force: true,
        project: target.project,
        toastOnFailure: false,
      });
    }
  }

  private async refreshProjectDiffStats(project: GxserverProjectDomainState): Promise<void> {
    const projectId = project.projectId;
    if (this.pendingProjectDiffRefreshProjectIds.has(projectId) || !this.client) {
      return;
    }
    this.pendingProjectDiffRefreshProjectIds.add(projectId);
    this.setProjectDiffStats(projectId, {
      ...this.getProjectDiffStats(projectId),
      isLoading: true,
    });
    try {
      const repoCheck = await this.runGitAction(project, { action: "isInsideWorkTree" });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
        this.setProjectDiffStats(projectId, createDefaultSidebarProjectDiffStats(false));
        return;
      }
      const trackedDiff = await this.runGitAction(project, { action: "diffNumstat" });
      const trackedStats = parseGitNumstatDiffStats(trackedDiff.stdout);
      const hasTrackedLineChanges = trackedStats.additions > 0 || trackedStats.deletions > 0;
      const settings = createGpuiSidebarSettings(this.runtimeSettings);
      let resolvedStats = trackedStats;
      if (settings.showUntrackedProjectDiffWhenNoTrackedChanges && !hasTrackedLineChanges) {
        const untrackedFiles = await this.runGitAction(project, { action: "listUntracked" });
        const untrackedPaths = parseGitZeroDelimitedPaths(untrackedFiles.stdout);
        resolvedStats = resolveSidebarProjectDiffStats({
          showUntrackedWhenNoTrackedChanges: true,
          trackedStats,
          untrackedStats: {
            additions: await this.countUntrackedProjectLines(project, untrackedPaths),
            deletions: 0,
            files: untrackedPaths.length,
            isLoading: false,
            isRepo: true,
          },
        });
      }
      this.setProjectDiffStats(projectId, resolvedStats);
    } catch {
      this.setProjectDiffStats(projectId, {
        ...this.getProjectDiffStats(projectId),
        isLoading: false,
      });
    } finally {
      this.pendingProjectDiffRefreshProjectIds.delete(projectId);
    }
  }

  private async refreshRemoteProjectDiffStats(
    reference: GpuiRemoteProjectReference,
  ): Promise<void> {
    const scopedProjectId = createGpuiRemotePresentationProjectId(
      reference.machineId,
      reference.projectId,
    );
    if (this.pendingProjectDiffRefreshProjectIds.has(scopedProjectId)) {
      return;
    }
    this.pendingProjectDiffRefreshProjectIds.add(scopedProjectId);
    this.setProjectDiffStats(scopedProjectId, {
      ...this.getProjectDiffStats(scopedProjectId),
      isLoading: true,
    });
    try {
      const repoCheck = await this.runRemoteGitAction(reference, {
        action: "isInsideWorkTree",
      });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
        this.setProjectDiffStats(scopedProjectId, createDefaultSidebarProjectDiffStats(false));
        return;
      }
      const trackedDiff = await this.runRemoteGitAction(reference, { action: "diffNumstat" });
      const trackedStats = parseGitNumstatDiffStats(trackedDiff.stdout);
      const hasTrackedLineChanges = trackedStats.additions > 0 || trackedStats.deletions > 0;
      const settings = createGpuiSidebarSettings(this.runtimeSettings);
      let resolvedStats = trackedStats;
      if (settings.showUntrackedProjectDiffWhenNoTrackedChanges && !hasTrackedLineChanges) {
        const untrackedFiles = await this.runRemoteGitAction(reference, {
          action: "listUntracked",
        });
        const untrackedPaths = parseGitZeroDelimitedPaths(untrackedFiles.stdout);
        resolvedStats = resolveSidebarProjectDiffStats({
          showUntrackedWhenNoTrackedChanges: true,
          trackedStats,
          untrackedStats: {
            additions: await this.countRemoteUntrackedProjectLines(reference, untrackedPaths),
            deletions: 0,
            files: untrackedPaths.length,
            isLoading: false,
            isRepo: true,
          },
        });
      }
      this.setProjectDiffStats(scopedProjectId, resolvedStats);
    } catch {
      this.setProjectDiffStats(scopedProjectId, {
        ...this.getProjectDiffStats(scopedProjectId),
        isLoading: false,
      });
    } finally {
      this.pendingProjectDiffRefreshProjectIds.delete(scopedProjectId);
    }
  }

  private async countUntrackedProjectLines(
    project: GxserverProjectDomainState,
    paths: readonly string[],
  ): Promise<number> {
    let lines = 0;
    for (const path of paths) {
      const result = await this.runGitAction(project, {
        action: "countFileLines",
        filePaths: [path],
      });
      if (result.exitCode !== 0) {
        throw new Error("Could not count untracked file lines.");
      }
      lines += Number(result.stdout.trim()) || 0;
    }
    return lines;
  }

  private async countRemoteUntrackedProjectLines(
    reference: GpuiRemoteProjectReference,
    paths: readonly string[],
  ): Promise<number> {
    let lines = 0;
    for (const path of paths) {
      const result = await this.runRemoteGitAction(reference, {
        action: "countFileLines",
        filePaths: [path],
      });
      if (result.exitCode !== 0) {
        throw new Error("Could not count remote untracked file lines.");
      }
      lines += Number(result.stdout.trim()) || 0;
    }
    return lines;
  }

  private getProjectDiffStats(projectId: string): SidebarProjectDiffStats {
    return (
      this.projectDiffStatsByProjectId.get(projectId) ?? createDefaultSidebarProjectDiffStats()
    );
  }

  private setProjectDiffStats(projectId: string, stats: SidebarProjectDiffStats): void {
    this.projectDiffStatsByProjectId.set(projectId, stats);
    if (!this.hasHydrated) {
      return;
    }
    // Diff stats live inside the group projection (projectContext.editor), so
    // republish groups through the existing patch path instead of a HUD-only
    // update.
    this.publishRemotePresentationPatch();
  }

  private async runGpuiAutoSleepMonitor(
    _source: "interval" | "settings-change" | "startup",
  ): Promise<void> {
    if (this.autoSleepMonitorRunning) {
      return;
    }
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    if (!settings.autoSleepAgentSessionsEnabled || !this.presentation) {
      return;
    }
    const sessionIdsToSleep = createGpuiAutoSleepAgentSessionIds({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      groups: this.latestGroups,
      nowMs: Date.now(),
      presentation: this.presentation,
      settings,
    });
    if (sessionIdsToSleep.length === 0) {
      return;
    }
    this.autoSleepMonitorRunning = true;
    try {
      /*
      CDXC:GPUISidebarAutoSleep 2026-06-27-02:05:
      Auto Sleep must match native bulk sleep pacing: eligible agent sessions sleep one at a time with a 350 ms gap so gxserver and terminal teardown are not hit concurrently. Use the shared aggregate-count helper and ignore its private-data-free result because monitor progress is already reflected by gxserver presentation updates.
      */
      await runGpuiSidebarBulkSleepPaced(sessionIdsToSleep, async (sessionId) => {
        await this.setSessionSleeping(sessionId, true);
      });
    } finally {
      this.autoSleepMonitorRunning = false;
    }
  }

  private handleGpuiStatusPetActivation(payload: unknown): void {
    const activation = normalizeGpuiStatusPetActivation(payload);
    if (!activation) {
      return;
    }
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-05:07:
    Visible GPUI status activation, and later pet activation, must re-enter the sidebar runtime's existing focusSession route. Keep this as a fixed callback with one bounded session id so local focus stays local, remote focus uses the reviewed remote native action path, and Rust never creates or wakes unrelated sessions for indicator clicks.
    */
    void this.focusSession(activation.sessionId, {
      sessionId: activation.sessionId,
      type: "focusSession",
    });
  }

  private handleGpuiMenuBarProjectActivation(payload: unknown): void {
    const activation = normalizeGpuiMenuBarProjectActivation(payload);
    if (!activation) {
      return;
    }
    /*
    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
    Running Agents project rows should behave like focusing the matching sidebar project group. Reuse local focusProjectId or the remote group projection plus the normal presentation publish instead of creating a native-only project switch path, and accept only the bounded project id from Rust.
    */
    const remoteProject = parseGpuiRemotePresentationProjectId(activation.projectId);
    if (remoteProject) {
      this.activeGroupId = createGpuiRemotePresentationGroupId(
        remoteProject.machineId,
        remoteProject.projectId,
      );
      this.publishRemotePresentationPatch();
      return;
    }
    this.focusProjectId(activation.projectId);
    this.publishPresentation("patch");
  }

  private async handleGpuiMenuBarSessionActivation(payload: unknown): Promise<void> {
    const activation = normalizeGpuiMenuBarSessionActivation(payload);
    if (!activation) {
      return;
    }
    /*
    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
    Running Agents session rows should behave like sidebar session-card clicks. Normalize raw local gxserver ids into the existing project-scoped presentation id when needed, then reuse focusSession so local clicks update presentation focus and post WorkspaceTerminalFocus back to Rust for terminal selection/materialization.
    */
    const sessionId = gpuiMenuBarStatusSessionFocusRoutingId(
      activation.projectId,
      activation.sessionId,
    );
    await this.focusSession(sessionId, {
      sessionId,
      type: "focusSession",
    });
  }

  private async handleGpuiCommandPaletteSessionFocus(payload: unknown): Promise<void> {
    /*
    Command-palette current-session rows post {type:"focusSession"} from the
    app-modal host window; Rust forwards only the bounded projected session id
    here so palette selection reuses the same reviewed focusSession routing as
    sidebar card clicks (local materialize/wake, remote-shaped ids included).
    */
    const sessionId = normalizeGpuiCommandPaletteSessionFocus(payload);
    if (!sessionId) {
      return;
    }
    await this.focusSession(sessionId, {
      sessionId,
      type: "focusSession",
    });
  }

  private handleGpuiCommandPaletteRunSidebarCommand(payload: unknown): void {
    /*
    Command-palette Action rows post {type:"runSidebarCommand"} from the
    app-modal host window; Rust forwards only the selector (command id +
    optional runMode). Execution resolves the trusted saved/HUD command and
    goes through the same strict SidebarCommandAction bridge as sidebar-surface
    Action clicks.
    */
    const selection = normalizeGpuiCommandPaletteRunSidebarCommand(payload);
    if (!selection) {
      return;
    }
    this.runSidebarCommand(selection.message.commandId, selection.message, selection.scope);
  }

  private requestT3SessionBrowserAccess(sessionId: string): void {
    if (!T3CODE_ENABLED) {
      return;
    }
    /*
    macOS `requestNativeT3SessionBrowserAccess` parity for the T3 card's
    Remote Access action. The runtime revalidates the local T3 presentation
    row and sends Rust only the bounded project/session ids plus a display
    title; Rust owns the bearer read, the pairing-token issue, and the
    network-address detection (no runtime start — Decision #7).
    */
    if (parseGpuiRemotePresentationSessionId(sessionId)) {
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (
      !reference ||
      !this.isLocalPresentationT3Session(reference.projectId, reference.sessionId)
    ) {
      return;
    }
    const sessionTitle = (
      this.presentation?.sessions.find(
        (session) =>
          session.projectId === reference.projectId && session.sessionId === reference.sessionId,
      )?.title ?? "Chat"
    ).slice(0, GPUI_SIDEBAR_T3_BROWSER_ACCESS_TITLE_MAX_CHARS);
    const post = window.ghostexGpui?.postT3SessionBrowserAccessRequest;
    if (typeof post !== "function") {
      this.postT3RemoteAccessToast("error", "Remote Access unavailable", {
        description: "The native Remote Access bridge is not installed.",
      });
      return;
    }
    post(
      JSON.stringify({
        projectId: reference.projectId,
        sessionId: reference.sessionId,
        sessionTitle,
        type: GPUI_SIDEBAR_T3_BROWSER_ACCESS_REQUEST_MESSAGE_TYPE,
        version: GPUI_SIDEBAR_T3_BROWSER_ACCESS_REQUEST_MESSAGE_VERSION,
      }),
    );
  }

  private handleGpuiT3SessionBrowserAccessResult(payload: unknown): void {
    /*
    Rust posts the resolved pairing link (or an honest error) back here; the
    success path travels the macOS route: a `showT3BrowserAccess` SidebarApp
    message whose handler opens the shared QR modal through the app-modal
    host bridge.
    */
    if (typeof payload !== "object" || payload === null) {
      return;
    }
    const record = payload as Record<string, unknown>;
    if (record.ok !== true) {
      const description =
        typeof record.message === "string" && record.message.trim()
          ? record.message.trim()
          : "Could not create the T3 remote access link.";
      this.postT3RemoteAccessToast("error", "Remote Access failed", { description });
      return;
    }
    const endpointUrl = typeof record.endpointUrl === "string" ? record.endpointUrl.trim() : "";
    const localUrl = typeof record.localUrl === "string" ? record.localUrl.trim() : "";
    const mode = typeof record.mode === "string" ? record.mode : "";
    const note = typeof record.note === "string" ? record.note : "";
    const projectId = typeof record.projectId === "string" ? record.projectId : "";
    const sessionId = typeof record.sessionId === "string" ? record.sessionId : "";
    const sessionTitle = typeof record.sessionTitle === "string" ? record.sessionTitle : "";
    const tailscaleEnabled = record.tailscaleEnabled === true;
    const urlsAreHttp = [endpointUrl, localUrl].every(
      (url) => url.startsWith("http://") || url.startsWith("https://"),
    );
    if (
      !endpointUrl ||
      !localUrl ||
      !urlsAreHttp ||
      !GPUI_T3_BROWSER_ACCESS_MODES.has(mode) ||
      !projectId ||
      !sessionId
    ) {
      return;
    }
    this.messageSource.postMessage({
      endpointUrl,
      localUrl,
      mode,
      note,
      sessionId: createGxserverPresentationProjectSessionId(projectId, sessionId),
      sessionTitle,
      tailscaleEnabled,
      type: "showT3BrowserAccess",
    });
  }

  private postT3RemoteAccessToast(
    level: AppToastLevel,
    title: string,
    options: { description?: string } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description, {}),
        "AppModals:gpuiT3BrowserAccess",
      );
    } catch {
      // The missing toast bridge is a presentation problem only.
    }
  }

  private async handleGpuiProjectBoardConversationRequest(payload: unknown): Promise<void> {
    /*
    macOS `handleProjectBoardRequest` parity for the conversation half of the
    Kanban board bridge: Rust forwards the first-party page request here
    because the sidebar runtime — the GPUI equivalent of native-sidebar.tsx —
    owns agents, presentation state, focus routing, worktree creation, and the
    gxserver client. Links persist in the daemon's
    `projectBoardConfig.beadConversationLinks`, the same durable storage the
    macOS remote board flow writes.
    */
    const request = normalizeGpuiProjectBoardConversationRequest(payload);
    if (!request) {
      return;
    }
    const respond = (response: { error?: string; ok: boolean; payload?: unknown }) => {
      this.postGpuiProjectBoardConversationResponse({
        ...response,
        requestId: request.requestId,
      });
    };
    try {
      switch (request.action) {
        case "showToast": {
          this.postSidebarActionToast(
            normalizeGpuiProjectBoardToastLevel(request.toastLevel),
            request.toastTitle?.trim() || "Project Board update failed",
            { description: request.toastDescription?.trim() || undefined },
          );
          respond({ ok: true });
          return;
        }
        case "appendDebugLog":
        case "getState": {
          // appendDebugLog answers with state like macOS; the sanitized log
          // line itself is written by Rust before this request is forwarded
          // (dispatch_gpui_project_board_conversation_request), so the
          // runtime only supplies the state echo.
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case "associateFocusedSession": {
          await this.associateGpuiProjectBoardFocusedSession(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case "startWork": {
          await this.startGpuiProjectBoardWork(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case "jumpToConversation": {
          await this.jumpToGpuiProjectBoardConversation(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case "unlinkConversation": {
          await this.unlinkGpuiProjectBoardConversation(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
      }
    } catch (error) {
      respond({
        error:
          error instanceof Error && error.message.trim()
            ? error.message
            : "Project board conversation action failed.",
        ok: false,
      });
    }
  }

  private postGpuiProjectBoardConversationResponse(response: {
    error?: string;
    ok: boolean;
    payload?: unknown;
    requestId: string;
  }): void {
    const post = window.ghostexGpui?.postProjectBoardConversationResponse;
    if (typeof post !== "function") {
      return;
    }
    post(
      JSON.stringify({
        response,
        type: GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE,
        version: GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION,
      }),
    );
  }

  private async resolveGpuiProjectBoardDomainProject(request: {
    projectId?: string;
    projectPath?: string;
  }): Promise<GxserverProjectDomainState> {
    return (await this.resolveGpuiProjectBoardDomainScope(request)).boardProject;
  }

  private async resolveGpuiProjectBoardDomainScope(request: {
    projectId?: string;
    projectPath?: string;
  }): Promise<{
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: GxserverProjectDomainState[];
  }> {
    const projects = await this.listGpuiProjectBoardDomainProjects();
    const boardProject = this.selectGpuiProjectBoardDomainProject(request, projects);
    return {
      boardProject,
      linkStoreProjects: selectBeadConversationLinkStoreProjects(boardProject, projects),
    };
  }

  private async listGpuiProjectBoardDomainProjects(): Promise<GxserverProjectDomainState[]> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    // Read fresh domain projects so link mutations from other clients are
    // visible to every board response.
    const response = await this.client.rpc<{ projects?: GxserverProjectDomainState[] }>(
      "/api/listProjects",
      {},
    );
    return Array.isArray(response.projects) ? response.projects : [];
  }

  private selectGpuiProjectBoardDomainProject(
    request: { projectId?: string; projectPath?: string },
    projects: readonly GxserverProjectDomainState[],
  ): GxserverProjectDomainState {
    // macOS `resolveProjectBoardProject` order: project id, then path, then
    // the active project.
    const projectId = request.projectId?.trim();
    const byId = projectId
      ? projects.find((candidate) => candidate.projectId === projectId)
      : undefined;
    if (byId) {
      return byId;
    }
    const normalizedPath = normalizeGpuiProjectPath(request.projectPath);
    const byPath = normalizedPath
      ? projects.find((candidate) => normalizeGpuiProjectPath(candidate.path) === normalizedPath)
      : undefined;
    if (byPath) {
      return byPath;
    }
    const active = this.activeDomainProject();
    if (active) {
      return projects.find((candidate) => candidate.projectId === active.projectId) ?? active;
    }
    throw new Error("Project not found.");
  }

  private async createGpuiProjectBoardConversationState(request: {
    projectId?: string;
    projectPath?: string;
  }): Promise<ProjectBoardConversationState> {
    const { boardProject, linkStoreProjects } =
      await this.resolveGpuiProjectBoardDomainScope(request);
    const sessionOptions = this.createGpuiProjectBoardSessionOptions(
      boardProject,
      linkStoreProjects,
    );
    const sessionById = new Map(sessionOptions.map((session) => [session.sessionId, session]));
    const activeLinks = canonicalizeBeadConversationLinksForBoard(
      this.readGpuiProjectBoardConversationLinks(linkStoreProjects).filter(
        (link) => link.status !== "archived",
      ),
      boardProject.projectId,
    );
    const linkViews: ProjectBoardConversationLinkView[] = [];
    for (
      let start = 0;
      start < activeLinks.length;
      start += GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY
    ) {
      linkViews.push(
        ...(await Promise.all(
          activeLinks
            .slice(start, start + GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY)
            .map(async (link) => {
              const session =
                sessionById.get(link.ghostexSessionId) ??
                this.findGpuiProjectBoardLinkedSessionOption(
                  boardProject,
                  link.ghostexSessionId,
                );
              const availability = session
                ? undefined
                : await this.checkGpuiProjectBoardLinkAvailability(
                    boardProject,
                    link.ghostexSessionId,
                  );
              return {
                ...link,
                agentId: link.agentId ?? session?.agentId,
                isFocused: session?.isFocused,
                isLive: Boolean(session),
                isRestorable: availability?.restorable === true,
                isResumable: availability?.resumable === true,
                isSleeping: session?.isSleeping,
                sessionTitle: session?.label ?? availability?.title,
              };
            }),
        )),
      );
    }
    return {
      activeSessionId:
        this.activeProjectId === boardProject.projectId ? this.focusedSessionId : undefined,
      agents: this.createGpuiProjectBoardAgentOptions(),
      debuggingMode: this.runtimeSettings?.debuggingMode === true,
      // The board page gates appendDebugLog breadcrumbs on the
      // native.project.board scenario; Rust owns the actual writer and also
      // enforces the global Show debug UI controls gate.
      diagnosticLogging: normalizeghostexSettings(this.runtimeSettings?.settings).diagnosticLogging,
      defaultAgentId: this.resolveDefaultPromptAgentId(),
      focusedTerminalSessionId: sessionOptions.find((session) => session.isFocused)?.sessionId,
      links: linkViews,
      projectId: boardProject.projectId,
      sessions: sessionOptions,
    };
  }

  private readGpuiProjectBoardConversationLinks(
    linkStoreProjects: readonly GxserverProjectDomainState[],
  ): BeadConversationLink[] {
    return linkStoreProjects.flatMap((project) =>
      normalizeBeadConversationLinks(
        project.projectBoardConfig?.beadConversationLinks,
        project.projectId,
      ),
    );
  }

  private createGpuiProjectBoardAgentOptions(): ProjectBoardAgentOption[] {
    // macOS `createProjectBoardAgentOptions` sources the configured prompt
    // agents; GPUI's configured agent registry is the gxserver-fetched HUD
    // (the same source the daemon's automation agent list reads). T3 and
    // commandless agents cannot run board prompts.
    const agents: SidebarAgentButton[] = this.sidebarHud
      ? ([...this.sidebarHud.agents] as SidebarAgentButton[])
      : createSidebarAgentButtons([], []);
    return agents
      .filter((agent) => agent.agentId !== "t3" && Boolean(agent.command?.trim()))
      .map((agent) => ({
        agentId: agent.agentId,
        command: agent.command,
        icon: agent.icon,
        label: agent.name,
      }));
  }

  private createGpuiProjectBoardSessionOptions(
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[] = [],
  ): ProjectBoardSessionOption[] {
    const presentation = this.presentation;
    if (!presentation) {
      return [];
    }
    /*
    macOS `createProjectBoardConversationProjects` parity: a bead can be worked
    from a sibling worktree while its ticket stays on the parent board, so the
    option list spans the worktree family.

    CDXC:ProjectBoardBeads 2026-08-07:
    Rows that mount the same Beads board are part of the same board too. Their
    sessions belong in the list, or a link inherited from one of them reads as
    dead while its session is still running.
    */
    const familyParentId =
      normalizeGpuiWorktreeParentProjectId(boardProject.worktree) ?? boardProject.projectId;
    const relatedProjectIds = new Set<string>([
      boardProject.projectId,
      ...linkStoreProjects.map((project) => project.projectId),
    ]);
    for (const candidate of this.domainProjects) {
      if (
        candidate.projectId === familyParentId ||
        normalizeGpuiWorktreeParentProjectId(candidate.worktree) === familyParentId
      ) {
        relatedProjectIds.add(candidate.projectId);
      }
    }
    const presentationProjectTitleById = new Map(
      presentation.projects.map((project) => [project.projectId as string, project.title]),
    );
    return presentation.sessions.flatMap((session): ProjectBoardSessionOption[] => {
      if (session.kind !== "terminal" && session.kind !== "agent") {
        return [];
      }
      if (!relatedProjectIds.has(session.projectId)) {
        return [];
      }
      const isBoardProject = session.projectId === boardProject.projectId;
      const label = isBoardProject
        ? session.title
        : `${presentationProjectTitleById.get(session.projectId) ?? session.projectId} · ${session.title}`;
      return [
        {
          agentId: session.agentName ?? session.agentId,
          isFocused:
            session.projectId === this.activeProjectId &&
            session.sessionId === this.focusedSessionId,
          isSleeping: this.isSleepingLocalPresentationSession(session.projectId, session.sessionId),
          label,
          sessionId: isBoardProject
            ? session.sessionId
            : createGxserverPresentationProjectSessionId(session.projectId, session.sessionId),
        },
      ];
    });
  }

  private findGpuiProjectBoardLinkedSessionOption(
    boardProject: GxserverProjectDomainState,
    ghostexSessionId: string,
  ): ProjectBoardSessionOption | undefined {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    The option list is scoped to the board's worktree family and board mounts,
    but a bead can be worked from any project. Jump already focuses such a
    session straight from presentation, so liveness is resolved the same way —
    otherwise a card offers to resume a conversation that is running right now.
    */
    const presentation = this.presentation;
    if (!presentation) {
      return undefined;
    }
    const reference = parseGxserverPresentationProjectSessionId(ghostexSessionId) ?? {
      projectId: boardProject.projectId,
      sessionId: ghostexSessionId,
    };
    const session = presentation.sessions.find(
      (candidate) =>
        candidate.projectId === reference.projectId &&
        candidate.sessionId === reference.sessionId &&
        (candidate.kind === "terminal" || candidate.kind === "agent"),
    );
    if (!session) {
      return undefined;
    }
    const isBoardProject = session.projectId === boardProject.projectId;
    const projectTitle = presentation.projects.find(
      (project) => project.projectId === session.projectId,
    )?.title;
    return {
      agentId: session.agentName ?? session.agentId,
      isFocused:
        session.projectId === this.activeProjectId && session.sessionId === this.focusedSessionId,
      isSleeping: this.isSleepingLocalPresentationSession(session.projectId, session.sessionId),
      label: isBoardProject
        ? session.title
        : `${projectTitle ?? session.projectId} · ${session.title}`,
      sessionId: ghostexSessionId,
    };
  }

  private async checkGpuiProjectBoardLinkAvailability(
    boardProject: GxserverProjectDomainState,
    ghostexSessionId: string,
  ): Promise<{ restorable: boolean; resumable: boolean; title?: string }> {
    /*
    macOS resolves link restorability from its previous-sessions cache with a
    gxserver fallback; GPUI keeps no such cache, so non-live links check the
    daemon directly behind a short TTL because getState re-runs on the board's
    8s auto-refresh.

    CDXC:ProjectBoardBeads 2026-08-07:
    Previous-session history only carries rows that closed with a trusted
    resume title, so a bead worked by a since-closed agent session usually has
    no restorable row at all. The daemon can still plan a resume from the
    session row's own agent identity, so ask it before calling the link dead.
    */
    const reference = parseGxserverPresentationProjectSessionId(ghostexSessionId) ?? {
      projectId: boardProject.projectId,
      sessionId: ghostexSessionId,
    };
    const cacheKey = `${reference.projectId}:${reference.sessionId}`;
    const now = Date.now();
    const cached = this.projectBoardRestorableLinkChecks.get(cacheKey);
    if (cached && now - cached.checkedAt < GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_TTL_MS) {
      return cached;
    }
    let result: { checkedAt: number; restorable: boolean; resumable: boolean; title?: string } = {
      checkedAt: now,
      restorable: false,
      resumable: false,
    };
    if (this.client) {
      try {
        const row = await this.findGpuiProjectBoardPreviousSessionRow(reference);
        result = {
          checkedAt: now,
          restorable: Boolean(row),
          resumable: row ? false : await this.checkGpuiProjectBoardLinkResumable(reference),
          title: row ? (row.displayTitle ?? row.primaryTitle ?? row.title) : undefined,
        };
      } catch {
        // An unavailable history lookup renders the link as not restorable
        // for this cycle; the next TTL window re-checks.
      }
    }
    if (
      this.projectBoardRestorableLinkChecks.size >=
      GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_CACHE_MAX
    ) {
      this.projectBoardRestorableLinkChecks.clear();
    }
    this.projectBoardRestorableLinkChecks.set(cacheKey, result);
    return result;
  }

  private async checkGpuiProjectBoardLinkResumable(reference: {
    projectId: string;
    sessionId: string;
  }): Promise<boolean> {
    // `/api/readAgentResumePlan` is the daemon's own answer to "can this
    // conversation come back": it plans from the stored agent session id,
    // session path, or trusted title, and returns no primary command when
    // there is nothing to resume. Command construction stays in gxserver.
    if (!this.client) {
      return false;
    }
    try {
      const response = await this.client.rpc<{ plan?: GxserverAgentResumePlan }>(
        "/api/readAgentResumePlan",
        { projectId: reference.projectId, sessionId: reference.sessionId },
      );
      return (
        Boolean(normalizeNonEmptyString(response.plan?.primaryCommand)) &&
        GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS.has(
          normalizeNonEmptyString(response.plan?.agentId)?.toLowerCase() ?? "",
        )
      );
    } catch {
      // A removed session row answers with an error; that is a dead link.
      return false;
    }
  }

  private async findGpuiProjectBoardPreviousSessionRow(reference: {
    projectId: string;
    sessionId: string;
  }): Promise<GxserverPresentationSearchResult | undefined> {
    if (!this.client) {
      return undefined;
    }
    const response = await this.client.rpc<GxserverPresentationSearchResponse>(
      "/api/listPreviousSessions",
      {
        includeActive: false,
        includePrevious: true,
        limit: 20,
        projectId: reference.projectId,
        query: reference.sessionId,
      },
    );
    return response.results?.find(
      (result) =>
        result.projectId === reference.projectId &&
        result.sessionId === reference.sessionId &&
        result.lifecycleState !== "running",
    );
  }

  private async reloadGpuiProjectBoardDomainScope(
    boardProject: GxserverProjectDomainState,
    fallbackLinkStoreProjects: readonly GxserverProjectDomainState[],
  ): Promise<{
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: GxserverProjectDomainState[];
  }> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    Starting work can take minutes before the link is written (the worktree
    path registers a project, runs setup, and refreshes presentation), and
    /api/updateProject replaces projectBoardConfig wholesale. Re-read the row
    so the link write extends the current links instead of persisting a
    snapshot taken before the session existed — otherwise a link that landed
    during the gap is dropped and its card reads as never worked.
    */
    try {
      const projects = await this.listGpuiProjectBoardDomainProjects();
      const latestBoardProject =
        projects.find((candidate) => candidate.projectId === boardProject.projectId) ??
        boardProject;
      return {
        boardProject: latestBoardProject,
        linkStoreProjects: selectBeadConversationLinkStoreProjects(latestBoardProject, projects),
      };
    } catch {
      return {
        boardProject,
        linkStoreProjects:
          fallbackLinkStoreProjects.length > 0 ? [...fallbackLinkStoreProjects] : [boardProject],
      };
    }
  }

  private async writeGpuiProjectBoardConversationLinks(
    boardProject: GxserverProjectDomainState,
    nextLinks: BeadConversationLink[],
  ): Promise<void> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    await this.client.rpc("/api/updateProject", {
      projectBoardConfig: {
        ...(boardProject.projectBoardConfig ?? {}),
        beadConversationLinks: nextLinks,
      },
      projectId: boardProject.projectId,
    });
  }

  private async upsertGpuiProjectBoardConversationLink(
    boardProject: GxserverProjectDomainState,
    initialLinkStoreProjects: readonly GxserverProjectDomainState[],
    args: {
      agent?: SidebarAgentButton;
      beadDisplayId?: string;
      beadId: string;
      session: GpuiCreatedProjectAgentSessionRecord;
    },
  ): Promise<void> {
    const now = new Date().toISOString();
    const presentationSession = this.presentation?.sessions.find(
      (session) =>
        session.projectId === args.session.projectId &&
        session.sessionId === args.session.sessionId,
    );
    const { boardProject: latestBoardProject, linkStoreProjects } =
      await this.reloadGpuiProjectBoardDomainScope(boardProject, initialLinkStoreProjects);
    // A shared board is read across every row that mounts it, so update the row
    // that already holds this conversation instead of adding a second copy.
    const boardSessionId =
      args.session.projectId === latestBoardProject.projectId
        ? args.session.sessionId
        : createGxserverPresentationProjectSessionId(
            args.session.projectId,
            args.session.sessionId,
          );
    const beadMatchKey = beadConversationLinkMatchKey(args.beadId);
    const storedLinks = this.readGpuiProjectBoardConversationLinks(linkStoreProjects);
    const linkProject =
      linkStoreProjects.find((storeProject) =>
        normalizeBeadConversationLinks(
          storeProject.projectBoardConfig?.beadConversationLinks,
          storeProject.projectId,
        ).some(
          (link) =>
            beadConversationLinkMatchKey(link.beadId) === beadMatchKey &&
            resolveBeadConversationLinkBoardSessionId(
              link,
              latestBoardProject.projectId,
              storedLinks,
            ) ===
              boardSessionId,
        ),
      ) ?? latestBoardProject;
    const ghostexSessionId =
      args.session.projectId === linkProject.projectId
        ? args.session.sessionId
        : createGxserverPresentationProjectSessionId(
            args.session.projectId,
            args.session.sessionId,
          );
    const nextLink: BeadConversationLink = {
      agentId: args.agent?.agentId ?? presentationSession?.agentId,
      agentName: args.agent?.name ?? presentationSession?.agentName,
      agentSessionId: args.session.agentSessionId ?? presentationSession?.agentSessionId,
      agentSessionPath: args.session.agentSessionPath ?? presentationSession?.agentSessionPath,
      beadDisplayId: args.beadDisplayId,
      beadId: args.beadId,
      createdAt: now,
      ghostexSessionId,
      id: createBeadConversationLinkId(linkProject.projectId, args.beadId, ghostexSessionId),
      projectId: linkProject.projectId,
      sessionPersistenceName: args.session.zmxName ?? presentationSession?.zmxName,
      sessionPersistenceProvider: "zmx",
      sessionProjectId: args.session.projectId,
      status: "active",
      updatedAt: now,
    };
    const currentLinks = normalizeBeadConversationLinks(
      linkProject.projectBoardConfig?.beadConversationLinks,
      linkProject.projectId,
    );
    const existingLink = currentLinks.find(
      (link) =>
        beadConversationLinkMatchKey(link.beadId) === beadMatchKey &&
        resolveBeadConversationLinkBoardSessionId(
          link,
          latestBoardProject.projectId,
          storedLinks,
        ) === boardSessionId,
    );
    const nextLinks = existingLink
      ? currentLinks.map((link) =>
          link.id === existingLink.id
            ? { ...link, ...nextLink, createdAt: link.createdAt }
            : link,
        )
      : [...currentLinks, nextLink];
    await this.writeGpuiProjectBoardConversationLinks(linkProject, nextLinks);
  }

  private async associateGpuiProjectBoardFocusedSession(request: {
    beadDisplayId?: string;
    beadId?: string;
    projectId?: string;
    projectPath?: string;
  }): Promise<void> {
    const beadId = request.beadId?.trim();
    if (!beadId) {
      throw new Error("No bead id is available.");
    }
    const { boardProject, linkStoreProjects } =
      await this.resolveGpuiProjectBoardDomainScope(request);
    const focusedOption = this.createGpuiProjectBoardSessionOptions(
      boardProject,
      linkStoreProjects,
    ).find((session) => session.isFocused);
    if (!focusedOption) {
      throw new Error("Focus an agent session before associating this bead.");
    }
    const reference = parseGxserverPresentationProjectSessionId(focusedOption.sessionId) ?? {
      projectId: boardProject.projectId,
      sessionId: focusedOption.sessionId,
    };
    await this.upsertGpuiProjectBoardConversationLink(
      boardProject,
      linkStoreProjects,
      {
        beadDisplayId: request.beadDisplayId?.trim() || undefined,
        beadId,
        session: {
          projectId: reference.projectId,
          sessionId: reference.sessionId,
        },
      },
    );
  }

  private async startGpuiProjectBoardWork(request: {
    agentId?: string;
    beadDisplayId?: string;
    beadId?: string;
    projectId?: string;
    projectPath?: string;
    prompt?: string;
    startLocation?: string;
  }): Promise<void> {
    const beadId = request.beadId?.trim();
    if (!beadId) {
      throw new Error("No bead id is available.");
    }
    const prompt = request.prompt?.trim();
    if (!prompt) {
      throw new Error("No bead prompt is available.");
    }
    const { boardProject, linkStoreProjects } =
      await this.resolveGpuiProjectBoardDomainScope(request);
    const agent = this.resolveDefaultPromptAgent(request.agentId);
    if (!agent?.command?.trim()) {
      throw new Error("Choose a configured agent before starting work.");
    }
    let session: GpuiCreatedProjectAgentSessionRecord;
    if (request.startLocation === "newWorktree") {
      session = await this.startGpuiProjectBoardWorktreeWork(boardProject, agent, prompt);
    } else {
      // macOS `handleProjectBoardStartWork` current-project path: focus the
      // board project, then launch the agent with the bead prompt staged as
      // the gxserver first user message (the created session is focused by
      // the create path itself).
      if (this.activeProjectId !== boardProject.projectId) {
        this.focusProjectId(boardProject.projectId);
      }
      session = await this.createAgentSessionRecordForProject(boardProject, agent, prompt, {
        errorMessage: "Could not create an agent session for this bead.",
      });
    }
    await this.upsertGpuiProjectBoardConversationLink(
      boardProject,
      linkStoreProjects,
      {
        agent,
        beadDisplayId: request.beadDisplayId?.trim() || undefined,
        beadId,
        session,
      },
    );
  }

  private async startGpuiProjectBoardWorktreeWork(
    boardProject: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string,
  ): Promise<GpuiCreatedProjectAgentSessionRecord> {
    /*
    macOS board "New worktree" starts ride `createNativeWorktreeForAgentPrompt`
    with baseBranch HEAD and a "Worktree started" toast; GPUI reuses the
    reviewed worktree-modal creation path (unique target, git worktree add,
    project registration, beads hooks, setup command, prompt-staged agent
    session) with the same toast lifecycle.
    */
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast("info", "Creating worktree", {
      persistent: true,
      toastId,
    });
    try {
      const created = await this.createNewProjectWorktree(
        {
          agentId: agent.agentId,
          baseBranch: "HEAD",
          mode: "create",
          projectId: boardProject.projectId,
          prompt,
          type: "createProjectWorktree",
        },
        boardProject,
      );
      this.trustedExistingWorktreeList = undefined;
      await this.refreshDomainPresentationFromClient("patch").catch(() => undefined);
      this.postWorktreeToast("success", "Worktree started", { toastId });
      return created.session;
    } catch (error) {
      this.postWorktreeToast("error", "Could not create worktree", {
        description: gpuiWorktreeUserVisibleErrorMessage(error),
        toastId,
      });
      throw error;
    }
  }

  private async jumpToGpuiProjectBoardConversation(request: {
    beadId?: string;
    projectId?: string;
    projectPath?: string;
    sessionId?: string;
  }): Promise<void> {
    const sessionId = request.sessionId?.trim();
    if (!sessionId) {
      throw new Error("No linked conversation is selected.");
    }
    const { boardProject, linkStoreProjects } =
      await this.resolveGpuiProjectBoardDomainScope(request);
    const reference = parseGxserverPresentationProjectSessionId(sessionId) ?? {
      projectId: boardProject.projectId,
      sessionId,
    };
    const live = this.presentation?.sessions.some(
      (session) =>
        session.projectId === reference.projectId && session.sessionId === reference.sessionId,
    );
    if (live) {
      await this.focusSession(
        createGxserverPresentationProjectSessionId(reference.projectId, reference.sessionId),
      );
      return;
    }
    /*
    macOS restores dead links through the previous-sessions owner and rewrites
    the link to the restored session; GPUI uses the same daemon restore
    contract as the Previous Sessions modal (`createSession` with
    `restoredFromSessionId`, then remove the stopped history row).
    */
    const row = await this.findGpuiProjectBoardPreviousSessionRow(reference);
    if (!row) {
      await this.resumeGpuiProjectBoardConversation({
        beadId: request.beadId?.trim() || undefined,
        boardProject,
        linkStoreProjects,
        oldGhostexSessionId: sessionId,
        reference,
      });
      return;
    }
    if (!this.client) {
      throw new Error("The linked Ghostex session is no longer available.");
    }
    const created = await this.client.rpc<{
      session?: { projectId?: string; sessionId?: string; zmxName?: string };
    }>("/api/createSession", {
      kind: "terminal",
      lifecycleState: "running",
      projectId: reference.projectId,
      restoredFromSessionId: reference.sessionId,
      ...(row.sessionTag ? { sessionTag: row.sessionTag } : {}),
      ...(row.sidebarOrder !== undefined ? { sidebarOrder: row.sidebarOrder } : {}),
      surface: "workspace",
      title: gpuiProjectBoardPreviousSessionRowTitle(row),
    });
    const restoredSessionId = normalizeNonEmptyString(created.session?.sessionId);
    if (!restoredSessionId) {
      throw new Error("The linked Ghostex session could not be restored.");
    }
    const restoredProjectId =
      normalizeNonEmptyString(created.session?.projectId) ?? reference.projectId;
    await this.client
      .rpc("/api/removeSession", {
        projectId: reference.projectId,
        reason: "projectBoardJumpToConversationRestore",
        sessionId: reference.sessionId,
      })
      .catch(() => undefined);
    this.projectBoardRestorableLinkChecks.delete(`${reference.projectId}:${reference.sessionId}`);
    await this.replaceGpuiProjectBoardConversationLinkSession(boardProject, linkStoreProjects, {
      beadId: request.beadId?.trim() || undefined,
      oldGhostexSessionId: sessionId,
      restoredProjectId,
      restoredSessionId,
      restoredSessionPersistenceName: normalizeNonEmptyString(created.session?.zmxName),
    });
    this.focusLocalWorkspaceSession(restoredProjectId, restoredSessionId);
  }

  private async resumeGpuiProjectBoardConversation(args: {
    beadId?: string;
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: readonly GxserverProjectDomainState[];
    oldGhostexSessionId: string;
    reference: { projectId: string; sessionId: string };
  }): Promise<void> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    A bead's session usually closes without leaving a restorable history row,
    but the agent conversation it worked is still resumable from the session
    row's agent identity. `/api/forkSession` is the daemon-owned path for that:
    it plans the resume command in gxserver, starts the provider, and hands
    back a live session, which the bead then follows through the same link
    replacement the restore path uses.
    */
    if (!this.client) {
      throw new Error("The linked Ghostex session is no longer available.");
    }
    const { fork } = await this.client.rpc<{ fork?: GxserverForkSessionResult }>(
      "/api/forkSession",
      {
        projectId: args.reference.projectId,
        reason: "projectBoardResumeConversation",
        sessionId: args.reference.sessionId,
      },
    );
    const resumedSessionId = normalizeNonEmptyString(fork?.session.sessionId);
    if (!resumedSessionId) {
      throw new Error("The linked conversation could not be resumed.");
    }
    const resumedProjectId =
      normalizeNonEmptyString(fork?.session.projectId) ?? args.reference.projectId;
    this.projectBoardRestorableLinkChecks.delete(
      `${args.reference.projectId}:${args.reference.sessionId}`,
    );
    await this.replaceGpuiProjectBoardConversationLinkSession(
      args.boardProject,
      args.linkStoreProjects,
      {
        beadId: args.beadId,
        oldGhostexSessionId: args.oldGhostexSessionId,
        restoredProjectId: resumedProjectId,
        restoredSessionId: resumedSessionId,
        restoredSessionPersistenceName: normalizeNonEmptyString(fork?.session.zmxName),
      },
    );
    this.focusLocalWorkspaceSession(resumedProjectId, resumedSessionId);
  }

  private async replaceGpuiProjectBoardConversationLinkSession(
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[],
    args: {
      beadId?: string;
      oldGhostexSessionId: string;
      restoredProjectId: string;
      restoredSessionId: string;
      restoredSessionPersistenceName?: string;
    },
  ): Promise<void> {
    // macOS `replaceProjectBoardConversationLinkSession`: every link on the
    // old session id moves to the restored one (scoped to one bead when the
    // jump carried a bead id), collapsing any pre-existing duplicate link.
    const now = new Date().toISOString();
    const ghostexSessionId =
      args.restoredProjectId === boardProject.projectId
        ? args.restoredSessionId
        : createGxserverPresentationProjectSessionId(
            args.restoredProjectId,
            args.restoredSessionId,
          );
    const storedLinks = this.readGpuiProjectBoardConversationLinks(linkStoreProjects);
    const beadMatchKey = args.beadId
      ? beadConversationLinkMatchKey(args.beadId)
      : undefined;
    await this.mutateGpuiProjectBoardConversationLinkStores(
      boardProject,
      linkStoreProjects,
      (currentLinks, storeProject) => {
        return currentLinks.flatMap((link) => {
          const linkBeadMatches =
            !beadMatchKey || beadConversationLinkMatchKey(link.beadId) === beadMatchKey;
          const boardSessionId = resolveBeadConversationLinkBoardSessionId(
            link,
            boardProject.projectId,
            storedLinks,
          );
          const isTarget =
            boardSessionId === args.oldGhostexSessionId && linkBeadMatches;
          const isDuplicateForTarget =
            Boolean(beadMatchKey) && linkBeadMatches && boardSessionId === ghostexSessionId;
          if (!isTarget) {
            return isDuplicateForTarget ? [] : [link];
          }
          return [
            {
              ...link,
              ghostexSessionId,
              id: createBeadConversationLinkId(
                storeProject.projectId,
                link.beadId,
                ghostexSessionId,
              ),
              // The stored provider name describes the session being replaced,
              // so it is re-stated from the new session rather than left to
              // describe a session this link no longer points at.
              sessionPersistenceName: args.restoredSessionPersistenceName,
              sessionProjectId: args.restoredProjectId,
              updatedAt: now,
            },
          ];
        });
      },
    );
  }

  private async unlinkGpuiProjectBoardConversation(request: {
    beadId?: string;
    projectId?: string;
    projectPath?: string;
    sessionId?: string;
  }): Promise<void> {
    const beadId = request.beadId?.trim();
    if (!beadId) {
      throw new Error("No bead id is available.");
    }
    const sessionId = request.sessionId?.trim();
    if (!sessionId) {
      throw new Error("No linked conversation is selected.");
    }
    const { boardProject, linkStoreProjects } =
      await this.resolveGpuiProjectBoardDomainScope(request);
    const now = new Date().toISOString();
    const beadMatchKey = beadConversationLinkMatchKey(beadId);
    const storedLinks = this.readGpuiProjectBoardConversationLinks(linkStoreProjects);
    await this.mutateGpuiProjectBoardConversationLinkStores(
      boardProject,
      linkStoreProjects,
      (currentLinks) =>
        currentLinks.map((link) =>
          beadConversationLinkMatchKey(link.beadId) === beadMatchKey &&
          resolveBeadConversationLinkBoardSessionId(
            link,
            boardProject.projectId,
            storedLinks,
          ) === sessionId
            ? { ...link, status: "archived" as const, updatedAt: now }
            : link,
        ),
    );
  }

  private async mutateGpuiProjectBoardConversationLinkStores(
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[],
    mutate: (
      currentLinks: BeadConversationLink[],
      storeProject: GxserverProjectDomainState,
    ) => BeadConversationLink[],
  ): Promise<void> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    The board reads links from every project row that mounts the same Beads
    board, so a link the user acts on can be stored on a row other than the one
    whose board is open. Apply link mutations to each row that actually holds a
    matching link; a row whose links come back unchanged is never written.
    */
    const projects = linkStoreProjects.length > 0 ? linkStoreProjects : [boardProject];
    for (const storeProject of projects) {
      const currentLinks = normalizeBeadConversationLinks(
        storeProject.projectBoardConfig?.beadConversationLinks,
        storeProject.projectId,
      );
      const nextLinks = mutate(currentLinks, storeProject);
      if (JSON.stringify(nextLinks) === JSON.stringify(currentLinks)) {
        continue;
      }
      await this.writeGpuiProjectBoardConversationLinks(storeProject, nextLinks);
    }
  }

  private handleGpuiWorkspaceTabSessionSelected(payload: unknown): void {
    const selection = normalizeGpuiWorkspaceTabSessionSelection(payload);
    if (!selection) {
      return;
    }
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-08:01:
    A GPUI workspace tab click has already selected the native tab in Rust. Match macOS `paneTabSelected` by updating the sidebar's local presentation focus and publishing only the sidebar patch; do not post `workspaceTerminalFocus` back to Rust or call gxserver `/api/focusSession`.

    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-00:33:
    MacOS reconciles stale native sleeping pane tabs when gxserver presentation already reports the canonical P/G session running. Preserve the one-way tab-selection path for ordinary clicks, but if Rust marks the selected mapped tab as locally sleeping and the current presentation row is running, post one bounded WorkspaceTerminalFocus so Rust reuses and attaches that existing tab instead of leaving an inert sleeping placeholder.

    CDXC:GPUIWorkspaceSessionFocus 2026-07-11:
    A restored-after-restart Running tab can carry no local terminal runtime at all (no live owner, parked owner, or pending attach payload); Rust reports that as `localRuntimeMissing`. Reconcile it exactly like the stale sleeping case: when gxserver presentation still reports the canonical P/G session running, post one bounded WorkspaceTerminalFocus so Rust materializes the tab through the ordinary gxserver attach pipeline instead of leaving an empty body behind the selected tab.
    */
    const shouldReconcileRunningPresentation =
      (selection.localWasSleeping === true || selection.localRuntimeMissing === true) &&
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === selection.projectId &&
          session.sessionId === selection.sessionId &&
          session.lifecycleState === "running",
      ) === true;
    this.setLocalPresentationSessionFocus(
      selection.projectId,
      selection.sessionId,
      undefined,
      selection.visibleSessionIds,
    );
    if (shouldReconcileRunningPresentation) {
      this.postLocalWorkspaceTerminalFocus(selection.projectId, selection.sessionId);
    }
    this.publishPresentation("patch");
  }

  private handleGpuiWorkspaceSessionAttentionAcknowledge(payload: unknown): void {
    const acknowledgement = normalizeGpuiWorkspaceSessionAttentionAcknowledge(payload);
    if (!acknowledgement) {
      return;
    }
    this.acknowledgeSessionAttention(
      createGxserverPresentationProjectSessionId(
        acknowledgement.projectId,
        acknowledgement.sessionId,
      ),
      "native-focus",
    );
  }

  private handleGpuiWorkspaceTerminalEscapePressed(payload: unknown): void {
    const escape = normalizeGpuiWorkspaceTerminalEscapePressed(payload);
    if (!escape) {
      return;
    }
    const target: GpuiSessionAttentionTarget = {
      kind: "local",
      projectId: escape.projectId,
      sessionId: escape.sessionId,
    };
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    this.suppressAttentionCompletionSoundAfterTerminalEscape(sessionKey);
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity === "attention") {
      const didChange = this.clearPresentationSessionAttentionLocally(target, "terminal-escape");
      if (didChange) {
        this.publishPresentation("patch");
      }
    }
    void this.syncLocalSessionTerminalEscapeWithGxserver(
      escape.projectId,
      escape.sessionId,
      normalizeNonEmptyString(session?.agentName),
    );
  }

  private async handleGpuiWorkspaceFirstPromptTitleGenerationCancel(
    payload: unknown,
  ): Promise<void> {
    /*
    CDXC:GPUISessionTitleOverlay 2026-07-26:
    Escape inside the blocking "Generating title" pane overlay cancels the
    gxserver-owned first-prompt title job, matching the managed macOS pane.
    Rust only reports the suppressed-pane Escape; the sidebar runtime owns the
    decision and the gxserver call. Clear the local presentation flag first so
    the overlay and terminal input suppression lift immediately instead of
    waiting for the next gxserver delta.
    */
    const cancel = normalizeGpuiWorkspaceFirstPromptTitleGenerationCancel(payload);
    if (!cancel || !this.client) {
      return;
    }
    const session = this.findLocalPresentationSession(cancel.projectId, cancel.sessionId);
    if (session?.isGeneratingFirstPromptTitle !== true) {
      return;
    }
    if (
      this.clearLocalPresentationSessionFirstPromptTitleGeneration(
        cancel.projectId,
        cancel.sessionId,
      )
    ) {
      this.publishPresentation("patch");
    }
    try {
      await this.client.rpc("/api/cancelFirstPromptAutoTitle", {
        projectId: cancel.projectId,
        reason: "escape",
        sessionId: cancel.sessionId,
      });
    } catch {
      /*
      gxserver owns the title job, so a rejected cancel is recovered by the
      next presentation delta: it republishes the generating flag and the
      overlay comes back if the job is still alive.
      */
    }
  }

  private clearLocalPresentationSessionFirstPromptTitleGeneration(
    projectId: string,
    sessionId: string,
  ): boolean {
    const presentation = this.presentation;
    if (!presentation) {
      return false;
    }
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (
        session.projectId !== projectId ||
        session.sessionId !== sessionId ||
        session.isGeneratingFirstPromptTitle !== true
      ) {
        return session;
      }
      didChange = true;
      return {
        ...session,
        isGeneratingFirstPromptTitle: false,
      };
    });
    if (!didChange) {
      return false;
    }
    this.presentation = {
      ...presentation,
      sessions,
    };
    return true;
  }

  private async handleGpuiWorkspaceTerminalBell(payload: unknown): Promise<void> {
    /*
    Shells use BEL for routine feedback such as zsh Tab-completion misses, so
    the bell only becomes gxserver attention when the user opts in from
    Terminal settings — the same gate macOS applies to its terminalBell host
    event. Agent completion keeps its separate explicit attention path.
    */
    const bell = normalizeGpuiWorkspaceTerminalBell(payload);
    if (!bell || !this.client) {
      return;
    }
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    if (!settings.showNotificationOnTerminalBell) {
      return;
    }
    const agentName = normalizeNonEmptyString(
      this.presentation?.sessions.find(
        (session) => session.projectId === bell.projectId && session.sessionId === bell.sessionId,
      )?.agentName,
    );
    try {
      await this.client.rpc("/api/updateAgentActivity", {
        ...(agentName ? { agentName } : {}),
        event: "bell",
        projectId: bell.projectId,
        sessionId: bell.sessionId,
      });
    } catch {
      // gxserver attention sync is best-effort, matching macOS's log-only failure path.
    }
  }

  private async handleGpuiWorkspaceTerminalRuntimeAction(payload: unknown): Promise<void> {
    /*
    Rust-origin Fork/Reload for focused Agents terminals reuse the exact card
    action paths so gxserver ownership, focus follow-up, and reload semantics
    stay identical to sidebar-driven Fork/Full Reload.
    */
    const request = normalizeGpuiWorkspaceTerminalRuntimeAction(payload);
    if (!request) {
      return;
    }
    if (request.action === "sleepInactiveSessions") {
      await this.sleepInactiveSessionsFromTitlebar();
      return;
    }
    if (request.action === "sleepAllDaemonSessions") {
      await this.sleepAllLocalDaemonSessions();
      return;
    }
    const sessionId = createGxserverPresentationProjectSessionId(
      request.projectId,
      request.sessionId,
    );
    if (request.action === "forkSession") {
      await this.forkSession(sessionId);
      return;
    }
    await this.fullReloadSession(sessionId);
  }

  private async sleepInactiveSessionsFromTitlebar(): Promise<void> {
    /*
    macOS's titlebar Resources shortcut revalidates and sleeps every inactive
    awake terminal. GPUI derives the same set from the shared inactive-session
    filter used by per-project bulk sleep, across the local daemon and every
    connected remote presentation.
    */
    const sessionIds: string[] = [];
    for (const tab of this.browserTabs) {
      if (!tab.isSleeping && !tab.isVisible) {
        sessionIds.push(gpuiBrowserSidebarSessionId(tab));
      }
    }
    for (const session of this.presentation?.sessions ?? []) {
      if (isGpuiInactiveProjectPresentationSession(session)) {
        sessionIds.push(
          createGxserverPresentationProjectSessionId(session.projectId, session.sessionId),
        );
      }
    }
    for (const [machineId, presentation] of this.remotePresentations) {
      for (const session of presentation.sessions ?? []) {
        if (isGpuiInactiveProjectPresentationSession(session)) {
          sessionIds.push(
            createGpuiRemotePresentationSessionId(machineId, session.projectId, session.sessionId),
          );
        }
      }
    }
    if (sessionIds.length === 0) {
      return;
    }
    await this.setSessionsSleeping(sessionIds, true);
  }

  /*
  macOS killTerminalDaemon parity: since the gxserver cutover the Running
  Sessions daemon-stop control is a local-first bulk sleep — macOS routes
  every awake gxserver-presented terminal through the shared sleep path and
  leaves the shared daemon process running. GPUI sleeps every non-sleeping
  local daemon session the same way; remote presentations are untouched
  because the modal lists local daemon state.
  */
  private async sleepAllLocalDaemonSessions(): Promise<void> {
    const sessionIds = this.browserTabs
      .filter((tab) => !tab.isSleeping)
      .map(gpuiBrowserSidebarSessionId);
    for (const session of this.presentation?.sessions ?? []) {
      if (session.lifecycleState !== "sleeping") {
        sessionIds.push(
          createGxserverPresentationProjectSessionId(session.projectId, session.sessionId),
        );
      }
    }
    if (sessionIds.length === 0) {
      return;
    }
    await this.setSessionsSleeping(sessionIds, true);
  }

  private applyGxserverBootstrapChanged(bootstrap: GpuiGxserverBootstrap): void {
    const validated = validateGpuiGxserverBootstrap(bootstrap);
    if (!validated) {
      this.startFromBootstrap(bootstrap);
      return;
    }
    if (
      !this.gxserverBootstrap ||
      !hasSameGpuiGxserverBootstrapTransport(this.gxserverBootstrap, validated) ||
      !this.presentation
    ) {
      this.startFromBootstrap(bootstrap);
      return;
    }
    /*
    CDXC:GPUISidebarBootstrapReplay 2026-06-26-05:31:
    Post-start same-transport bootstrap refreshes are Rust's replay channel for the sidebar bridge, not a new macOS-style focus command. Store the refreshed transport/focus hint snapshot but do not reapply `initialActiveProjectId`, focused session, or visible ids over live React focus; otherwise the active project can bounce between stale and current sidebar snapshots after a local click.
    */
    this.gxserverBootstrap = validated;
  }

  private async handleNativeAppShotCaptured(payload: unknown): Promise<void> {
    const appShot = normalizeGpuiNativeAppShotCapture(payload);
    if (!appShot) {
      this.postAppShotToast("warning", "App Shot Failed", {
        description: "Could not read the native App Shot.",
      });
      return;
    }

    const prompt = formatGpuiNativeAppShotPrompt(
      appShot,
      createGpuiSidebarSettings(this.runtimeSettings).appShotsMetadataEnabled,
    );
    const staged = await this.stageNativeAppShotInAgentSession(prompt);
    if (!staged.ok) {
      this.postAppShotToast("warning", "App Shot Failed", {
        description: staged.description,
      });
      return;
    }

    this.postAppShotToast("success", "App Shot Added", {
      description: appShot.appName,
    });
  }

  private async stageNativeAppShotInAgentSession(
    prompt: string,
  ): Promise<{ ok: true } | { description: string; ok: false }> {
    /*
    CDXC:GPUIAppShots 2026-06-25-23:28:
    GPUI App Shots mirror macOS target order for local sessions: reuse the last successful local App Shot target for 60 seconds when it is still a live local agent row, otherwise use the focused/visible local agent row, and create a default prompt-agent session only when the exact local insert bridge declines. Keep command-pane, sleeping, stale, non-agent, and sidebar-only rows out of insertion.

    CDXC:GPUIAppShots 2026-06-26-04:27:
    Existing-session App Shot targeting now accepts live remote agent rows by their machine-scoped presentation session id, but only as an insertion request to Rust. React must not wake, materialize, or open remote attach tabs for App Shots; Rust may write only when that exact remote attach surface is already mounted.
    */
    const targetSession = this.resolveNativeAppShotTargetSession();
    if (
      targetSession &&
      (await this.stageNativeAppShotInExistingAgentSession(targetSession, prompt))
    ) {
      return { ok: true };
    }

    if (!this.client) {
      return {
        description: "The local agent service is not ready.",
        ok: false,
      };
    }
    const project = this.activeDomainProject();
    if (!project) {
      return {
        description: "Open a project before using App Shots.",
        ok: false,
      };
    }
    const agent = this.resolveDefaultPromptAgent();
    if (!agent?.command?.trim()) {
      return {
        description: "Choose a configured default prompt agent before using App Shots.",
        ok: false,
      };
    }

    try {
      const sessionId = await this.createAgentSessionForProject(project, agent, prompt);
      this.rememberNativeAppShotTargetSessionId(sessionId);
      return { ok: true };
    } catch {
      return {
        description: "Could not stage the App Shot in an agent session.",
        ok: false,
      };
    }
  }

  private async stageNativeAppShotInExistingAgentSession(
    session: SidebarSessionItem,
    prompt: string,
  ): Promise<boolean> {
    const sessionId = nativeAppShotPromptSessionIdForSidebarSession(session);
    if (!sessionId) {
      return false;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.setRemotePresentationSessionFocus(remoteSession);
    } else {
      const projectId = localGxserverProjectIdForSidebarSession(session, this.presentation);
      if (projectId) {
        this.focusLocalWorkspaceSession(projectId, sessionId);
      } else {
        this.focusedSessionId = sessionId;
        this.visibleSessionIds = new Set([sessionId]);
        this.postGxserverPresentationFocusState();
      }
    }
    const inserted = await this.postNativeAppShotPromptToSession(sessionId, prompt);
    if (inserted) {
      this.rememberNativeAppShotTargetSessionId(sessionId);
    }
    return inserted;
  }

  private resolveNativeAppShotTargetSession(): SidebarSessionItem | undefined {
    const now = Date.now();
    const recentTarget =
      this.lastAppShotTargetSessionId && now - this.lastAppShotTargetAt <= APP_SHOT_RECENT_TARGET_MS
        ? this.findNativeAppShotSessionByPresentationSessionId(this.lastAppShotTargetSessionId)
        : undefined;
    if (isNativeAppShotAgentSession(recentTarget)) {
      return recentTarget;
    }

    const focusedSession = this.focusedSessionId
      ? this.findNativeAppShotSessionByPresentationSessionId(this.focusedSessionId)
      : undefined;
    if (isNativeAppShotAgentSession(focusedSession)) {
      return focusedSession;
    }

    for (const sessionId of this.visibleSessionIds) {
      const visibleSession = this.findNativeAppShotSessionByPresentationSessionId(sessionId);
      if (visibleSession?.isVisible && isNativeAppShotAgentSession(visibleSession)) {
        return visibleSession;
      }
    }
    return undefined;
  }

  private findNativeAppShotSessionByPresentationSessionId(
    sessionId: string,
  ): SidebarSessionItem | undefined {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId) {
      return undefined;
    }
    if (parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
      return this.findNativeAppShotSessionByRemotePresentationSessionId(normalizedSessionId);
    }
    return this.findNativeAppShotSessionByLocalGxserverSessionId(normalizedSessionId);
  }

  private findNativeAppShotSessionByLocalGxserverSessionId(
    sessionId: string,
  ): SidebarSessionItem | undefined {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId || parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
      return undefined;
    }
    for (const group of this.latestGroups) {
      if (group.remoteMachineContext) {
        continue;
      }
      const session = group.sessions.find(
        (candidate) => localGxserverSessionIdForSidebarSession(candidate) === normalizedSessionId,
      );
      if (session) {
        return session;
      }
    }
    return undefined;
  }

  private findNativeAppShotSessionByRemotePresentationSessionId(
    sessionId: string,
  ): SidebarSessionItem | undefined {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId || !parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
      return undefined;
    }
    for (const group of this.latestGroups) {
      if (!group.remoteMachineContext) {
        continue;
      }
      const session = group.sessions.find(
        (candidate) => candidate.sessionId === normalizedSessionId,
      );
      if (session) {
        return session;
      }
    }
    return undefined;
  }

  private async postNativeAppShotPromptToSession(
    sessionId: string,
    prompt: string,
  ): Promise<boolean> {
    const postPrompt = window.ghostexGpui?.postNativeAppShotPromptToSession;
    if (typeof postPrompt !== "function") {
      return false;
    }
    const payload = JSON.stringify({
      prompt,
      sessionId,
      type: GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION,
    });

    return await new Promise<boolean>((resolve) => {
      const pending: GpuiPendingNativeAppShotPromptInsertion = {
        resolve,
        sessionId,
        timeoutId: 0,
      };
      pending.timeoutId = window.setTimeout(() => {
        this.resolvePendingNativeAppShotPromptInsertion(pending, false);
      }, APP_SHOT_PROMPT_INSERT_RESULT_TIMEOUT_MS);
      this.pendingNativeAppShotPromptInsertions.push(pending);
      let sent = false;
      try {
        sent = postPrompt(payload) === true;
      } catch {
        sent = false;
      }
      if (!sent) {
        this.resolvePendingNativeAppShotPromptInsertion(pending, false);
      }
    });
  }

  private handleNativeAppShotPromptResult(payload: unknown): void {
    const result = normalizeGpuiNativeAppShotPromptResult(payload);
    if (!result) {
      return;
    }
    const pending = this.pendingNativeAppShotPromptInsertions.find(
      (candidate) => candidate.sessionId === result.sessionId,
    );
    if (!pending) {
      return;
    }
    this.resolvePendingNativeAppShotPromptInsertion(pending, result.ok);
  }

  private resolvePendingNativeAppShotPromptInsertion(
    pending: GpuiPendingNativeAppShotPromptInsertion,
    ok: boolean,
  ): void {
    const index = this.pendingNativeAppShotPromptInsertions.indexOf(pending);
    if (index >= 0) {
      this.pendingNativeAppShotPromptInsertions.splice(index, 1);
    }
    window.clearTimeout(pending.timeoutId);
    pending.resolve(ok);
  }

  private rememberNativeAppShotTargetSessionId(sessionId: string): void {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId) {
      return;
    }
    this.lastAppShotTargetSessionId = normalizedSessionId;
    this.lastAppShotTargetAt = Date.now();
  }

  private readonly handleGpuiSidebarRemoteEvent = (event: Event): void => {
    const remoteEvent = normalizeGpuiSidebarRemoteEvent((event as CustomEvent<unknown>).detail);
    if (!remoteEvent) {
      return;
    }
    if (remoteEvent.type === "remoteMachineStatus") {
      this.messageSource.postMessage(remoteEvent);
      if (remoteEvent.state === "connected") {
        this.finishRemoteStartupReconnects(remoteEvent.machineId);
      } else if (GPUI_REMOTE_MACHINE_STARTUP_RETRY_STATES.has(remoteEvent.state)) {
        this.scheduleRemoteStartupReconnect(remoteEvent.machineId);
      } else {
        this.clearRemoteStartupReconnectTimeout(remoteEvent.machineId);
      }
      if (remoteEvent.state === "connected") {
        this.clearRemotePresentationRecoveryTimeout(remoteEvent.machineId);
      }
      if (remoteEvent.state === "presentationStreamFailed") {
        this.scheduleRemoteGxserverPresentationRecovery(remoteEvent.machineId);
        return;
      }
      if (GPUI_REMOTE_MACHINE_PRESENTATION_CLEAR_STATES.has(remoteEvent.state)) {
        this.clearRemotePresentationRecoveryTimeout(remoteEvent.machineId);
        const previousPresentation = this.remotePresentations.get(remoteEvent.machineId);
        if (previousPresentation) {
          this.syncRemotePresentationAttentionTracking(
            remoteEvent.machineId,
            previousPresentation.sessions,
            [],
          );
        }
        this.remotePresentations.delete(remoteEvent.machineId);
        this.dropRemotePresentationSessionFocus(remoteEvent.machineId);
        this.publishRemotePresentationPatch();
      }
      return;
    }

    if (remoteEvent.type === "remoteGxserverResponse") {
      this.resolveRemoteGxserverRequest(remoteEvent);
      return;
    }

    if (remoteEvent.payload.type === "presentationSnapshot") {
      const previousSessions =
        this.remotePresentations.get(remoteEvent.remoteMachineId)?.sessions ?? [];
      const snapshot = this.projectRemotePresentationAttentionAcknowledgementGuards(
        remoteEvent.remoteMachineId,
        remoteEvent.payload.snapshot,
      );
      const previous = this.remotePresentations.get(remoteEvent.remoteMachineId);
      if (previous && previous.revision > snapshot.revision) {
        return;
      }
      this.remotePresentations.set(remoteEvent.remoteMachineId, snapshot);
      this.pruneRemoteWorkspaceGroupAssignments(remoteEvent.remoteMachineId, snapshot);
      this.syncRemotePresentationAttentionTracking(
        remoteEvent.remoteMachineId,
        previousSessions,
        snapshot.sessions,
      );
      this.publishRemotePresentationPatch();
      return;
    }

    const previous = this.remotePresentations.get(remoteEvent.remoteMachineId);
    if (!previous) {
      void this.refreshRemotePresentationFromGxserver(remoteEvent.remoteMachineId).catch(
        () => undefined,
      );
      return;
    }
    if (remoteEvent.payload.revision <= previous.revision) {
      void this.refreshRemotePresentationFromGxserver(remoteEvent.remoteMachineId).catch(
        () => undefined,
      );
      return;
    }
    const snapshot = this.projectRemotePresentationAttentionAcknowledgementGuards(
      remoteEvent.remoteMachineId,
      reduceGxserverPresentationDelta(
        previous,
        remoteEvent.payload.delta,
        remoteEvent.payload.revision,
      ),
    );
    this.remotePresentations.set(remoteEvent.remoteMachineId, snapshot);
    this.pruneRemoteWorkspaceGroupAssignments(remoteEvent.remoteMachineId, snapshot);
    this.syncRemotePresentationAttentionTracking(
      remoteEvent.remoteMachineId,
      previous.sessions,
      snapshot.sessions,
    );
    this.publishRemotePresentationPatch();
  };

  private tryStartFromInstalledBootstrap(attempt: number): void {
    const bootstrap = window.ghostexGpui?.gxserverBootstrap;
    if (bootstrap) {
      this.startFromBootstrap(bootstrap);
      return;
    }
    if (attempt >= GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS) {
      return;
    }
    this.bootstrapPollTimeoutId = window.setTimeout(() => {
      this.tryStartFromInstalledBootstrap(attempt + 1);
    }, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS);
  }

  private startFromBootstrap(bootstrap: GpuiGxserverBootstrap): void {
    if (this.bootstrapPollTimeoutId !== undefined) {
      window.clearTimeout(this.bootstrapPollTimeoutId);
      this.bootstrapPollTimeoutId = undefined;
    }

    const validated = validateGpuiGxserverBootstrap(bootstrap);
    if (!validated) {
      this.publishUnavailable("bootstrap-invalid");
      return;
    }

    this.subscription?.close();
    this.gxserverBootstrap = validated;
    this.client = new GpuiGxserverClient(validated);
    this.applyGxserverBootstrapPresentationState(validated);

    const client = this.client;
    void Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchAppUserData(),
      client.fetchProjectList().catch(() => undefined),
      client.fetchRecentProjects().catch(() => undefined),
      client.fetchSidebarHud(validated.initialActiveProjectId),
      client.fetchWorkspaceSessionGroups().catch(() => undefined),
    ])
      .then(
        ([snapshot, appUserData, domainProjects, recentProjects, sidebarHud, workspaceGroups]) => {
          if (this.client !== client) {
            return;
          }
          this.appUserData = appUserData;
          this.domainProjects = domainProjects ? [...domainProjects] : [];
          this.recentProjects = recentProjects ? [...recentProjects] : [];
          this.sidebarHud = sidebarHud;
          this.adoptWorkspaceGroupsFromGxserver(workspaceGroups);
          this.applyPresentationSnapshot(snapshot, "hydrate");
          this.openPresentationSubscription(validated.clientId, snapshot.revision);
        },
      )
      .catch(() => {
        this.publishUnavailable("snapshot-failed");
      });
  }

  private applyGxserverBootstrapPresentationState(
    bootstrap: GpuiValidatedGxserverBootstrap,
  ): boolean {
    const nextFocusedSessionId = bootstrap.focusedSessionId;
    const nextVisibleSessionIds = new Set(bootstrap.visibleSessionIds ?? []);
    /*
    CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
    A bootstrap can replay a machine-scoped remote project id after a remote
    session owned focus at shutdown. `this.activeProjectId` is a local-only
    gxserver key (HUD fetches, domain-project lookups), so the scoped id may
    only select the remote group; it must never become the local active
    project id.
    */
    const nextActiveProjectId =
      bootstrap.initialActiveProjectId &&
      parseGpuiRemotePresentationProjectId(bootstrap.initialActiveProjectId)
        ? this.activeProjectId
        : bootstrap.initialActiveProjectId;
    const nextActiveGroupId = activeGroupIdForGpuiGxserverBootstrapPresentationState({
      focusedSessionId: nextFocusedSessionId,
      initialActiveProjectId: bootstrap.initialActiveProjectId,
    });
    const didChange =
      this.activeProjectId !== nextActiveProjectId ||
      this.activeGroupId !== nextActiveGroupId ||
      this.focusedSessionId !== nextFocusedSessionId ||
      !sameStringSet(this.visibleSessionIds, nextVisibleSessionIds);
    this.activeProjectId = nextActiveProjectId;
    this.activeGroupId = nextActiveGroupId;
    this.focusedSessionId = nextFocusedSessionId;
    this.visibleSessionIds = nextVisibleSessionIds;
    return didChange;
  }

  private openPresentationSubscription(clientId: string, lastRevision: number): void {
    if (!this.client) {
      return;
    }
    this.subscription = this.client.subscribePresentation({
      clientId,
      lastRevision,
      onClose: () => {
        this.recoverPresentationStream(clientId);
      },
      onDelta: (delta, revision) => {
        this.applyPresentationDelta(delta, revision);
      },
      onError: () => {
        this.recoverPresentationStream(clientId);
      },
      /*
      CDXC:GlobalActions 2026-08-07:
      Global Action writes reach this surface only as this announcement. They
      are not project writes, so they produce no projectUpdated delta, and the
      Settings window that made the write is a different surface whose response
      never lands here. Refetch the HUD the same way a project Action edit
      already does, so a Global Action flagged for the project row appears and
      disappears with the toggle instead of on the next unrelated delta.
      */
      onGlobalSidebarCommands: () => {
        this.refreshSidebarHudFromClient();
      },
      onRendererCommand: (command) => this.handleGxserverRendererCommand(command),
      onSidebarProjectCollections: (state) => {
        this.forwardSidebarProjectCollectionsFromGxserver(state);
      },
      onSnapshot: (snapshot) => {
        this.applyPresentationSnapshot(snapshot, this.hasHydrated ? "patch" : "hydrate");
      },
    });
  }

  private async handleGxserverRendererCommand(
    command: GxserverRendererCommand,
  ): Promise<Record<string, unknown>> {
    switch (command.action) {
      case "focusSession": {
        const resolvedSession = this.resolveGxserverRendererCommandSession(command.payload);
        if (!resolvedSession) {
          throw new Error("No matching session was found.");
        }
        await this.focusSession(resolvedSession.sidebarSessionId, {
          sessionId: resolvedSession.sidebarSessionId,
          type: "focusSession",
        });
        return {
          ok: true,
          session: {
            ghostexId: resolvedSession.sidebarSessionId,
            projectId: resolvedSession.projectId,
            sessionId: resolvedSession.sessionId,
          },
        };
      }
      case "renameCommand": {
        const resolvedSession = this.resolveGxserverRendererCommandSession(command.payload);
        if (!resolvedSession) {
          throw new Error("No matching session was found.");
        }
        const title = normalizeGpuiRendererCommandRenameTitle(command.payload);
        if (!title) {
          throw new Error("Invalid renderer command title.");
        }
        this.postLocalWorkspaceTerminalRenameCommand(
          resolvedSession.projectId,
          resolvedSession.sessionId,
          title,
        );
        return {
          accepted: true,
          action: "renameCommand",
          ok: true,
          session: {
            ghostexId: resolvedSession.sidebarSessionId,
            projectId: resolvedSession.projectId,
            sessionId: resolvedSession.sessionId,
          },
        };
      }
      case "runCommand":
        return this.runGxserverRendererCommandButton(
          readGpuiRecordString(command.payload, "commandId"),
          command,
        );
      case "openBrowser":
      case "openBrowserPane":
        return this.openEmbeddedBrowserFromRendererCommand(command);
      case "clickButton": {
        const kind = readGpuiRecordString(command.payload, "kind")?.trim();
        if (kind !== "command") {
          throw new Error("Unsupported renderer command.");
        }
        return this.runGxserverRendererCommandButton(
          readGpuiRecordString(command.payload, "id"),
          command,
        );
      }
      default:
        throw new Error("Unsupported renderer command.");
    }
  }

  private runGxserverRendererCommandButton(
    rawCommandId: string | undefined,
    rendererCommand: GxserverRendererCommand,
  ): Record<string, unknown> {
    /*
    CDXC:GxserverRendererCommands 2026-06-27-05:51:
    gxserver `runCommand` and `clickButton(kind:"command")` must launch the same trusted project Action button as native. Treat renderer payloads as selectors only; command text, URLs, close-on-exit normalization, completion-sound preference, cwd/env, paths, output, and logs must come from the live HUD command and fixed Rust command-action bridge.
    */
    const commandId = normalizeNonEmptyString(rawCommandId)?.trim();
    if (!commandId) {
      throw new Error("Unsupported renderer command.");
    }
    const command = this.resolveSidebarCommand(commandId);
    if (!command || !isSidebarCommandConfigured(command)) {
      throw new Error("Unsupported renderer command.");
    }
    const selectionMessage: Extract<SidebarToExtensionMessage, { type: "runSidebarCommand" }> = {
      commandId,
      type: "runSidebarCommand",
    };
    if (!this.postSidebarCommandAction(command, selectionMessage)) {
      throw new Error("Renderer command bridge unavailable.");
    }
    return {
      accepted: true,
      action: rendererCommand.action,
      ok: true,
    };
  }

  private openEmbeddedBrowserFromRendererCommand(
    command: GxserverRendererCommand,
  ): Record<string, unknown> {
    /*
    macOS `openNativeBrowserPaneFromCli` parity for `ghostex browser open` /
    `gx ln`. The renderer payload contributes only the URL-or-search text and
    the fixed reuse selector; Rust re-normalizes the address and owns tab
    reuse/creation. GPUI's Browser shell swaps with the active project, so the
    reuse scope is the current project's Browser tabs.
    */
    const post = window.ghostexGpui?.postOpenBrowserUrl;
    if (typeof post !== "function") {
      throw new Error("Renderer command bridge unavailable.");
    }
    const url = readGpuiRecordString(command.payload, "url")?.trim() ?? "";
    if (url.length > GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS) {
      throw new Error("Invalid renderer command URL.");
    }
    const rawReuse = readGpuiRecordString(command.payload, "reuse")?.trim().toLowerCase();
    const reuse = rawReuse === "exact" || rawReuse === "none" ? rawReuse : "similar";
    const payload = JSON.stringify({
      reuse,
      type: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
      url,
      version: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
    });
    if (!post(payload)) {
      throw new Error("Renderer command bridge unavailable.");
    }
    return {
      accepted: true,
      action: command.action,
      ok: true,
    };
  }

  private resolveGxserverRendererCommandSession(
    payload: Record<string, unknown>,
  ): GpuiRendererCommandResolvedSession | undefined {
    /*
    CDXC:GxserverRendererCommands 2026-06-27-02:05:
    gxserver renderer commands can target local sessions with raw project/session ids in `sessionTarget`, while the reused GPUI SidebarApp renders combined `combined-session:<project>:<session>` ids. Resolve those raw ids to the same combined sidebar id before invoking runtime focus logic, and keep the command result bounded to ids/status rather than paths, titles, command text, URLs, tokens, terminal output, or renderer payload echoes.
    */
    const target = readGpuiRendererCommandSessionTarget(payload);
    const globalReference = parseGpuiRendererCommandGlobalSessionRef(
      readGpuiRecordString(target, "globalRef") ?? readGpuiRecordString(payload, "globalRef"),
    );
    const projectId =
      readGpuiRecordString(target, "projectId")?.trim() ||
      readGpuiRecordString(payload, "projectId")?.trim() ||
      globalReference?.projectId;
    const sessionId =
      readGpuiRecordString(target, "sessionId")?.trim() ||
      readGpuiRecordString(payload, "sessionId")?.trim() ||
      globalReference?.sessionId;
    if (!sessionId) {
      return undefined;
    }
    const scopedSession = parseGxserverPresentationProjectSessionId(sessionId);
    if (scopedSession) {
      if (projectId && scopedSession.projectId !== projectId) {
        return undefined;
      }
      if (
        !this.hasGpuiRendererCommandLocalSession(scopedSession.projectId, scopedSession.sessionId)
      ) {
        return undefined;
      }
      return {
        projectId: scopedSession.projectId,
        sessionId: scopedSession.sessionId,
        sidebarSessionId: sessionId,
      };
    }
    if (!projectId) {
      return undefined;
    }
    if (!this.hasGpuiRendererCommandLocalSession(projectId, sessionId)) {
      return undefined;
    }
    return {
      projectId,
      sessionId,
      sidebarSessionId: createGxserverPresentationProjectSessionId(projectId, sessionId),
    };
  }

  private hasGpuiRendererCommandLocalSession(projectId: string, sessionId: string): boolean {
    if (
      this.presentation?.sessions.some(
        (session) => session.projectId === projectId && session.sessionId === sessionId,
      )
    ) {
      return true;
    }
    return this.latestGroups.some((group) =>
      group.sessions.some((session) => {
        const reference = parseGxserverPresentationProjectSessionId(session.sessionId);
        return reference?.projectId === projectId && reference.sessionId === sessionId;
      }),
    );
  }

  private postLocalWorkspaceTerminalRenameCommand(
    projectId: string,
    sessionId: string,
    title: string,
  ): void {
    /*
    CDXC:GxserverRendererCommands 2026-06-27-02:27:
    GPUI `renameCommand` is accepted when TypeScript resolves gxserver's raw sessionTarget to the local workspace session and posts one fixed fire-and-forget Rust bridge payload. Keep the result and errors id-only, and pass the normalized title only through `postWorkspaceTerminalRenameCommand` so logs/results do not expose user title text, command text, paths, URLs, tokens, or terminal output.

    CDXC:GPUISidebarRename 2026-07-28:
    Pi names its session with `/name <title>` and Hermes Agent uses
    `/title <title>` instead of `/rename <title>`, so the payload carries a
    fixed command selector resolved from the session's own agent identity.
    Rust still owns turning that selector into the actual terminal input.
    */
    const postRename = window.ghostexGpui?.postWorkspaceTerminalRenameCommand;
    if (typeof postRename !== "function") {
      throw new Error("Renderer command bridge unavailable.");
    }
    /*
    CDXC:GPUISidebarRename 2026-07-29:
    Rust may only type the rename command into a mounted Ghostty surface, and
    it accepts a sidebar-focus attach for this session only while gxserver
    presentation focus agrees. Activate the session exactly like a session-card
    click first so a rename of a background session mounts its terminal
    instead of being dropped at the surface-ownership check.
    */
    this.focusLocalWorkspaceSession(projectId, sessionId);
    this.publishPresentation("patch");
    const session = this.findLocalPresentationSession(projectId, sessionId);
    const agent = (session?.agentId ?? session?.agentName ?? "").trim().toLowerCase();
    const bridgeSent = postRename(
      JSON.stringify({
        version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION,
        type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE,
        projectId,
        sessionId,
        title,
        command: gpuiWorkspaceTerminalTitleCommandForAgent(agent),
      }),
    );
    if (!bridgeSent) {
      throw new Error("Renderer command bridge unavailable.");
    }
  }

  private recoverPresentationStream(clientId: string): void {
    if (!this.client) {
      return;
    }
    const client = this.client;
    this.subscription?.close();
    this.subscription = undefined;
    void Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchProjectList().catch(() => undefined),
      client.fetchRecentProjects().catch(() => undefined),
      client.fetchSidebarHud(this.activeProjectId),
    ])
      .then(([snapshot, domainProjects, recentProjects, sidebarHud]) => {
        if (this.client !== client) {
          return;
        }
        if (domainProjects) {
          this.domainProjects = [...domainProjects];
        }
        if (recentProjects) {
          this.recentProjects = [...recentProjects];
        }
        this.sidebarHud = sidebarHud;
        this.applyPresentationSnapshot(snapshot, this.hasHydrated ? "patch" : "hydrate");
        this.openPresentationSubscription(clientId, snapshot.revision);
      })
      .catch(() => {
        this.publishUnavailable("stream-recovery-failed");
      });
  }

  private applyPresentationSnapshot(
    snapshot: GxserverPresentationSnapshot,
    kind: GpuiSidebarRuntimeSnapshotKind,
  ): void {
    const previousSessions = this.presentation?.sessions ?? [];
    const projectedSnapshot = this.projectLocalPresentationAttentionAcknowledgementGuards(snapshot);
    this.presentation = projectedSnapshot;
    this.syncLocalPresentationAttentionTracking(previousSessions, projectedSnapshot.sessions);
    if (isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
      this.forwardSidebarProjectCollectionsFromGxserver(snapshot.sidebarProjectCollections);
    }
    this.publishPresentation(kind);
    this.notifyNativeGxserverPresentationReady();
    if (kind === "hydrate") {
      void this.runGpuiAutoSleepMonitor("startup");
      this.autoMaterializeStartupFocusedSession();
    }
  }

  private autoMaterializeStartupFocusedSession(): void {
    /*
    Restore eagerness (Decision #3, 2026-07-02, revised 2026-08-07): the
    session the user was looking at when the app quit re-materializes
    automatically on relaunch. Rust persists the presentation focus state
    across restarts and replays it through the bootstrap; once the first
    presentation hydrate confirms that focused session is still a running local
    session, re-attach it through the normal workspace focus bridge. This
    covers the focused session only. Every other surfaced session — the other
    panes of a split, remote attach tabs, and sessions whose provider went to
    sleep while the app was closed — is now restored by Rust from the workspace
    model it already owns, so nothing further is needed here.
    */
    if (this.didAutoMaterializeStartupSession) {
      return;
    }
    this.didAutoMaterializeStartupSession = true;
    const focusedSessionId = this.focusedSessionId;
    if (!focusedSessionId || !this.visibleSessionIds.has(focusedSessionId)) {
      return;
    }
    const session = this.presentation?.sessions.find(
      (presentationSession) => presentationSession.sessionId === focusedSessionId,
    );
    if (!session || session.lifecycleState !== "running") {
      return;
    }
    this.postLocalWorkspaceTerminalFocus(session.projectId, focusedSessionId);
  }

  private applyPresentationDelta(delta: GxserverPresentationDelta, gxserverRevision: number): void {
    if (!this.presentation || gxserverRevision <= this.presentation.revision) {
      return;
    }
    this.applyDomainProjectDelta(delta);
    const previousSessions = this.presentation.sessions;
    const projectedSnapshot = this.projectLocalPresentationAttentionAcknowledgementGuards(
      reduceGxserverPresentationDelta(this.presentation, delta, gxserverRevision),
    );
    this.presentation = projectedSnapshot;
    this.syncLocalPresentationAttentionTracking(previousSessions, projectedSnapshot.sessions);
    this.detectSessionAttentionCompletionSounds(previousSessions, projectedSnapshot.sessions);
    this.publishPresentation("patch");
  }

  private acknowledgeSessionAttention(
    sessionId: string,
    reason: GpuiSessionAttentionAcknowledgeReason,
  ): boolean {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      return this.acknowledgePresentationSessionAttention(
        {
          kind: "remote",
          machineId: remoteSession.machineId,
          projectId: remoteSession.projectId,
          sessionId: remoteSession.sessionId,
        },
        reason,
      );
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference) {
      return false;
    }
    return this.acknowledgePresentationSessionAttention(
      {
        kind: "local",
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      },
      reason,
    );
  }

  private acknowledgePresentationSessionAttention(
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason,
  ): boolean {
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity !== "attention") {
      return false;
    }
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    const attentionEnteredAt = this.attentionEnteredAtBySessionKey.get(sessionKey);
    const remainingVisibleMs =
      attentionEnteredAt === undefined
        ? 0
        : GPUI_MIN_ATTENTION_VISIBLE_MS - Math.max(0, Date.now() - attentionEnteredAt);
    if (attentionEnteredAt !== undefined && remainingVisibleMs > 0) {
      if (!this.attentionAcknowledgementTimeoutsBySessionKey.has(sessionKey)) {
        const timeout = window.setTimeout(() => {
          this.attentionAcknowledgementTimeoutsBySessionKey.delete(sessionKey);
          const latestAttentionEnteredAt = this.attentionEnteredAtBySessionKey.get(sessionKey);
          if (
            latestAttentionEnteredAt !== attentionEnteredAt ||
            !this.completePresentationSessionAttentionAcknowledgement(
              target,
              reason,
              attentionEnteredAt,
            )
          ) {
            return;
          }
        }, remainingVisibleMs);
        this.attentionAcknowledgementTimeoutsBySessionKey.set(sessionKey, timeout);
      }
      return true;
    }
    return this.completePresentationSessionAttentionAcknowledgement(
      target,
      reason,
      attentionEnteredAt,
    );
  }

  private completePresentationSessionAttentionAcknowledgement(
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason,
    attentionEnteredAt?: number,
  ): boolean {
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity !== "attention") {
      return false;
    }
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    const latestAttentionEnteredAt = this.attentionEnteredAtBySessionKey.get(sessionKey);
    if (
      attentionEnteredAt !== undefined &&
      latestAttentionEnteredAt !== undefined &&
      latestAttentionEnteredAt !== attentionEnteredAt
    ) {
      return false;
    }

    const didChange = this.clearPresentationSessionAttentionLocally(target, reason);
    if (target.kind === "remote") {
      if (didChange) {
        this.publishRemotePresentationPatch();
      }
      void this.syncRemoteSessionAttentionAcknowledgementWithGxserver(
        target.machineId,
        target.projectId,
        target.sessionId,
        normalizeNonEmptyString(session.agentName),
      );
      return true;
    }

    if (didChange) {
      this.publishPresentation("patch");
    }
    void this.syncLocalSessionAttentionAcknowledgementWithGxserver(
      target.projectId,
      target.sessionId,
      normalizeNonEmptyString(session.agentName),
    );
    return true;
  }

  private clearPresentationSessionAttentionLocally(
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason,
  ): boolean {
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity !== "attention") {
      return false;
    }
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    const attentionEventId =
      getGpuiPresentationAttentionEventId(session) ??
      this.attentionEventIdBySessionKey.get(sessionKey);
    this.markSessionAttentionEventLocallyAcknowledged(sessionKey, attentionEventId);
    const didChange =
      target.kind === "remote"
        ? this.setRemotePresentationSessionActivityLocally(
            target.machineId,
            target.projectId,
            target.sessionId,
            "idle",
            reason,
          )
        : this.setLocalPresentationSessionActivityLocally(
            target.projectId,
            target.sessionId,
            "idle",
            reason,
          );
    this.clearSessionAttentionTracking(sessionKey);
    return didChange;
  }

  private currentPresentationSessionForAttentionTarget(
    target: GpuiSessionAttentionTarget,
  ): GxserverPresentationSession | undefined {
    if (target.kind === "remote") {
      return this.findRemotePresentationSession(target);
    }
    return this.findLocalPresentationSession(target.projectId, target.sessionId);
  }

  private findLocalPresentationSession(
    projectId: string,
    sessionId: string,
  ): GxserverPresentationSession | undefined {
    return this.presentation?.sessions.find(
      (session) => session.projectId === projectId && session.sessionId === sessionId,
    );
  }

  private setLocalPresentationSessionActivityLocally(
    projectId: string,
    sessionId: string,
    activity: GxserverPresentationSession["activity"],
    _reason: string,
  ): boolean {
    const presentation = this.presentation;
    if (!presentation) {
      return false;
    }
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (session.projectId !== projectId || session.sessionId !== sessionId) {
        return session;
      }
      if (
        session.activity === activity &&
        (activity === "attention" || session.attention === undefined)
      ) {
        return session;
      }
      didChange = true;
      if (activity !== "attention") {
        const { attention: _attention, ...withoutAttention } = session;
        return {
          ...withoutAttention,
          activity,
        };
      }
      return {
        ...session,
        activity,
      };
    });
    if (!didChange) {
      return false;
    }
    this.presentation = {
      ...presentation,
      sessions,
    };
    return true;
  }

  private setRemotePresentationSessionActivityLocally(
    machineId: string,
    projectId: string,
    sessionId: string,
    activity: GxserverPresentationSession["activity"],
    _reason: string,
  ): boolean {
    const presentation = this.remotePresentations.get(machineId);
    if (!presentation) {
      return false;
    }
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (session.projectId !== projectId || session.sessionId !== sessionId) {
        return session;
      }
      if (
        session.activity === activity &&
        (activity === "attention" || session.attention === undefined)
      ) {
        return session;
      }
      didChange = true;
      if (activity !== "attention") {
        const { attention: _attention, ...withoutAttention } = session;
        return {
          ...withoutAttention,
          activity,
        };
      }
      return {
        ...session,
        activity,
      };
    });
    if (!didChange) {
      return false;
    }
    this.remotePresentations.set(machineId, {
      ...presentation,
      sessions,
    });
    return true;
  }

  private async syncLocalSessionAttentionAcknowledgementWithGxserver(
    projectId: string,
    sessionId: string,
    agentName: string | undefined,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    try {
      await client.rpc("/api/updateAgentActivity", {
        ...(agentName ? { agentName } : {}),
        event: "acknowledge",
        projectId,
        sessionId,
      });
    } catch {
      // gxserver acknowledgement sync is best-effort, matching macOS's log-only failure path.
    }
  }

  private async syncLocalSessionTerminalEscapeWithGxserver(
    projectId: string,
    sessionId: string,
    agentName: string | undefined,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    try {
      await client.rpc("/api/updateAgentActivity", {
        ...(agentName ? { agentName } : {}),
        event: "escape",
        projectId,
        sessionId,
      });
    } catch {
      // Terminal escape suppression is best-effort locally until gxserver confirms it.
    }
  }

  private async syncRemoteSessionAttentionAcknowledgementWithGxserver(
    machineId: string,
    projectId: string,
    sessionId: string,
    agentName: string | undefined,
  ): Promise<void> {
    try {
      await this.requestRemoteGxserver(machineId, "/api/updateAgentActivity", {
        ...(agentName ? { agentName } : {}),
        event: "acknowledge",
        projectId,
        sessionId,
      });
    } catch {
      // Remote acknowledgement uses the same optimistic presentation clear as local.
    }
  }

  private projectLocalPresentationAttentionAcknowledgementGuards(
    presentation: GxserverPresentationSnapshot,
  ): GxserverPresentationSnapshot {
    return this.projectPresentationAttentionAcknowledgementGuards(presentation, (session) =>
      createGxserverPresentationProjectSessionId(session.projectId, session.sessionId),
    );
  }

  private projectRemotePresentationAttentionAcknowledgementGuards(
    machineId: string,
    presentation: GxserverPresentationSnapshot,
  ): GxserverPresentationSnapshot {
    return this.projectPresentationAttentionAcknowledgementGuards(presentation, (session) =>
      createGpuiRemotePresentationSessionId(machineId, session.projectId, session.sessionId),
    );
  }

  private projectPresentationAttentionAcknowledgementGuards(
    presentation: GxserverPresentationSnapshot,
    sessionKeyForPresentationSession: (session: GxserverPresentationSession) => string,
  ): GxserverPresentationSnapshot {
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (session.activity !== "attention") {
        return session;
      }
      const sessionKey = sessionKeyForPresentationSession(session);
      const attentionEventId = getGpuiPresentationAttentionEventId(session);
      if (
        attentionEventId !== undefined &&
        this.isSessionAttentionEventLocallyAcknowledged(sessionKey, attentionEventId)
      ) {
        didChange = true;
        const { attention: _attention, ...withoutAttention } = session;
        return {
          ...withoutAttention,
          activity: "idle" as const,
        };
      }
      if (attentionEventId !== undefined) {
        this.clearLocallyAcknowledgedAttentionEventsForSession(sessionKey);
      }
      return session;
    });
    return didChange
      ? {
          ...presentation,
          sessions,
        }
      : presentation;
  }

  private syncLocalPresentationAttentionTracking(
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[],
  ): void {
    this.syncPresentationAttentionTracking(previousSessions, nextSessions, (session) =>
      createGxserverPresentationProjectSessionId(session.projectId, session.sessionId),
    );
  }

  private syncRemotePresentationAttentionTracking(
    machineId: string,
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[],
  ): void {
    this.syncPresentationAttentionTracking(previousSessions, nextSessions, (session) =>
      createGpuiRemotePresentationSessionId(machineId, session.projectId, session.sessionId),
    );
  }

  private syncPresentationAttentionTracking(
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[],
    sessionKeyForPresentationSession: (session: GxserverPresentationSession) => string,
  ): void {
    const previousKeys = new Set(previousSessions.map(sessionKeyForPresentationSession));
    const nextAttentionKeys = new Set<string>();
    for (const session of nextSessions) {
      const sessionKey = sessionKeyForPresentationSession(session);
      if (session.activity !== "attention") {
        if (previousKeys.has(sessionKey)) {
          this.clearSessionAttentionTracking(sessionKey);
        }
        continue;
      }
      nextAttentionKeys.add(sessionKey);
      const nextAttentionEventId = getGpuiPresentationAttentionEventId(session);
      const hadPreviousEventId = this.attentionEventIdBySessionKey.has(sessionKey);
      const previousAttentionEventId = this.attentionEventIdBySessionKey.get(sessionKey);
      const eventIdChanged =
        nextAttentionEventId !== (hadPreviousEventId ? previousAttentionEventId : undefined) &&
        (nextAttentionEventId !== undefined || hadPreviousEventId);
      if (!this.attentionEnteredAtBySessionKey.has(sessionKey) || eventIdChanged) {
        this.clearSessionAttentionAcknowledgementTimer(sessionKey);
        this.attentionEnteredAtBySessionKey.set(sessionKey, Date.now());
      }
      if (nextAttentionEventId === undefined) {
        this.attentionEventIdBySessionKey.delete(sessionKey);
      } else {
        this.attentionEventIdBySessionKey.set(sessionKey, nextAttentionEventId);
      }
    }
    for (const session of previousSessions) {
      const sessionKey = sessionKeyForPresentationSession(session);
      if (!nextAttentionKeys.has(sessionKey)) {
        this.clearSessionAttentionTracking(sessionKey);
      }
    }
  }

  private clearSessionAttentionTracking(sessionKey: string): void {
    this.clearSessionAttentionAcknowledgementTimer(sessionKey);
    this.attentionEnteredAtBySessionKey.delete(sessionKey);
    this.attentionEventIdBySessionKey.delete(sessionKey);
  }

  private clearSessionAttentionAcknowledgementTimer(sessionKey: string): void {
    const timeout = this.attentionAcknowledgementTimeoutsBySessionKey.get(sessionKey);
    if (timeout === undefined) {
      return;
    }
    window.clearTimeout(timeout);
    this.attentionAcknowledgementTimeoutsBySessionKey.delete(sessionKey);
  }

  private markSessionAttentionEventLocallyAcknowledged(
    sessionKey: string,
    attentionEventId: string | undefined,
  ): void {
    const eventKey = getGpuiSessionAttentionEventKey(sessionKey, attentionEventId);
    if (eventKey === undefined || this.locallyAcknowledgedAttentionEventKeys.has(eventKey)) {
      return;
    }
    this.locallyAcknowledgedAttentionEventKeys.add(eventKey);
    this.locallyAcknowledgedAttentionEventKeyOrder.push(eventKey);
    while (
      this.locallyAcknowledgedAttentionEventKeyOrder.length >
      GPUI_LOCALLY_ACKNOWLEDGED_ATTENTION_EVENT_CACHE_LIMIT
    ) {
      const staleKey = this.locallyAcknowledgedAttentionEventKeyOrder.shift();
      if (staleKey !== undefined) {
        this.locallyAcknowledgedAttentionEventKeys.delete(staleKey);
      }
    }
  }

  private isSessionAttentionEventLocallyAcknowledged(
    sessionKey: string,
    attentionEventId: string | undefined,
  ): boolean {
    const eventKey = getGpuiSessionAttentionEventKey(sessionKey, attentionEventId);
    return eventKey !== undefined && this.locallyAcknowledgedAttentionEventKeys.has(eventKey);
  }

  private clearLocallyAcknowledgedAttentionEventsForSession(sessionKey: string): void {
    const keyPrefix = `${sessionKey}\u001f`;
    let didClear = false;
    for (const eventKey of this.locallyAcknowledgedAttentionEventKeys) {
      if (eventKey.startsWith(keyPrefix)) {
        this.locallyAcknowledgedAttentionEventKeys.delete(eventKey);
        didClear = true;
      }
    }
    if (didClear) {
      this.locallyAcknowledgedAttentionEventKeyOrder =
        this.locallyAcknowledgedAttentionEventKeyOrder.filter(
          (eventKey) => !eventKey.startsWith(keyPrefix),
        );
    }
  }

  /*
  Completion sound + card flash (macOS parity): only live presentation deltas
  represent the edge where a session newly enters attention. Startup and
  stream-recovery snapshots can carry sessions that were already in attention
  before this client observed them, so applyPresentationSnapshot must not run
  this detection, and re-published attention states dedupe by attention event
  id so a replayed event updates UI state without replaying the sound. Focus
  acknowledgement round-trips through gxserver and only ever transitions a
  session OUT of attention, so it cannot re-trigger this edge.
  */
  private detectSessionAttentionCompletionSounds(
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[],
  ): void {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    if (!settings.completionBellEnabled) {
      return;
    }
    let previousActivityBySessionKey:
      Map<string, GxserverPresentationSession["activity"]> | undefined;
    for (const session of nextSessions) {
      if (session.activity !== "attention" || session.attention?.acknowledged === true) {
        continue;
      }
      const sessionKey = createGxserverPresentationProjectSessionId(
        session.projectId,
        session.sessionId,
      );
      if (this.getAttentionCompletionSoundSuppressedUntil(sessionKey) !== undefined) {
        continue;
      }
      previousActivityBySessionKey ??= new Map(
        previousSessions.map((previousSession) => [
          createGxserverPresentationProjectSessionId(
            previousSession.projectId,
            previousSession.sessionId,
          ),
          previousSession.activity,
        ]),
      );
      const previousActivity = previousActivityBySessionKey.get(sessionKey);
      if (previousActivity === undefined || previousActivity === "attention") {
        continue;
      }
      const attentionEventId = getGpuiPresentationAttentionEventId(session);
      if (
        attentionEventId !== undefined &&
        !this.rememberAttentionCompletionSoundEventKey(`${sessionKey}\u001f${attentionEventId}`)
      ) {
        continue;
      }
      this.messageSource.postMessage({
        sessionId: sessionKey,
        sound: settings.completionSound,
        type: "playCompletionSound",
      });
      this.postNativeSessionCompletionSound(settings.completionSound);
    }
  }

  private suppressAttentionCompletionSoundAfterTerminalEscape(sessionKey: string): void {
    this.attentionCompletionSoundSuppressedUntilBySessionKey.set(
      sessionKey,
      Date.now() + GPUI_ESCAPE_DONE_SUPPRESSION_MS,
    );
  }

  private getAttentionCompletionSoundSuppressedUntil(sessionKey: string): number | undefined {
    const suppressedUntil =
      this.attentionCompletionSoundSuppressedUntilBySessionKey.get(sessionKey);
    if (suppressedUntil === undefined) {
      return undefined;
    }
    if (!Number.isFinite(suppressedUntil) || suppressedUntil <= Date.now()) {
      this.attentionCompletionSoundSuppressedUntilBySessionKey.delete(sessionKey);
      return undefined;
    }
    return suppressedUntil;
  }

  /*
  GPUI has no webview sound assets (the SidebarApp player's sound-URL global
  is never populated), so audible playback is Rust-owned from the bundled
  sound files — the same native-playback ownership macOS uses via its
  playSound host message. The SidebarApp message above still drives the card
  flash.
  */
  private postNativeSessionCompletionSound(sound: CompletionSoundSetting): void {
    const postCompletionSound = window.ghostexGpui?.postSessionCompletionSound;
    if (typeof postCompletionSound !== "function") {
      return;
    }
    postCompletionSound(
      JSON.stringify({
        version: GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION,
        type: GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE,
        sound,
      }),
    );
  }

  private rememberAttentionCompletionSoundEventKey(eventKey: string): boolean {
    if (this.attentionCompletionSoundEventKeys.has(eventKey)) {
      return false;
    }
    this.attentionCompletionSoundEventKeys.add(eventKey);
    this.attentionCompletionSoundEventKeyOrder.push(eventKey);
    while (
      this.attentionCompletionSoundEventKeyOrder.length >
      GPUI_ATTENTION_COMPLETION_SOUND_EVENT_CACHE_LIMIT
    ) {
      const staleKey = this.attentionCompletionSoundEventKeyOrder.shift();
      if (staleKey !== undefined) {
        this.attentionCompletionSoundEventKeys.delete(staleKey);
      }
    }
    return true;
  }

  private publishPresentation(kind: GpuiSidebarRuntimeSnapshotKind): void {
    const presentation = this.presentation;
    if (!presentation) {
      this.publishUnavailable("presentation-missing");
      return;
    }

    const previousGroups = this.latestGroups;
    const groups = this.createSidebarGroups(presentation);
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-23:24:
    Sidebar session-card wake decisions should use the lifecycle state that was just rendered from gxserver presentation. Cache only bounded local project/session routing ids for sleeping rows before emitting hydrate/patch so a same-tick click cannot miss the sleeping state and fall through to plain focus.
    */
    this.sleepingLocalSidebarSessionIds = new Set(
      groups.flatMap((group) =>
        group.sessions.flatMap((session) => {
          const reference = parseGxserverPresentationProjectSessionId(session.sessionId);
          return reference && (session.lifecycleState === "sleeping" || session.isSleeping === true)
            ? [createGxserverPresentationProjectSessionId(reference.projectId, reference.sessionId)]
            : [];
        }),
      ),
    );
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      git: this.gitStateForHud(),
      groups,
      presentation,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });

    if (kind === "hydrate" || !this.hasHydrated) {
      this.messageSource.postMessage(this.createHydrateMessage(groups, this.latestHud));
      this.hasHydrated = true;
    } else {
      const patch = createGpuiSidebarGroupsPatch(previousGroups, groups);
      const revision = ++this.revision;
      this.messageSource.postMessage({
        groupOrder: patch.groupOrder,
        groups: patch.groups,
        removedGroupIds: patch.removedGroupIds,
        removedSessionIds: patch.removedSessionIds,
        revision,
        type: "sidebarGroupsChanged",
      });
      this.messageSource.postMessage({
        hud: this.latestHud,
        revision,
        type: "sidebarHudChanged",
      });
    }
    this.latestGroups = groups;
    this.postGpuiStatusPetState();
    this.postActiveProjectContext();
    this.postGxserverPresentationFocusState();
    this.postTitlebarGitMenuState();
    this.refreshGitStateForActiveProjectIfNeeded();
  }

  private publishUnavailable(_reason: string): void {
    if (this.presentation) {
      this.syncLocalPresentationAttentionTracking(this.presentation.sessions, []);
    }
    this.presentation = undefined;
    this.appUserData = createEmptyGpuiAppUserData();
    this.domainProjects = [];
    this.dropLocalPresentationSessionFocus();
    this.gitState = createDefaultSidebarGitState();
    this.lastGitRefreshProjectId = undefined;
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    gxserver went away, so nothing memoized about its projects can be trusted
    or republished. Drop both leases and cancel any in-flight GitHub probe so a
    reconnect starts from real probes.
    */
    this.gitStateMemoByProjectId.clear();
    this.gitHubStateMemoByProjectId.clear();
    for (const timeoutId of this.gitHubProbeTimeoutIds) {
      window.clearTimeout(timeoutId);
    }
    this.gitHubProbeTimeoutIds.clear();
    this.pendingGitHubProbeProjectIds.clear();
    this.pendingGitCommitRequests.clear();
    this.recentProjects = [];
    this.sidebarHud = undefined;
    this.latestGroups = this.overlayProjectDiffStats([
      ...createGpuiGxserverUnavailableSidebarGroups(),
      ...this.createRemoteSidebarGroups(),
    ]);
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      git: this.gitStateForHud(),
      groups: this.latestGroups,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });
    this.messageSource.postMessage(this.createHydrateMessage(this.latestGroups, this.latestHud));
    this.hasHydrated = true;
    this.postGpuiStatusPetState();
    this.postActiveProjectContext();
    this.postGxserverPresentationFocusState();
    this.postTitlebarGitMenuState();
  }

  private publishRemotePresentationPatch(): void {
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
        this.forwardRemoteSidebarProjectCollectionsFromGxserver(
          machineId,
          snapshot.sidebarProjectCollections,
        );
      }
    }
    const previousGroups = this.latestGroups;
    const groups = this.presentation
      ? this.createSidebarGroups(this.presentation)
      : this.overlayProjectDiffStats([
          ...createGpuiGxserverUnavailableSidebarGroups(),
          ...this.createRemoteSidebarGroups(),
        ]);
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      git: this.gitStateForHud(),
      groups,
      presentation: this.presentation,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });
    if (!this.hasHydrated) {
      this.messageSource.postMessage(this.createHydrateMessage(groups, this.latestHud));
      this.hasHydrated = true;
    } else {
      const patch = createGpuiSidebarGroupsPatch(previousGroups, groups);
      const revision = ++this.revision;
      this.messageSource.postMessage({
        groupOrder: patch.groupOrder,
        groups: patch.groups,
        removedGroupIds: patch.removedGroupIds,
        removedSessionIds: patch.removedSessionIds,
        revision,
        type: "sidebarGroupsChanged",
      });
      this.messageSource.postMessage({
        hud: this.latestHud,
        revision,
        type: "sidebarHudChanged",
      });
    }
    this.latestGroups = groups;
    this.postGpuiStatusPetState();
    this.postActiveProjectContext();
    this.postGxserverPresentationFocusState();
    this.postTitlebarGitMenuState();
  }

  private applyDomainProjectDelta(delta: GxserverPresentationDelta): void {
    if ("domainProject" in delta && delta.domainProject) {
      const nextProject = delta.domainProject;
      const existingIndex = this.domainProjects.findIndex(
        (project) => project.projectId === nextProject.projectId,
      );
      this.domainProjects =
        existingIndex >= 0
          ? this.domainProjects.map((project, index) =>
              index === existingIndex ? nextProject : project,
            )
          : [...this.domainProjects, nextProject];
      if (
        nextProject.isRecentProject === true ||
        this.recentProjects.some((project) => project.projectId === nextProject.projectId)
      ) {
        this.refreshRecentProjectsFromClient();
      }
      this.refreshSidebarHudFromClient();
      return;
    }
    if (delta.type === "projectRemoved") {
      this.domainProjects = this.domainProjects.filter(
        (project) => project.projectId !== delta.projectId,
      );
      this.refreshRecentProjectsFromClient();
      this.refreshSidebarHudFromClient();
    }
  }

  private refreshRecentProjectsFromClient(): void {
    const client = this.client;
    if (!client) {
      return;
    }
    void client
      .fetchRecentProjects()
      .then((recentProjects) => {
        if (this.client !== client) {
          return;
        }
        this.recentProjects = [...recentProjects];
        if (this.presentation) {
          this.publishPresentation("patch");
          return;
        }
        this.publishHudPatch();
      })
      .catch(() => undefined);
  }

  private refreshSidebarHudFromClient(): void {
    const client = this.client;
    if (!client) {
      return;
    }
    void client
      .fetchSidebarHud(this.activeProjectId)
      .then((sidebarHud) => {
        if (this.client !== client) {
          return;
        }
        this.sidebarHud = sidebarHud;
        this.publishHudPatch();
      })
      .catch(() => {
        /*
         * CDXC:SidebarHudContract 2026-06-24-20:34:
         * Sidebar HUD projection refresh is best-effort after active-project or
         * project-metadata changes. Failure keeps the previous gxserver
         * projection instead of rebuilding custom launcher/action rows from
         * raw project metadata in the renderer.
         */
      });
  }

  private publishHudPatch(): void {
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      git: this.gitStateForHud(),
      groups: this.latestGroups,
      presentation: this.presentation,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });
    this.postTitlebarGitMenuState();
    if (!this.hasHydrated) {
      return;
    }
    this.messageSource.postMessage({
      hud: this.latestHud,
      revision: ++this.revision,
      type: "sidebarHudChanged",
    });
  }

  private postTitlebarGitMenuState(attempt = 0): void {
    if (this.titlebarGitMenuStateRetryId !== undefined) {
      window.clearTimeout(this.titlebarGitMenuStateRetryId);
      this.titlebarGitMenuStateRetryId = undefined;
    }
    const postTitlebarGitMenuState = window.ghostexGpui?.postTitlebarGitMenuState;
    if (typeof postTitlebarGitMenuState !== "function") {
      if (attempt < GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS) {
        this.titlebarGitMenuStateRetryId = window.setTimeout(() => {
          this.postTitlebarGitMenuState(attempt + 1);
        }, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS);
      }
      return;
    }
    const payload = JSON.stringify(createGpuiTitlebarGitMenuStatePayload(this.gitStateForHud()));
    if (payload === this.lastTitlebarGitMenuStatePayload) {
      return;
    }
    this.lastTitlebarGitMenuStatePayload = payload;
    postTitlebarGitMenuState(payload);
  }

  private handleGpuiTitlebarGitAction(payload: unknown): void {
    const action = parseGpuiTitlebarGitAction(payload);
    if (!action) {
      return;
    }
    if (action === "refresh") {
      this.refreshTitlebarGitMenuState();
      return;
    }
    void this.runSidebarGitAction({
      ...(this.activeGroupId ? { groupId: this.activeGroupId } : {}),
      action,
      type: "runSidebarGitAction",
    });
  }

  private async handleGpuiGitCommitModalCommand(payload: unknown): Promise<void> {
    const message = parseGpuiGitCommitModalCommand(payload);
    if (!message) {
      return;
    }
    await this.handleSidebarMessage(message);
  }

  private handleGpuiWorktreeModalCommand(payload: unknown): void {
    const message = parseGpuiWorktreeModalCommand(payload);
    if (!message) {
      return;
    }
    switch (message.type) {
      case "requestProjectWorktrees":
        void this.requestProjectWorktrees(message);
        return;
      case "createProjectWorktree":
        void this.createProjectWorktree(message);
        return;
      case "confirmDeleteWorktree":
        void this.confirmDeleteWorktree(message);
        return;
      case "commitWorktreeBeforeDelete":
        void this.runSidebarGitAction({
          action: "commit",
          groupId: message.groupId,
          type: "runSidebarGitAction",
        });
        return;
    }
  }

  private refreshTitlebarGitMenuState(): void {
    if (this.activeGroupId && parseGpuiRemotePresentationGroupId(this.activeGroupId)) {
      void this.refreshGitStateForMessage({
        groupId: this.activeGroupId,
        type: "refreshGitState",
      });
      return;
    }
    const project = this.activeDomainProject();
    if (!project) {
      return;
    }
    void this.refreshGitState({ force: true, project, toastOnFailure: false });
  }

  private postActiveProjectContext(attempt = 0): void {
    if (this.activeProjectContextRetryId !== undefined) {
      window.clearTimeout(this.activeProjectContextRetryId);
      this.activeProjectContextRetryId = undefined;
    }

    const postActiveProjectContext = window.ghostexGpui?.postActiveProjectContext;
    if (typeof postActiveProjectContext !== "function") {
      /*
      CDXC:GPUISidebarGxserverRuntime 2026-06-24-11:00:
      CEF may install the sidebar bridge after the React entrypoint starts. Retry only the bridge send and rebuild the active-project payload from the latest live groups at send time, so startup never replays a stale fixture/workspace payload.
      */
      if (attempt < GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS) {
        this.activeProjectContextRetryId = window.setTimeout(() => {
          this.postActiveProjectContext(attempt + 1);
        }, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS);
      }
      return;
    }

    const payload = createGpuiSidebarActiveProjectContextPayloadFromGroups({
      groups: this.activeProjectContextGroups(),
    });
    /*
    CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
    The active-project helper owns Source/Kanban/Automate/Docs surface identity. Post its payload unchanged so Rust can strictly accept `automateBoardId` beside `kanbanBoardId` before issuing the bundled Automate runtime URL.
    */
    postActiveProjectContext(JSON.stringify(payload));
  }

  private postGxserverPresentationFocusState(): void {
    const postFocusState = window.ghostexGpui?.postGxserverPresentationFocusState;
    if (typeof postFocusState !== "function") {
      return;
    }
    const focusedRemoteSession = this.focusedSessionId
      ? parseGpuiRemotePresentationSessionId(this.focusedSessionId)
      : undefined;
    /*
    CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
    Rust treats this snapshot's activeProjectId as the authoritative Agents
    workspace switch target. `this.activeProjectId` stays a local-only concept
    while a remote session or remote group is active, so publishing it here
    yanked the workspace back to the last local project on every routine
    remote presentation patch. Publish the machine-scoped remote project id —
    the same key the active-project context bridge uses — whenever the remote
    machine owns focus. Tab sessions use the same already-projected SidebarApp
    group for local and remote workspaces. Remote rows retain their
    machine-scoped project identity so Rust can reconcile restored attach tabs
    without confusing them with local gxserver sessions.
    */
    const activeGroupRemoteReference = (() => {
      if (!this.activeGroupId) {
        return undefined;
      }
      const remoteGroup = parseGpuiRemotePresentationGroupId(this.activeGroupId);
      if (remoteGroup) {
        return remoteGroup;
      }
      const subgroup = parseGpuiWorkspaceSessionSubgroupId(this.activeGroupId);
      return subgroup ? parseGpuiRemotePresentationProjectId(subgroup.projectId) : undefined;
    })();
    const activeRemoteReference = focusedRemoteSession ?? activeGroupRemoteReference;
    const activeTabSessions = this.activeWorkspaceTabSessionsFromLatestGroups();
    const activeProjectId = activeRemoteReference
      ? createGpuiRemotePresentationProjectId(
          activeRemoteReference.machineId,
          activeRemoteReference.projectId,
        )
      : this.activeProjectId;
    const payload = JSON.stringify({
      activeProjectId,
      tabSessions: activeTabSessions,
      focusedSessionId: this.focusedSessionId,
      type: GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION,
      visibleSessionIds: [...this.visibleSessionIds],
    });
    try {
      postFocusState(payload);
    } catch {
      /*
      CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
      Focus-state publication is a sidebar-native synchronization hint for Rust bootstrap replay only. A missing or rejecting CEF bridge must not change gxserver data, create fallback focus ids, log renderer payloads, or block the visible SidebarApp state that React already owns.
      */
    }
  }

  private activeWorkspaceTabSessionsFromLatestGroups(): GpuiActiveWorkspaceTabSessionPayload[] {
    /*
    CDXC:GPUIWorkspaceTabsParity 2026-07-05:
    The native GPUI Agents tab strip mirrors the already-projected active
    SidebarApp group. Hidden, companion, carrier, and subgroup filtering stays
    upstream in createSidebarGroups; this bridge only serializes the active
    gxserver rows in their rendered order with the same visible title chain
    used by the SidebarApp cards and macOS pane tabs. Remote rows carry their
    machine-scoped project id plus the owning daemon's raw session id; titles
    are never reconstructed from remote attach metadata.
    */
    const activeGroup =
      this.latestGroups.find((group) => group.groupId === this.activeGroupId) ??
      this.latestGroups.find((group) => group.isActive);
    if (!activeGroup) {
      return [];
    }
    const seen = new Set<string>();
    const sessions: GpuiActiveWorkspaceTabSessionPayload[] = [];
    for (const session of activeGroup.sessions) {
      const localReference = parseGxserverPresentationProjectSessionId(session.sessionId);
      const remoteReference = parseGpuiRemotePresentationSessionId(session.sessionId);
      if (!localReference && !remoteReference) {
        continue;
      }
      if (localReference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
        continue;
      }
      const kind = session.sessionKind;
      if (kind !== "terminal" && kind !== "t3") {
        continue;
      }
      const projectId = remoteReference
        ? createGpuiRemotePresentationProjectId(
            remoteReference.machineId,
            remoteReference.projectId,
          )
        : localReference!.projectId;
      const sessionId = remoteReference?.sessionId ?? localReference!.sessionId;
      const key = remoteReference
        ? session.sessionId
        : createGxserverPresentationProjectSessionId(projectId, sessionId);
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      sessions.push({
        activity: session.activity,
        ...(session.agentIcon ? { agentIcon: session.agentIcon } : {}),
        ...(session.agentSessionId?.trim()
          ? { agentSessionId: session.agentSessionId.trim() }
          : {}),
        isGeneratingFirstPromptTitle: session.isGeneratingFirstPromptTitle === true,
        isSleeping: session.isSleeping === true,
        kind,
        ...(session.lifecycleState ? { lifecycleState: session.lifecycleState } : {}),
        projectId,
        sessionId,
        title: boundedGpuiActiveWorkspaceTabSessionTitle(
          session.displayTitle?.trim() ||
            session.primaryTitle?.trim() ||
            session.terminalTitle?.trim() ||
            session.alias.trim() ||
            DEFAULT_TERMINAL_SESSION_TITLE,
        ),
      });
    }
    return sessions;
  }

  /*
   * CDXC:GlobalActions 2026-08-01:
   * Publish only what the native strip draws: bounded action id, display name,
   * and icon slug. Command text, URLs, links, and run state deliberately stay
   * on this side — a strip click sends the id back and this runtime resolves
   * the trusted definition, so gpui never holds anything executable.
   */
  private postGpuiGlobalActions(): void {
    const actions = (this.sidebarHudState?.globalCommands ?? [])
      .slice(0, GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS)
      .map((command) => ({
        commandId: command.commandId,
        ...(command.icon ? { icon: command.icon } : {}),
        name: command.name,
      }));
    const payload = JSON.stringify({
      actions,
      type: GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION,
    });
    if (payload === this.postedGlobalActionsPayload) {
      return;
    }
    /*
     * Cache only what the bridge confirmed it took. An absent CEF function
     * makes the optional call return undefined WITHOUT throwing, and a rejected
     * payload returns false, so caching before the call would record an
     * undelivered payload as sent and leave the strip empty until some
     * unrelated HUD change happened to produce a different payload. Leaving the
     * cache unset instead means the next HUD refresh retries on its own.
     */
    let delivered = false;
    try {
      delivered = window.ghostexGpui?.postGlobalActions?.(payload) === true;
    } catch {
      /*
       * The strip is presentation-only. Keep this runtime authoritative and do
       * not log raw payloads or invent native state.
       */
      delivered = false;
    }
    this.postedGlobalActionsPayload = delivered ? payload : undefined;
  }

  private postGpuiStatusPetState(): void {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    const candidates = createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups(
      this.latestGroups,
    );
    const statusPayload = createGpuiSessionStatusIndicatorsPayload(candidates, settings);
    const petPayload = createGpuiPetOverlayStatePayload(candidates, settings);
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
    GPUI status indicators and the pet overlay consume the same saved shared Settings object as SidebarApp hydrate. Publish only bounded counts, booleans, pet id, and sidebar-projected project/session ids/titles through fixed bridge functions.

    CDXC:GPUIStatusPetOverlay 2026-06-27-20:11:
    The standalone GPUI floating session indicator was removed. Keep posting
    status counts/projects for the menu bar and pet badge surfaces, but do not
    include floating visibility or floating size settings in the status payload.
    */
    try {
      window.ghostexGpui?.postSessionStatusIndicators?.(JSON.stringify(statusPayload));
      window.ghostexGpui?.postPetOverlayState?.(JSON.stringify(petPayload));
    } catch {
      /*
      CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
      The status/pet bridge is presentation-only. If CEF has not installed the fixed functions or rejects a payload, keep SidebarApp state authoritative and avoid fallback UI state, raw JSON logging, project/path/title side channels, or invented native indicators.
      */
    }
  }

  private createHydrateMessage(
    groups: SidebarSessionGroup[],
    hud: SidebarHudState,
  ): SidebarHydrateMessage {
    return {
      groups,
      hud,
      pinnedPrompts: [...this.appUserData.pinnedPrompts],
      previousSessions: [],
      remoteSidebarProjectCollectionsByMachineId:
        this.remoteSidebarProjectCollectionsByMachineId(),
      revision: ++this.revision,
      scratchPadContent: this.appUserData.scratchPadContent,
      type: "hydrate",
    };
  }

  private remoteSidebarProjectCollectionsByMachineId(): Readonly<
    Record<string, GxserverSidebarProjectCollectionsState>
  > {
    const result: Record<string, GxserverSidebarProjectCollectionsState> = {};
    const savedMachineIds = new Set(
      createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.map((machine) => machine.id),
    );
    for (const [machineId, snapshot] of this.remoteLastSeenPresentations) {
      if (
        savedMachineIds.has(machineId) &&
        isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)
      ) {
        result[machineId] = snapshot.sidebarProjectCollections;
      }
    }
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (
        savedMachineIds.has(machineId) &&
        isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)
      ) {
        result[machineId] = snapshot.sidebarProjectCollections;
      }
    }
    return result;
  }

  private createSidebarGroups(presentation: GxserverPresentationSnapshot): SidebarSessionGroup[] {
    this.refreshCloseAfterDoneTimers();
    this.pruneWorkspaceGroupAssignments(presentation);
    const projectProjection = createGpuiPresentationProjectProjectionMetadata({
      domainProjects: this.domainProjects,
      presentation,
      recentProjects: this.recentProjects,
      projectOrder: this.workspaceGroups.projectOrder,
    });
    this.ensureActiveProject(presentation, projectProjection);
    const subgroupHiddenSessionKeys = this.collectWorkspaceSubgroupSessionKeys(presentation);
    const hiddenSessionKeys =
      subgroupHiddenSessionKeys.size > 0
        ? new Set([...this.localFirstHiddenPresentationSessionKeys, ...subgroupHiddenSessionKeys])
        : this.localFirstHiddenPresentationSessionKeys;
    const projectGroups = createGxserverPresentationSidebarGroups({
      activeProjectId: this.activeProjectId,
      chatProjectIds: projectProjection.chatProjectIds,
      focusedSessionId: this.focusedSessionId,
      hiddenProjectIds: projectProjection.hiddenProjectIds,
      hiddenSessionKeys,
      presentation,
      projectOverlays: projectProjection.projectOverlays,
      resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      resolveCloseAfterDone: (projectId, sessionId) =>
        this.getCloseAfterDoneProjection(
          createGxserverPresentationProjectSessionId(projectId, sessionId),
        ),
      resolveDelayedSend: (projectId, sessionId) =>
        this.getDelayedSendProjection(
          createGxserverPresentationProjectSessionId(projectId, sessionId),
        ),
      resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
      visibleSessionIds: this.visibleSessionIds,
    }).map((group) => {
      const projectId = group.projectContext?.editor.projectId;
      if (!projectId) {
        return group;
      }
      const browserSessions = this.browserTabs
        .filter((tab) => tab.projectId === projectId)
        .map((tab, index): SidebarSessionItem => ({
          activity: "idle",
          agentIcon: "browser",
          alias: tab.title,
          column: index % GRID_COLUMN_COUNT,
          displayTitle: tab.title,
          isFocused: tab.isActive && this.activeProjectId === projectId,
          isLive: !tab.isSleeping,
          isRunning: !tab.isSleeping,
          isSleeping: tab.isSleeping,
          isVisible: tab.isVisible && this.activeProjectId === projectId,
          kind: "browser",
          lifecycleState: tab.isSleeping ? "sleeping" : "running",
          nativePaneState: tab.isSleeping ? "unmounted" : "mounted",
          primaryTitle: tab.title,
          row: Math.floor(index / GRID_COLUMN_COUNT),
          sessionId: gpuiBrowserSidebarSessionId(tab),
          sessionKind: "browser",
          shortcutLabel: "",
        }));
      if (browserSessions.length === 0) {
        return group;
      }
      const sessions = [...browserSessions, ...group.sessions];
      const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
      return {
        ...group,
        layoutVisibleCount: visibleCount,
        sessions,
        visibleCount,
      };
    });
    const groups = this.spliceWorkspaceSubgroups(projectGroups, presentation, projectProjection);

    if (!this.activeGroupId) {
      this.activeGroupId =
        groups.find((group) => group.isActive)?.groupId ??
        groups.find((group) => group.projectContext)?.groupId ??
        groups.find((group) => group.isChatCollection)?.groupId;
    }

    const localGroups = groups.map((group) => {
      const isActiveGroup = group.groupId === this.activeGroupId;
      const browserOwnsFocus =
        isActiveGroup &&
        group.sessions.some((session) => session.sessionKind === "browser" && session.isFocused);
      return {
        ...group,
        isActive: isActiveGroup,
        sessions: group.sessions.map((session) => ({
          ...session,
          ...(session.sessionKind === "t3"
            ? {
                alias: gpuiAgentGuiTitle(session.alias),
                displayTitle: gpuiAgentGuiTitle(session.displayTitle),
                displayTitleTooltip: gpuiAgentGuiTitle(session.displayTitleTooltip),
                primaryTitle: gpuiAgentGuiTitle(session.primaryTitle),
                terminalTitle: gpuiAgentGuiTitle(session.terminalTitle),
              }
            : {}),
          isFocused:
            isActiveGroup &&
            (session.sessionKind === "browser"
              ? session.isFocused
              : !browserOwnsFocus &&
                this.focusedSessionId ===
                  parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId),
          /*
          GPUI terminal visibility is owned by the native workspace callback.
          Do not preserve the shared projection's first-row fallback here:
          pinned sessions sort first and would otherwise look surfaced without
          owning a pane. Browser rows keep their separate browser-pane state.
          */
          isVisible:
            isActiveGroup &&
            (session.sessionKind === "browser"
              ? session.isVisible
              : this.visibleSessionIds.has(
                  parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId ??
                    session.sessionId,
                )),
        })),
      };
    });
    return this.overlayProjectDiffStats([
      ...this.withQuickAutomationsOverviewGroup(localGroups),
      ...this.createRemoteSidebarGroups(),
    ]);
  }

  private withQuickAutomationsOverviewGroup(groups: SidebarSessionGroup[]): SidebarSessionGroup[] {
    if (!this.quickAutomationsOverviewOpen) {
      return groups;
    }
    const quickSession = this.createQuickAutomationsSidebarSession();
    const nextGroups = groups.map((group) => {
      if (group.groupId !== GPUI_GXSERVER_CHATS_GROUP_ID) {
        return group;
      }
      const sessions = [
        quickSession,
        ...group.sessions.filter(
          (session) => !this.isQuickAutomationsSidebarSessionId(session.sessionId),
        ),
      ].map((session, index) => ({ ...session, column: index % GRID_COLUMN_COUNT, row: index }));
      const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
      return {
        ...group,
        isActive: this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID || group.isActive,
        layoutVisibleCount: visibleCount,
        sessions,
        visibleCount,
      };
    });
    if (nextGroups.some((group) => group.groupId === GPUI_GXSERVER_CHATS_GROUP_ID)) {
      return nextGroups;
    }
    const sessions = [quickSession];
    const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
    return [
      {
        groupId: GPUI_GXSERVER_CHATS_GROUP_ID,
        isActive: this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
        isChatCollection: true,
        isFocusModeActive: false,
        kind: "workspace",
        layoutVisibleCount: visibleCount,
        sessions,
        title: "Chats",
        viewMode: "grid",
        visibleCount,
      },
      ...groups,
    ];
  }

  private createQuickAutomationsSidebarSession(): SidebarSessionItem {
    const sessionId = this.quickAutomationsSidebarSessionId();
    const isActive = this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID;
    /*
    CDXC:GPUIAutomationsOverview 2026-07-08:
    Mirror macOS `createQuickAutomationsSidebarSession` and
    `isQuickAutomationsSidebarReference`: the overview is one synthetic Quick
    row named Automations Overview, scoped to project id `quick-automations`,
    and removed from the session-local runtime projection when closed.
    */
    return {
      activity: "idle",
      alias: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      column: 0,
      detail: "All projects",
      displayTitle: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      isFocused: isActive && this.focusedSessionId === GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
      isLive: false,
      isRunning: false,
      isVisible: isActive,
      lifecycleState: "done",
      nativePaneState: "unmounted",
      primaryTitle: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      providerSessionState: "missing",
      row: 0,
      sessionId,
      shortcutLabel: "",
    };
  }

  private quickAutomationsSidebarSessionId(): string {
    return createGxserverPresentationProjectSessionId(
      GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
      GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
    );
  }

  private isQuickAutomationsSidebarSessionId(sessionId: string): boolean {
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    return (
      reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID &&
      reference.sessionId === GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID
    );
  }

  private createQuickAutomationsProjectContext(): NonNullable<
    SidebarSessionGroup["projectContext"]
  > {
    return {
      canRemoveProject: false,
      editor: {
        diffStats: createDefaultSidebarProjectDiffStats(),
        isOpen: true,
        isSleeping: false,
        projectId: GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
        status: "running",
      },
      path: "",
    };
  }

  private activeProjectContextGroups(): SidebarSessionGroup[] {
    if (
      !this.quickAutomationsOverviewOpen ||
      this.activeProjectId !== GPUI_QUICK_AUTOMATIONS_PROJECT_ID
    ) {
      return this.latestGroups;
    }
    return this.latestGroups.map((group) =>
      group.groupId === GPUI_GXSERVER_CHATS_GROUP_ID
        ? {
            ...group,
            projectContext: this.createQuickAutomationsProjectContext(),
            title: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
          }
        : group,
    );
  }

  private overlayProjectDiffStats(groups: SidebarSessionGroup[]): SidebarSessionGroup[] {
    // Mirrors the macOS pre-publish overlay: header +/- counts come from the
    // background numstat loop, keyed by the projection's editor project id
    // (plain local ids, machine-scoped remote ids).
    return groups.map((group) => {
      const projectContext = group.projectContext;
      if (!projectContext) {
        return group;
      }
      const stats = this.projectDiffStatsByProjectId.get(projectContext.editor.projectId);
      if (!stats) {
        return group;
      }
      return {
        ...group,
        projectContext: {
          ...projectContext,
          editor: { ...projectContext.editor, diffStats: stats },
        },
      };
    });
  }

  private pruneWorkspaceGroupAssignments(presentation: GxserverPresentationSnapshot): void {
    let next = this.workspaceGroups;
    for (const project of presentation.projects) {
      if (!next.projects[project.projectId]) {
        continue;
      }
      const existingSessionIds = new Set(
        presentation.sessions
          .filter((session) => session.projectId === project.projectId)
          .map((session) => session.sessionId),
      );
      next = pruneGpuiWorkspaceSessionSubgroups(next, project.projectId, existingSessionIds);
    }
    if (next !== this.workspaceGroups) {
      this.workspaceGroups = next;
      this.persistWorkspaceGroups();
    }
  }

  private pruneRemoteWorkspaceGroupAssignments(
    machineId: string,
    snapshot: GxserverPresentationSnapshot,
  ): void {
    let next = this.workspaceGroups;
    for (const project of snapshot.projects) {
      const scopedProjectId = createGpuiRemotePresentationProjectId(machineId, project.projectId);
      if (!next.projects[scopedProjectId]) {
        continue;
      }
      const existingSessionIds = new Set(
        snapshot.sessions
          .filter((session) => session.projectId === project.projectId)
          .map((session) => session.sessionId),
      );
      next = pruneGpuiWorkspaceSessionSubgroups(next, scopedProjectId, existingSessionIds);
    }
    if (next !== this.workspaceGroups) {
      this.workspaceGroups = next;
      this.persistWorkspaceGroups();
    }
  }

  private collectWorkspaceSubgroupSessionKeys(
    presentation: GxserverPresentationSnapshot,
  ): Set<string> {
    const keys = new Set<string>();
    for (const project of presentation.projects) {
      for (const subgroup of getGpuiWorkspaceSessionSubgroups(
        this.workspaceGroups,
        project.projectId,
      )) {
        for (const sessionId of subgroup.sessionIds) {
          keys.add(createGxserverPresentationSidebarSessionKey(project.projectId, sessionId));
        }
      }
    }
    return keys;
  }

  private spliceWorkspaceSubgroups(
    groups: SidebarSessionGroup[],
    presentation: GxserverPresentationSnapshot,
    projectProjection: GpuiPresentationProjectProjectionMetadata,
  ): SidebarSessionGroup[] {
    const projectsById = new Map(
      presentation.projects.map((project) => [project.projectId, project]),
    );
    const sessionsByProject = new Map<string, Map<string, GxserverPresentationSession>>();
    for (const session of presentation.sessions) {
      const byId = sessionsByProject.get(session.projectId) ?? new Map();
      byId.set(session.sessionId, session);
      sessionsByProject.set(session.projectId, byId);
    }
    const result: SidebarSessionGroup[] = [];
    for (const group of groups) {
      const projectId = parseGxserverPresentationProjectGroupId(group.groupId);
      if (!projectId || projectProjection.chatProjectIds.has(projectId)) {
        result.push(group);
        continue;
      }
      result.push({ ...group, canCreateSessionGroup: true });
      const subgroups = getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, projectId);
      if (subgroups.length === 0) {
        continue;
      }
      const project = projectsById.get(projectId);
      if (!project) {
        continue;
      }
      const rowsById = sessionsByProject.get(projectId) ?? new Map();
      for (const subgroup of subgroups) {
        const memberRows = subgroup.sessionIds
          .map((sessionId) => rowsById.get(sessionId))
          .filter((row): row is GxserverPresentationSession => row !== undefined);
        const subgroupSidebarId = createGpuiWorkspaceSessionSubgroupId(projectId, subgroup.groupId);
        const built = createGxserverPresentationSidebarGroup({
          activeProjectId: this.activeProjectId,
          canRemoveProject: false,
          createProjectGroupId: () => subgroupSidebarId,
          focusedSessionId: this.focusedSessionId,
          project,
          resolveAgentIcon: resolveGpuiSidebarAgentIcon,
          resolveCloseAfterDone: (resolvedProjectId, sessionId) =>
            this.getCloseAfterDoneProjection(
              createGxserverPresentationProjectSessionId(resolvedProjectId, sessionId),
            ),
          resolveDelayedSend: (resolvedProjectId, sessionId) =>
            this.getDelayedSendProjection(
              createGxserverPresentationProjectSessionId(resolvedProjectId, sessionId),
            ),
          resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
          sessions: memberRows,
          visibleSessionIds: this.visibleSessionIds,
        });
        result.push({
          ...built,
          canCreateSessionGroup: true,
          canFocusMode: false,
          groupId: subgroupSidebarId,
          kind: "workspace",
          projectContext: undefined,
          title: subgroup.title,
        });
      }
    }
    return result;
  }

  private createRemoteSidebarGroups(): SidebarSessionGroup[] {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    /*
    CDXC:GPUIRemoteLastSeen 2026-07-12:
    Disconnected machines keep rendering their last-seen presentation as
    stale (faded, non-interactive terminals) instead of disappearing, so the
    user still sees which projects and sessions live on the machine and can
    keep using its local browser tabs. Live presentations refresh the
    client-persisted last-seen copy; machines with only a last-seen copy
    render with `isStale`.
    */
    this.captureRemoteLastSeenPresentations();
    const savedMachineIds = new Set(settings.remoteMachines.map((machine) => machine.id));
    const presentationsByMachineId = new Map(this.remotePresentations);
    const staleMachineIds = new Set<string>();
    for (const [machineId, snapshot] of this.remoteLastSeenPresentations) {
      if (presentationsByMachineId.has(machineId) || !savedMachineIds.has(machineId)) {
        continue;
      }
      presentationsByMachineId.set(machineId, snapshot);
      staleMachineIds.add(machineId);
    }
    const groups = createGpuiRemotePresentationSidebarGroups({
      activeGroupId: this.activeGroupId,
      focusedSessionId: this.focusedSessionId,
      presentationsByMachineId,
      remoteGroupOrderByMachineId: this.remoteGroupOrderByMachineId,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      resolveCloseAfterDone: (machineId, projectId, sessionId) =>
        this.getCloseAfterDoneProjection(
          createGpuiRemotePresentationSessionId(machineId, projectId, sessionId),
        ),
      settings,
      visibleSessionIds: this.visibleSessionIds,
    });
    return groups.flatMap((group) => {
      const expanded = this.expandRemoteSidebarGroup(group);
      const machineId = group.remoteMachineContext?.machineId;
      if (!machineId || !staleMachineIds.has(machineId)) {
        return expanded;
      }
      return expanded.map((expandedGroup) => ({ ...expandedGroup, isStale: true }));
    });
  }

  private captureRemoteLastSeenPresentations(): void {
    let changed = false;
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (this.remoteLastSeenPresentations.get(machineId) !== snapshot) {
        this.remoteLastSeenPresentations.set(machineId, snapshot);
        changed = true;
      }
    }
    if (!changed) {
      return;
    }
    if (this.remoteLastSeenPersistTimeoutId !== undefined) {
      return;
    }
    this.remoteLastSeenPersistTimeoutId = window.setTimeout(() => {
      this.remoteLastSeenPersistTimeoutId = undefined;
      writeStoredGpuiRemoteLastSeenPresentations(this.remoteLastSeenPresentations);
    }, GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_PERSIST_DELAY_MS);
  }

  /*
  CDXC:GPUIRemoteSidebarParity 2026-07-12:
  Remote project groups reuse the local sidebar overlays instead of a reduced
  remote feature set: machine-scoped browser tabs splice in as browser session
  rows, and the client-owned named session groups overlay applies to remote
  projects through their machine-scoped project ids.
  */
  private expandRemoteSidebarGroup(group: SidebarSessionGroup): SidebarSessionGroup[] {
    const remoteGroup = parseGpuiRemotePresentationGroupId(group.groupId);
    if (!remoteGroup) {
      return [group];
    }
    const scopedProjectId = createGpuiRemotePresentationProjectId(
      remoteGroup.machineId,
      remoteGroup.projectId,
    );
    return this.spliceRemoteWorkspaceSubgroups(
      this.withRemoteBrowserTabSessions(group, scopedProjectId),
      scopedProjectId,
    );
  }

  private withRemoteBrowserTabSessions(
    group: SidebarSessionGroup,
    scopedProjectId: string,
  ): SidebarSessionGroup {
    const browserSessions = this.browserTabs
      .filter((tab) => tab.projectId === scopedProjectId)
      .map((tab, index): SidebarSessionItem => ({
        activity: "idle",
        agentIcon: "browser",
        alias: tab.title,
        column: index % GRID_COLUMN_COUNT,
        displayTitle: tab.title,
        isFocused: tab.isActive && this.activeGroupId === group.groupId,
        isLive: !tab.isSleeping,
        isRunning: !tab.isSleeping,
        isSleeping: tab.isSleeping,
        isVisible: tab.isVisible && this.activeGroupId === group.groupId,
        kind: "browser",
        lifecycleState: tab.isSleeping ? "sleeping" : "running",
        nativePaneState: tab.isSleeping ? "unmounted" : "mounted",
        primaryTitle: tab.title,
        row: Math.floor(index / GRID_COLUMN_COUNT),
        sessionId: gpuiBrowserSidebarSessionId(tab),
        sessionKind: "browser",
        shortcutLabel: "",
      }));
    if (browserSessions.length === 0) {
      return group;
    }
    const sessions = [...browserSessions, ...group.sessions];
    const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
    return {
      ...group,
      layoutVisibleCount: visibleCount,
      sessions,
      visibleCount,
    };
  }

  private spliceRemoteWorkspaceSubgroups(
    group: SidebarSessionGroup,
    scopedProjectId: string,
  ): SidebarSessionGroup[] {
    const subgroups = getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, scopedProjectId);
    if (subgroups.length === 0) {
      return [group];
    }
    const sessionsByRawId = new Map<string, SidebarSessionItem>();
    for (const session of group.sessions) {
      const reference = parseGpuiRemotePresentationSessionId(session.sessionId);
      if (reference) {
        sessionsByRawId.set(reference.sessionId, session);
      }
    }
    const claimedRawIds = new Set<string>();
    const subgroupGroups = subgroups.map((subgroup) => {
      const members = subgroup.sessionIds.flatMap((rawSessionId) => {
        const session = sessionsByRawId.get(rawSessionId);
        if (!session) {
          return [];
        }
        claimedRawIds.add(rawSessionId);
        return [session];
      });
      const subgroupSidebarId = createGpuiWorkspaceSessionSubgroupId(
        scopedProjectId,
        subgroup.groupId,
      );
      const sessions = relayoutGpuiSidebarSessions(members);
      const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
      return {
        ...group,
        canCreateSessionGroup: true,
        canFocusMode: false,
        groupId: subgroupSidebarId,
        isActive: this.activeGroupId === subgroupSidebarId,
        kind: "workspace" as const,
        layoutVisibleCount: visibleCount,
        projectContext: undefined,
        sessions,
        title: subgroup.title,
        visibleCount,
      };
    });
    const remaining = relayoutGpuiSidebarSessions(
      group.sessions.filter((session) => {
        const reference = parseGpuiRemotePresentationSessionId(session.sessionId);
        return !reference || !claimedRawIds.has(reference.sessionId);
      }),
    );
    const visibleCount = visibleCountForGxserverPresentationSidebarSessions(remaining);
    return [
      {
        ...group,
        layoutVisibleCount: visibleCount,
        sessions: remaining,
        visibleCount,
      },
      ...subgroupGroups,
    ];
  }

  private ensureActiveProject(
    presentation: GxserverPresentationSnapshot,
    projectProjection: GpuiPresentationProjectProjectionMetadata,
  ): void {
    const projectIds = new Set(presentation.projects.map((project) => project.projectId));
    if (
      this.quickAutomationsOverviewOpen &&
      this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID
    ) {
      if (this.activeGroupId !== GPUI_GXSERVER_CHATS_GROUP_ID) {
        this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
      }
      return;
    }
    if (this.focusedSessionId) {
      /*
      CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:22:
      Re-clicking a local session in the GPUI sidebar must keep behaving like the macOS app: the focused terminal owns the active project. Bootstrap can replay a stale initial project beside the current focused session, so resolve the session from the fresh presentation snapshot before rendering groups.
      */
      const focusedProjectId = presentation.sessions.find(
        (session) => session.sessionId === this.focusedSessionId,
      )?.projectId;
      if (
        focusedProjectId &&
        projectIds.has(focusedProjectId) &&
        !projectProjection.hiddenProjectIds.has(focusedProjectId)
      ) {
        const focusedGroupId = projectProjection.chatProjectIds.has(focusedProjectId)
          ? GPUI_GXSERVER_CHATS_GROUP_ID
          : (this.workspaceSubgroupSidebarIdForSession(focusedProjectId, this.focusedSessionId) ??
            createGxserverPresentationProjectGroupId(focusedProjectId));
        if (this.activeProjectId !== focusedProjectId || this.activeGroupId !== focusedGroupId) {
          this.activeProjectId = focusedProjectId;
          this.activeGroupId = focusedGroupId;
          this.refreshSidebarHudFromClient();
        }
        return;
      }
    }
    if (
      this.activeProjectId &&
      projectIds.has(this.activeProjectId) &&
      !projectProjection.hiddenProjectIds.has(this.activeProjectId)
    ) {
      if (projectProjection.chatProjectIds.has(this.activeProjectId)) {
        if (this.activeGroupId !== GPUI_GXSERVER_CHATS_GROUP_ID) {
          this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
          this.refreshSidebarHudFromClient();
        }
        return;
      }
      return;
    }
    const firstProject = presentation.projects.find(
      (project) =>
        !projectProjection.hiddenProjectIds.has(project.projectId) &&
        !projectProjection.chatProjectIds.has(project.projectId),
    );
    if (firstProject) {
      this.focusProjectId(firstProject.projectId);
      return;
    }
    this.activeProjectId = undefined;
    this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
    this.refreshSidebarHudFromClient();
  }

  private async handleSidebarMessage(message: SidebarToExtensionMessage): Promise<void> {
    switch (message.type) {
      case "sidebarDebugLog":
        window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
          details: message.details,
          event: message.event,
          scenarioId: message.scenarioId,
          type: "sidebarDiagnosticLog",
        });
        return;
      case "focusGroup":
        this.focusGroup(message.groupId, message);
        return;
      case "focusSession":
        await this.focusSession(message.sessionId, message);
        return;
      case "focusSessionMode":
        if (parseGpuiRemotePresentationSessionId(message.sessionId)) {
          await this.focusSession(message.sessionId, message);
          return;
        }
        this.handleUnsupportedSidebarMessage(message);
        return;
      case "createSession":
        await this.createSession();
        return;
      case "createSessionInGroup":
        await this.createSession(message.groupId);
        return;
      case "createProjectTerminal":
        await this.createProjectTerminal(message.groupId);
        return;
      case "createChat":
        await this.createQuickTerminal();
        return;
      case "openBrowserChat":
        this.openQuickBrowserTab();
        return;
      case "openBrowserPaneInGroup":
        this.openBrowserPaneInGroup(message.groupId);
        return;
      case "runSidebarAgent":
        if (message.groupId === GPUI_GXSERVER_CHATS_GROUP_ID) {
          await this.createQuickAgentSession(message.agentId);
          return;
        }
        await this.createAgentSession(message.agentId, message.groupId);
        return;
      case "runSidebarCommand": {
        /*
        CDXC:GPUICommandPane 2026-06-26-05:22:
        Runtime command-pane messages can arrive from untyped CEF/renderer boundaries. Reject missing, non-string, or blank command ids before Action lookup so unsafe extra launch fields cannot make the selector path throw or reach the fixed command-action bridge.
        */
        const commandId = normalizeNonEmptyString(message.commandId);
        if (!commandId) {
          this.handleUnsupportedSidebarMessage(message);
          return;
        }
        /*
        CDXC:GlobalActions 2026-08-07:
        Project rows can run either list, so the renderer names the scope.
        Validate the value like runMode rather than trusting the annotation: an
        unrecognized scope is an unsupported no-op, never a silent fallback to
        the project list, which would run an Action the user did not click. An
        absent scope stays project, which is what every sender that predates
        Global Actions sends.
        */
        if (message.scope !== undefined && !isSidebarCommandScope(message.scope)) {
          this.handleUnsupportedSidebarMessage(message);
          return;
        }
        this.runSidebarCommand(commandId, message, message.scope ?? "project");
        return;
      }
      case "runGhostexHotkeyAction": {
        this.postGhostexHotkeyAction(message);
        return;
      }
      case "endSidebarCommandRun": {
        /*
        CDXC:GPUICommandPane 2026-06-26-05:22:
        Closing a command-pane Action run is command-id-only. Validate the selector at the runtime boundary so malformed renderer messages with command text, URLs, paths, cwd/env, logs, or output are unsupported no-ops instead of crashing before the run-end bridge can decline them.
        */
        const commandId = normalizeNonEmptyString(message.commandId);
        if (!commandId) {
          this.handleUnsupportedSidebarMessage(message);
          return;
        }
        this.endSidebarCommandRun(commandId, message);
        return;
      }
      case "setSessionSleeping":
        await this.setSessionSleeping(message.sessionId, message.sleeping);
        return;
      case "setSessionsSleeping":
        await this.setSessionsSleeping(message.sessionIds, message.sleeping);
        return;
      case "setGroupSleeping":
        await this.setGroupSleeping(message.groupId, message.sleeping);
        return;
      case "closeSession":
        await this.transitionSession(message.sessionId, "close");
        return;
      case "closeSessions":
        await Promise.all(
          message.sessionIds.map((sessionId) => this.transitionSession(sessionId, "close")),
        );
        return;
      case "copySessionDetails":
        this.copySessionDetails(message);
        return;
      case "fullReloadSession":
      case "restartSession":
        await this.fullReloadSession(message.sessionId);
        return;
      case "fullReloadProjectZmxSessions":
        await this.fullReloadProjectZmxSessions(message.groupId);
        return;
      case "fullReloadGroup":
        await this.fullReloadWorkspaceGroup(message.groupId);
        return;
      case "toggleCloseAfterDone":
        this.toggleCloseAfterDone(message.sessionId);
        return;
      case "requestT3SessionBrowserAccess":
        this.requestT3SessionBrowserAccess(message.sessionId);
        return;
      case "openAutomationsPage":
        /*
        CDXC:GPUIAutomationsOverview 2026-07-08:
        Mirror macOS `openQuickAutomationsPage`, `ensureQuickAutomationsProject`,
        and `focusQuickAutomationsProject` from native/sidebar/native-sidebar.tsx:
        create the session-local Quick overview row, focus it, and let the
        existing active-project context post carry the Automate workarea identity.
        */
        this.openQuickAutomationsPage();
        return;
      case "closeInactiveProjectSessions":
        await this.closeInactiveProjectSessions(message.groupId);
        return;
      case "sleepInactiveProjectSessions":
        await this.sleepInactiveProjectSessions(message.groupId);
        return;
      case "wakeProjectSleepingSessions":
        await this.wakeProjectSleepingSessions(message.groupId);
        return;
      case "forkSession":
        await this.forkSession(message.sessionId);
        return;
      case "renameSession":
        await this.renameSession(message);
        return;
      case "setSessionFavorite":
        await this.updateSessionFlags(message.sessionId, {
          isFavorite: message.favorite,
          sessionTag: message.favorite ? "favorite" : null,
        });
        return;
      case "setSessionTag":
        await this.updateSessionFlags(message.sessionId, {
          isFavorite: message.sessionTag === "favorite",
          sessionTag: message.sessionTag ?? null,
        });
        return;
      case "setSessionPinned":
        await this.updateSessionFlags(message.sessionId, {
          isPinned: message.pinned,
        });
        return;
      /*
      CDXC:SidebarV2Lifecycle 2026-07-29:
      Sidebar V2's settle/snooze commands map 1:1 onto gxserver endpoints. They
      are remote-allowed, so they route through the same machine resolution
      every other session mutation uses; the client posts no optimistic patch
      because the endpoints answer with a presentation delta and enforce guards
      (a working or blocked session cannot settle) that the client must not
      pre-empt.
      */
      case "settleSession":
        await this.runSessionLifecycleCommand(message.sessionId, "/api/settleSession", {});
        return;
      case "unsettleSession":
        await this.runSessionLifecycleCommand(message.sessionId, "/api/unsettleSession", {});
        return;
      case "snoozeSession":
        await this.runSessionLifecycleCommand(message.sessionId, "/api/snoozeSession", {
          snoozedUntil: message.snoozedUntil,
        });
        return;
      case "unsnoozeSession":
        await this.runSessionLifecycleCommand(message.sessionId, "/api/unsnoozeSession", {});
        return;
      case "syncSessionOrder":
        if (parseGpuiWorkspaceSessionSubgroupId(message.groupId)) {
          this.syncWorkspaceSubgroupSessionOrder(message.groupId, message.sessionIds);
          return;
        }
        await this.syncSessionOrder(message.groupId, message.sessionIds);
        return;
      case "createGroup":
        this.createWorkspaceGroup(message.groupId);
        return;
      case "createGroupFromSession":
        this.createWorkspaceGroupFromSession(message.sessionId);
        return;
      case "renameGroup":
        this.renameWorkspaceGroup(message.groupId, message.title);
        return;
      case "closeGroup":
        await this.closeWorkspaceGroup(message.groupId);
        return;
      case "moveSessionToGroup":
        this.moveSessionToWorkspaceGroup(message);
        return;
      case "syncGroupOrder":
        this.syncWorkspaceGroupOrder(message.groupIds);
        return;
      case "updateSidebarProjectCollections":
        if (message.remoteMachineId) {
          await this.updateRemoteSidebarProjectCollections(
            message.remoteMachineId,
            message.state,
          );
          return;
        }
        this.queueSidebarProjectCollectionsServerSync(message.state);
        return;
      case "requestPreviousSessions":
        await this.requestPreviousSessions(message);
        return;
      case "searchPreviousSessionsByText":
        await this.searchPreviousSessionsByText();
        return;
      case "restorePreviousSession":
        await this.restorePreviousSession(message.historyId);
        return;
      case "deletePreviousSession":
        await this.deletePreviousSession(message.historyId);
        return;
      case "copyAttachCommand": {
        const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
        if (remoteSession) {
          this.postRemoteSessionNativeAction("copyRemoteAttachCommand", remoteSession, message);
          return;
        }
        this.handleUnsupportedSidebarMessage(message);
        return;
      }
      case "copyResumeCommand": {
        const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
        if (remoteSession) {
          this.postRemoteSessionNativeAction("copyRemoteResumeCommand", remoteSession, message);
          return;
        }
        this.handleUnsupportedSidebarMessage(message);
        return;
      }
      case "requestProjectWorktrees":
        await this.requestProjectWorktrees(message);
        return;
      case "saveScratchPad":
        await this.saveScratchPad(message.content);
        return;
      case "savePinnedPrompt":
        await this.savePinnedPrompt(message);
        return;
      case "createProjectWorktree":
        await this.createProjectWorktree(message);
        return;
      case "createWorktreeSession":
        await this.createWorktreeSession(message);
        return;
      case "removeSessionWorktree":
        await this.removeSessionWorktree(message);
        return;
      case "promptDeleteWorktreeForGroup":
        await this.promptDeleteWorktreeForGroup(message.groupId);
        return;
      case "confirmDeleteWorktree":
        await this.confirmDeleteWorktree(message);
        return;
      case "updateSettingsPatch":
        this.saveSidebarSettingsPatch(message);
        return;
      case "openExternalUrl":
        this.openExternalUrl(message);
        return;
      case "openSettings":
        this.openAppModal("settings");
        return;
      case "openWorkspaceWelcome":
        this.openAppModal("firstLaunchSetup");
        return;
      case "openHighlightedFeatures":
      case "openGhostexTutorialVideo":
        this.openAppModal("watchGhostexVideo");
        return;
      case "reconnectRemoteMachine":
        this.reconnectRemoteMachine(message.remoteMachineId, message.installApproved === true);
        return;
      case "openRemoteCloneRepository":
        this.openRemoteCloneRepository(message.remoteMachineId);
        return;
      case "pickWorkspaceFolder":
        this.pickWorkspaceFolder(message);
        return;
      case "removeProject":
        await this.removeProject(message.projectId);
        return;
      case "restoreRecentProject":
        await this.restoreRecentProject(message.projectId);
        return;
      case "removeRecentProject":
        await this.removeRecentProject(message.projectId);
        return;
      case "copyRecentProjectPath":
        {
          const remoteProject = parseGpuiRemotePresentationProjectId(message.projectId);
          if (remoteProject) {
            this.postRemoteProjectNativeAction("copyRemoteProjectPath", remoteProject, message);
            return;
          }
        }
        this.postNativeProjectPathAction("copyRecentProjectPath", message.projectId, message);
        return;
      case "openRecentProjectInFinder":
        {
          const remoteProject = parseGpuiRemotePresentationProjectId(message.projectId);
          if (remoteProject) {
            this.postRemoteProjectNativeAction(
              "copyRemoteProjectOpenFolderCommand",
              remoteProject,
              message,
            );
            return;
          }
        }
        this.postNativeProjectPathAction("openRecentProjectInFinder", message.projectId, message);
        return;
      case "closeWorkspaceProjectForGroup":
        await this.closeProjectForGroup(message.groupId);
        return;
      case "copyWorkspaceProjectPathForGroup":
        this.postProjectPathActionForGroup("copyWorkspaceProjectPath", message.groupId, message);
        return;
      case "openWorkspaceProjectInFinderForGroup":
        this.postProjectPathActionForGroup(
          "openWorkspaceProjectInFinder",
          message.groupId,
          message,
        );
        return;
      case "openWorkspaceProjectInIdeForGroup":
        this.postProjectPathActionForGroup("openWorkspaceProjectInIde", message.groupId, message);
        return;
      case "openActiveWorkspaceProjectInFinder":
        this.postActiveProjectPathAction("openActiveWorkspaceProjectInFinder", message);
        return;
      case "openActiveWorkspaceProjectInIde":
        if (message.targetApp !== "vscode" && message.targetApp !== "zed") {
          this.handleUnsupportedSidebarMessage(message);
          return;
        }
        this.postActiveProjectPathAction(
          message.targetApp === "vscode"
            ? "openActiveWorkspaceProjectInVscode"
            : "openActiveWorkspaceProjectInZed",
          message,
        );
        return;
      case "removeWorkspaceProjectForGroup":
        await this.removeProjectForGroup(message.groupId);
        return;
      case "setProjectWorktreeCommand":
        await this.updateProjectWorktreeCommand(message.projectId, message.command);
        return;
      case "setProjectBeadsDisplayKey":
        await this.updateProjectBeadsDisplayKey(message.projectId, message.displayKey);
        return;
      case "setProjectBeadsDirectory":
        await this.updateProjectBeadsDirectory(message.projectId, message.directory);
        return;
      case "setProjectDocsDirectory":
        await this.updateProjectDocsDirectory(message.projectId, message.directory);
        return;
      case "refreshGitState":
        await this.refreshGitStateForMessage(message);
        return;
      case "setSidebarGitPrimaryAction":
        await this.persistGitPreferences({ primaryAction: message.action }, message);
        return;
      case "setSidebarGitCommitConfirmationEnabled":
        await this.persistGitPreferences({ confirmCommit: message.enabled }, message);
        return;
      case "setSidebarGitGenerateCommitBodyEnabled":
        await this.persistGitPreferences({ generateCommitBody: message.enabled }, message);
        return;
      case "runSidebarGitAction":
        await this.runSidebarGitAction(message);
        return;
      case "confirmSidebarGitCommit":
        await this.confirmSidebarGitCommit(message);
        return;
      case "cancelSidebarGitCommit":
        this.pendingGitCommitRequests.delete(message.requestId);
        this.publishHudPatch();
        return;
      case "runSidebarGitMultipleCommits":
        await this.runSidebarGitMultipleCommits(message.requestId, message.agentId);
        return;
      case "confirmSidebarGitDirectMerge":
        await this.confirmSidebarGitDirectMerge(message);
        return;
      case "commitWorktreeBeforeDelete":
        await this.runSidebarGitAction({
          action: "commit",
          groupId: message.groupId,
          type: "runSidebarGitAction",
        });
        return;
      case "openSidebarGitChangedFileDiff":
        await this.openSidebarGitChangedFileDiff(message.filePath, message.requestId);
        return;
      case "openSidebarGitChangedFile":
        await this.openSidebarGitChangedFileInIde(message);
        return;
      case "saveSidebarAgent":
        await this.saveSidebarAgent(message);
        return;
      case "deleteSidebarAgent":
        await this.deleteSidebarAgent(message.agentId);
        return;
      case "syncSidebarAgentOrder":
        await this.syncSidebarAgentOrder(message.requestId, message.agentIds);
        return;
      case "saveSidebarCommand":
        await this.saveSidebarCommand(message);
        return;
      case "deleteSidebarCommand":
        await this.deleteSidebarCommand(message.commandId);
        return;
      case "syncSidebarCommandOrder":
        await this.syncSidebarCommandOrder(message.requestId, message.commandIds);
        return;
      case "saveGlobalSidebarCommand":
        await this.saveGlobalSidebarCommand(message);
        return;
      case "deleteGlobalSidebarCommand":
        await this.deleteGlobalSidebarCommand(message.commandId);
        return;
      case "syncGlobalSidebarCommandOrder":
        await this.syncGlobalSidebarCommandOrder(message.requestId, message.commandIds);
        return;
      default:
        this.handleUnsupportedSidebarMessage(message);
        return;
    }
  }

  private focusGroup(groupId: string, originalMessage: SidebarToExtensionMessage): void {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const target = this.selectRemoteGroupAttachTarget(remoteGroup);
      if (!target) {
        this.postRemoteToast("info", "Remote attach unavailable", {
          description: "This remote project has no attachable sessions.",
        });
        return;
      }
      if (
        this.postRemoteSessionNativeAction("openRemoteSessionTerminal", target, originalMessage)
      ) {
        this.setRemotePresentationSessionFocus(target);
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      if (!parseGpuiRemotePresentationProjectId(subgroup.projectId)) {
        this.activeProjectId = subgroup.projectId;
      }
      this.activeGroupId = groupId;
      this.refreshSidebarHudFromClient();
      if (this.presentation) {
        this.publishPresentation("patch");
      } else {
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (projectId) {
      this.focusProjectId(projectId);
    } else {
      this.activeGroupId = groupId;
      this.refreshSidebarHudFromClient();
    }
    this.publishPresentation("patch");
  }

  private openQuickAutomationsPage(): void {
    this.ensureQuickAutomationsProject();
    this.focusQuickAutomationsProject();
  }

  private ensureQuickAutomationsProject(): void {
    /*
    CDXC:GPUIAutomationsOverview 2026-07-08:
    GPUI mirrors macOS `ensureQuickAutomationsProject` without daemon storage:
    macOS writes a client registry row, while GPUI keeps this overview as a
    session-local runtime projection until its synthetic Quick row is closed.
    */
    this.quickAutomationsOverviewOpen = true;
  }

  private focusQuickAutomationsProject(): void {
    /*
    CDXC:GPUIAutomationsOverview 2026-07-08:
    Mirror macOS `focusQuickAutomationsProject`: selecting the synthetic
    quick-automations project activates the Quick group and focused overview row;
    Rust receives the Automate workarea through the active-project context post.
    */
    this.activeProjectId = GPUI_QUICK_AUTOMATIONS_PROJECT_ID;
    this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
    this.focusedSessionId = GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID;
    this.visibleSessionIds = new Set([
      ...this.visibleSessionIds,
      GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
    ]);
    if (this.presentation) {
      this.publishPresentation("patch");
      return;
    }
    this.postActiveProjectContext();
  }

  private closeQuickAutomationsProject(): void {
    this.quickAutomationsOverviewOpen = false;
    this.visibleSessionIds.delete(GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID);
    if (this.focusedSessionId === GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID) {
      this.focusedSessionId = undefined;
    }
    if (this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      this.activeProjectId = undefined;
      this.activeGroupId = undefined;
    }
    if (this.presentation) {
      this.publishPresentation("patch");
      return;
    }
    this.postActiveProjectContext();
  }

  private async focusSession(
    sessionId: string,
    originalMessage?: SidebarToExtensionMessage,
  ): Promise<void> {
    const browserTab = this.browserTabs.find(
      (candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId,
    );
    if (browserTab) {
      /*
      A Browser row becomes the presentation focus owner when clicked. Clear
      the previous terminal owner before publishing the project change;
      otherwise ensureActiveProject resolves that stale terminal and switches
      the sidebar straight back to the terminal's project.
      */
      this.focusedSessionId = undefined;
      const remoteBrowserProject = parseGpuiRemotePresentationProjectId(browserTab.projectId);
      if (remoteBrowserProject) {
        const remoteGroupId = createGpuiRemotePresentationGroupId(
          remoteBrowserProject.machineId,
          remoteBrowserProject.projectId,
        );
        if (this.activeGroupId !== remoteGroupId) {
          this.activeGroupId = remoteGroupId;
          if (this.presentation) {
            this.publishPresentation("patch");
          } else {
            this.publishRemotePresentationPatch();
          }
        }
      } else if (this.activeProjectId !== browserTab.projectId) {
        this.focusProjectId(browserTab.projectId);
        if (this.presentation) {
          this.publishPresentation("patch");
        }
      }
      const post = window.ghostexGpui?.postBrowserTabFocus;
      if (typeof post === "function") {
        post(
          JSON.stringify({
            projectId: browserTab.projectId,
            tabId: browserTab.tabId,
            type: "ghostex.gpui.sidebar.browserTabFocus",
            version: 1,
          }),
        );
      }
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.acknowledgeSessionAttention(sessionId, "sidebar-focus");
      if (
        this.postRemoteSessionNativeAction(
          "openRemoteSessionTerminal",
          remoteSession,
          originalMessage ?? { sessionId, type: "focusSession" },
        )
      ) {
        this.setRemotePresentationSessionFocus(remoteSession);
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      if (this.isQuickAutomationsSidebarSessionId(sessionId)) {
        this.ensureQuickAutomationsProject();
        this.focusQuickAutomationsProject();
      }
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    this.acknowledgeSessionAttention(sessionId, "sidebar-focus");
    if (this.isSleepingLocalPresentationSession(reference.projectId, reference.sessionId)) {
      /*
      CDXC:GPUIWorkspaceSessionFocus 2026-06-26-23:24:
      Sleeping local session-card clicks must match macOS session activation by committing gxserver `/api/wakeSession` before the Rust workspace materializes the terminal. A plain focus bridge can select the tab but leaves gxserver sleeping, so route this branch through the same Wake path as the sidebar sleep toggle.
      */
      await this.setSessionSleeping(sessionId, false);
      return;
    }
    /*
    CDXC:GPUISidebarSessionFocus 2026-06-26-04:42:
    Local GPUI sidebar clicks must match the macOS sidebar ownership model: the SidebarApp adapter applies local focus immediately and publishes the CEF bootstrap focus hint, but it must not call gxserver `/api/focusSession`. That endpoint is an external renderer-command route and can bounce focus when another renderer is the first open gxserver subscriber.
    */
    if (this.isLocalPresentationT3Session(reference.projectId, reference.sessionId)) {
      this.focusLocalT3Session(reference.projectId, reference.sessionId);
    } else {
      this.focusLocalWorkspaceSession(reference.projectId, reference.sessionId);
    }
    this.publishPresentation("patch");
  }

  private focusLocalT3Session(projectId: string, sessionId: string): void {
    /*
    CDXC:GPUIT3SessionFocus 2026-06-28-22:27:
    GPUI T3 Code session-card clicks must activate T3 through a dedicated id-only bridge, not the terminal attach bridge. T3 rows already carry durable gxserver runtime metadata, so Rust owns route resolution and the renderer may send only bounded project/session ids.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedProjectId || !normalizedSessionId) {
      return;
    }
    this.setLocalPresentationSessionFocus(normalizedProjectId, normalizedSessionId);
    this.postLocalT3SessionFocus(normalizedProjectId, normalizedSessionId);
  }

  private focusLocalWorkspaceSession(
    projectId: string,
    sessionId: string,
    options?: { forceRemount?: boolean },
  ): void {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:18:
    Any successful local GPUI activation that makes a gxserver workspace session current must update both the reused SidebarApp presentation focus and the real GPUI Agents workspace. This matches macOS create, fork, restore, App Shot, and session-click behavior instead of requiring a second sidebar click to show the newly focused terminal.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedProjectId || !normalizedSessionId) {
      return;
    }
    this.setLocalPresentationSessionFocus(normalizedProjectId, normalizedSessionId);
    this.postLocalWorkspaceTerminalFocus(
      normalizedProjectId,
      normalizedSessionId,
      undefined,
      options,
    );
  }

  private postLocalWorkspaceTerminalFocus(
    projectId: string,
    sessionId: string,
    placementTargetSessionId?: string,
    options?: { forceRemount?: boolean },
  ): void {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
    Local GPUI session-card clicks must drive the real Agents workspace the way macOS does: after React updates gxserver presentation focus, send only bounded project/session ids to Rust so Rust can select or materialize the corresponding terminal tab from gxserver attach metadata. Do not pass labels, titles, commands, paths, terminal content, or daemon responses through the renderer bridge.
    */
    const postFocus = window.ghostexGpui?.postWorkspaceTerminalFocus;
    if (typeof postFocus !== "function") {
      return;
    }
    const payload = JSON.stringify({
      ...(placementTargetSessionId ? { placementTargetSessionId } : {}),
      ...(options?.forceRemount ? { forceRemount: true } : {}),
      projectId,
      sessionId,
      type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION,
    });
    postFocus(payload);
  }

  private postLocalT3SessionFocus(projectId: string, sessionId: string): void {
    const postFocus = window.ghostexGpui?.postT3SessionFocus;
    if (typeof postFocus !== "function") {
      return;
    }
    const payload = JSON.stringify({
      projectId,
      sessionId,
      type: GPUI_SIDEBAR_T3_SESSION_FOCUS_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_T3_SESSION_FOCUS_MESSAGE_VERSION,
    });
    postFocus(payload);
  }

  private postLocalT3SessionCreate(projectId: string): void {
    if (!T3CODE_ENABLED) {
      return;
    }
    /*
    CDXC:GPUIT3SessionCreate 2026-06-29-01:22:
    The sidebar project-header T3 Code create button must start a project-scoped T3 draft chat, not the generic `npx --yes t3` agent launcher. Send only the gxserver project id to Rust so the native side can create the `kind: "t3"` row, resolve T3 owner-only project metadata, and open the draft composer without renderer-owned URLs, paths, commands, tokens, or daemon responses.
    */
    const postCreate = window.ghostexGpui?.postT3SessionCreate;
    if (typeof postCreate !== "function") {
      return;
    }
    const payload = JSON.stringify({
      projectId,
      type: GPUI_SIDEBAR_T3_SESSION_CREATE_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_T3_SESSION_CREATE_MESSAGE_VERSION,
    });
    postCreate(payload);
  }

  private createFirstPromptTitleRuntimeSettings(
    firstUserMessage?: string,
  ): GpuiFirstPromptTitleRuntimeSettings {
    /*
    CDXC:GPUIFirstPromptTitle 2026-07-04-21:52:
    GPUI agent sessions must carry the same gxserver-owned first-prompt title
    settings as macOS before hooks claim the prompt. The daemon still owns
    eligibility, title generation, and command submission; GPUI only supplies
    the user's saved title-generation agent/command and any already-known first
    prompt.
    */
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    const runtimeSettings: GpuiFirstPromptTitleRuntimeSettings = {
      firstPromptTitleGenerationAgent: settings.sessionTitleGenerationAgent,
    };
    const command = this.resolveSessionTitleGenerationCommandForGxserver(settings);
    if (command) {
      runtimeSettings.firstPromptTitleGenerationCommand = command;
    }
    const prompt = firstUserMessage?.trim();
    if (prompt) {
      runtimeSettings.firstUserMessage = prompt;
    }
    return runtimeSettings;
  }

  private resolveSessionTitleGenerationCommandForGxserver(
    settings: ghostexSettings,
  ): string | undefined {
    if (settings.sessionTitleGenerationAgent === "custom") {
      return settings.customSessionTitleGenerationCommand.trim() || undefined;
    }
    return (
      this.resolveSidebarAgent(settings.sessionTitleGenerationAgent)?.command?.trim() || undefined
    );
  }

  private async createQuickProject(
    kind: "agent" | "terminal",
  ): Promise<GxserverProjectDomainState | undefined> {
    if (!this.client) {
      this.postSidebarActionToast("warning", "Quick action unavailable", {
        description: "gxserver is not connected.",
      });
      return undefined;
    }
    try {
      const response = await this.client.rpc<{ project: GxserverProjectDomainState }>(
        "/api/createQuickProject",
        {
          kind,
        },
      );
      this.upsertDomainProject(response.project);
      this.focusProjectId(response.project.projectId);
      this.publishPresentation("patch");
      return response.project;
    } catch {
      this.postSidebarActionToast("error", "Quick action failed", {
        description: "Ghostex could not create the Quick workspace.",
      });
      return undefined;
    }
  }

  private async createQuickTerminal(): Promise<void> {
    /*
    CDXC:GPUIQuickActions 2026-07-11:
    Match macOS createNativeChat: create and focus a new projectless chat
    workspace first, then create its initial running terminal through the
    ordinary gxserver session path.
    */
    const project = await this.createQuickProject("terminal");
    if (project) {
      await this.createSession(createGxserverPresentationProjectGroupId(project.projectId));
    }
  }

  private async createQuickAgentSession(agentId: string): Promise<void> {
    /*
    Match macOS createNativeAgentChat: a Quick agent never launches inside the
    active code project. Give it a new projectless chat workspace, then reuse
    the same configured-agent launch path as project headers.
    */
    const project = await this.createQuickProject("agent");
    if (project) {
      await this.createAgentSession(
        agentId,
        createGxserverPresentationProjectGroupId(project.projectId),
      );
    }
  }

  private openQuickBrowserTab(): void {
    /*
    GPUI currently owns Browser tabs at the window level instead of as Agents
    workspace sessions. Send the Quick header's explicit browser launch through
    the existing app-owned Browser bridge, with a distinct fixed origin so Rust
    can honor this projectless launcher even while project-scoped Browser mode
    is otherwise disabled in Quick context.
    */
    const post = window.ghostexGpui?.postOpenBrowserUrl;
    if (typeof post !== "function") {
      this.postSidebarActionToast("warning", "Quick Browser unavailable");
      return;
    }
    const accepted = post(
      JSON.stringify({
        origin: "quickHeader",
        reuse: "none",
        type: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
        url: DEFAULT_BROWSER_LAUNCH_URL,
        version: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
      }),
    );
    if (!accepted) {
      this.postSidebarActionToast("warning", "Quick Browser unavailable");
    }
  }

  private openBrowserPaneInGroup(groupId: string): void {
    const projectId = this.resolveWorkspaceGroupProjectId(groupId);
    if (!projectId) {
      return;
    }
    /*
    CDXC:GPUIRemoteBrowserTabs 2026-07-12:
    Browser tabs are project-keyed local CEF panes, so remote projects reuse
    the same workarea through their machine-scoped project ids. The payload
    carries the explicit target project id so Rust swaps the browser project
    model before creating the tab instead of racing the async active-project
    context round-trip through React.
    */
    const remoteProject = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteProject) {
      this.activeGroupId = groupId;
      this.publishRemotePresentationPatch();
      /*
      CDXC:GPUIRemotePortsBrowser 2026-07-30:
      A remote project's Browser pane defaults to the machine's listening-ports
      page instead of the generic launch URL, so the tab lands on the remote's
      address with its running apps one click away. Rust owns SSH port
      discovery, page generation, and the final tab URL; the renderer sends
      only the fixed action plus the machine-scoped project id.
      */
      if (
        !this.postRemoteProjectNativeAction("openRemoteProjectPortsBrowser", remoteProject, {
          groupId,
          type: "openBrowserPaneInGroup",
        })
      ) {
        this.postSidebarActionToast("warning", "Browser unavailable");
      }
      return;
    }
    if (!this.presentation) {
      return;
    }
    this.activeProjectId = projectId;
    this.activeGroupId = groupId;
    this.publishPresentation("patch");

    const post = window.ghostexGpui?.postOpenBrowserUrl;
    if (
      typeof post !== "function" ||
      !post(
        JSON.stringify({
          projectId,
          reuse: "none",
          type: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
          url: DEFAULT_BROWSER_LAUNCH_URL,
          version: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
        }),
      )
    ) {
      this.postSidebarActionToast("warning", "Browser unavailable");
    }
  }

  private async createSession(groupId = this.activeGroupId): Promise<void> {
    const subgroup = groupId ? parseGpuiWorkspaceSessionSubgroupId(groupId) : undefined;
    const subgroupRemoteProject = subgroup
      ? parseGpuiRemotePresentationProjectId(subgroup.projectId)
      : undefined;
    const remoteGroup =
      groupId && !subgroup ? parseGpuiRemotePresentationGroupId(groupId) : undefined;
    const remoteTarget = subgroupRemoteProject ?? remoteGroup;
    if (remoteTarget) {
      await this.requestRemoteGxserver<GpuiGxserverCreatedSessionResult>(
        remoteTarget.machineId,
        "/api/createSession",
        {
          kind: "terminal",
          lifecycleState: "running",
          projectId: remoteTarget.projectId,
          surface: "workspace",
          title: DEFAULT_TERMINAL_SESSION_TITLE,
        },
      )
        .then((response) => {
          const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
          if (createdSessionId) {
            if (subgroup && subgroupRemoteProject) {
              this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
                this.workspaceGroups,
                subgroup.projectId,
                createdSessionId,
                subgroup.groupId,
              );
              this.persistWorkspaceGroups();
            }
            const createdReference = {
              machineId: remoteTarget.machineId,
              projectId:
                normalizeNonEmptyString(response.session?.projectId) ?? remoteTarget.projectId,
              sessionId: createdSessionId,
            };
            this.setRemotePresentationSessionFocus(createdReference);
            this.postRemoteSessionNativeAction("openRemoteSessionTerminal", createdReference, {
              sessionId: createGpuiRemotePresentationSessionId(
                createdReference.machineId,
                createdReference.projectId,
                createdReference.sessionId,
              ),
              type: "focusSession",
            });
          }
          this.refreshRemotePresentationFromGxserver(remoteTarget.machineId).catch(() => undefined);
        })
        .catch(() => {
          this.postRemoteToast("warning", "Remote session failed", {
            description: "The remote gxserver could not create that session.",
          });
        });
      return;
    }
    const projectId = subgroup
      ? subgroup.projectId
      : groupId
        ? parseGxserverPresentationProjectGroupId(groupId)
        : this.activeProjectId;
    if (!this.client) {
      return;
    }
    if (projectId && !this.ensureLocalProjectPathAvailable(projectId)) {
      return;
    }
    /*
    CDXC:GPUISidebarGxserverRuntime 2026-07-07:
    gxserver defaults an omitted lifecycleState to "unknown", which the
    presentation layer treats as inactive, so the created terminal never gets a
    sidebar row even though the workspace pane opens. Declare the session
    running at create time like the remote path and the macOS client do.
    */
    let response: GpuiGxserverCreatedSessionResult;
    try {
      response = await this.client.rpc<GpuiGxserverCreatedSessionResult>("/api/createSession", {
        ...(projectId ? { projectId } : {}),
        kind: "terminal",
        lifecycleState: "running",
        surface: "workspace",
        title: DEFAULT_TERMINAL_SESSION_TITLE,
      });
    } catch (error) {
      if (
        projectId &&
        error instanceof GpuiGxserverRpcError &&
        error.code === "projectPathUnavailable" &&
        this.presentMissingProjectFolder(projectId)
      ) {
        void this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => undefined);
        return;
      }
      throw error;
    }
    const createdProjectId = normalizeNonEmptyString(response.session?.projectId) ?? projectId;
    const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (subgroup && createdProjectId === subgroup.projectId && createdSessionId) {
      this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
        this.workspaceGroups,
        subgroup.projectId,
        createdSessionId,
        subgroup.groupId,
      );
      this.persistWorkspaceGroups();
    }
    if (createdProjectId && createdSessionId) {
      this.focusLocalWorkspaceSession(createdProjectId, createdSessionId);
    }
  }

  private async createProjectTerminal(groupId: string): Promise<void> {
    /*
    CDXC:GPUIWindowsProjectTerminal 2026-07-26:
    The project-heading terminal button is an explicit project-scoped create
    request. On Windows, keep the WSL gxserver create and attach sequence in
    the Rust host by posting only the clicked local project id. The native host
    then reuses the same atomic path as GPUI New Terminal. macOS, Linux, remote
    projects, and generic subgroup creation keep their existing flows.
    */
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      await this.createSession(groupId);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    const isWindowsHost =
      typeof navigator !== "undefined" && /Windows/iu.test(navigator.userAgent);
    if (!isWindowsHost) {
      await this.createSession(groupId);
      return;
    }
    const postCreate = window.ghostexGpui?.postCreateProjectTerminal;
    if (!projectId || typeof postCreate !== "function") {
      this.postSidebarActionToast("warning", "Terminal unavailable");
      return;
    }
    try {
      const accepted = postCreate(
        JSON.stringify({
          projectId,
          type: "ghostex.gpui.sidebar.createProjectTerminal",
          version: 1,
        }),
      );
      if (!accepted) {
        this.postSidebarActionToast("warning", "Terminal unavailable");
      }
    } catch {
      this.postSidebarActionToast("warning", "Terminal unavailable");
    }
  }

  private async startAgentSessionProviderAndSendPrompt(
    startProvider: () => Promise<unknown>,
    sendPrompt: (promptText: string) => Promise<unknown>,
    prompt?: string,
    renameCommand?: string,
  ): Promise<void> {
    await startProvider();
    const promptText = normalizeNonEmptyString(prompt);
    const renameText = normalizeNonEmptyString(renameCommand);
    if (!promptText && !renameText) {
      return;
    }
    await delayGpuiAgentPromptStep(GPUI_AGENT_PROMPT_READY_DELAY_MS);
    if (renameText) {
      await sendPrompt(renameText);
      await delayGpuiAgentPromptStep(GPUI_AGENT_PROMPT_STEP_DELAY_MS);
    }
    if (promptText) {
      await sendPrompt(promptText);
    }
  }

  private async startRemoteAgentSessionAndSendPrompt(
    machineId: string,
    projectId: string,
    sessionId: string,
    prompt?: string,
  ): Promise<void> {
    await this.startAgentSessionProviderAndSendPrompt(
      () =>
        this.requestRemoteGxserver(
          machineId,
          "/api/startSessionProvider",
          {
            projectId,
            sessionId,
          },
          { timeoutMs: 15_000 },
        ),
      (promptText) =>
        this.requestRemoteGxserver(
          machineId,
          "/api/sendSessionMessage",
          {
            projectId,
            sessionId,
            submit: true,
            text: promptText,
          },
          { timeoutMs: 15_000 },
        ),
      prompt,
    );
  }

  private async startLocalAgentSessionAndSendPrompt(
    projectId: string,
    sessionId: string,
    prompt?: string,
    renameCommand?: string,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      throw new Error("gxserver is unavailable.");
    }
    await this.startAgentSessionProviderAndSendPrompt(
      () =>
        client.rpc("/api/startSessionProvider", {
          projectId,
          sessionId,
        }),
      (promptText) =>
        client.rpc("/api/sendSessionMessage", {
          projectId,
          sessionId,
          submit: true,
          text: promptText,
        }),
      prompt,
      renameCommand,
    );
  }

  private async createAgentSession(agentId: string, groupId = this.activeGroupId): Promise<void> {
    const remoteGroup = groupId ? parseGpuiRemotePresentationGroupId(groupId) : undefined;
    if (remoteGroup) {
      const normalizedAgentId = agentId.trim();
      if (!normalizedAgentId) {
        this.postRemoteToast("warning", "Remote agent unavailable", {
          description: "Choose a configured agent for this remote project.",
        });
        return;
      }
      /*
      CDXC:GPUIRemoteSessions 2026-06-24-17:19:
      Remote agent launches must let the owning remote gxserver resolve default and project-custom agent commands from remote project metadata. GPUI sends only the selected agent id, project id, surface, and a require-command guard through Rust's authenticated tunnel, never a renderer-provided command string.
      */
      const remoteAgent = this.resolveSidebarAgent(normalizedAgentId);
      const title = createAgentSessionDefaultTitle(remoteAgent?.name ?? normalizedAgentId);
      const response = await this.requestRemoteGxserver<GpuiGxserverCreatedSessionResult>(
        remoteGroup.machineId,
        "/api/createAgentSession",
        {
          agentId: normalizedAgentId,
          projectId: remoteGroup.projectId,
          requireLaunchCommand: true,
          runtimeSettings: this.createFirstPromptTitleRuntimeSettings(),
          surface: "workspace",
          title,
        },
      ).catch(() => {
        this.postRemoteToast("warning", "Remote agent failed", {
          description: "The remote gxserver could not create that agent session.",
        });
        return undefined;
      });
      if (response) {
        const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
        if (createdSessionId) {
          const createdProjectId =
            normalizeNonEmptyString(response.session?.projectId) ?? remoteGroup.projectId;
          await this.startRemoteAgentSessionAndSendPrompt(
            remoteGroup.machineId,
            createdProjectId,
            createdSessionId,
          ).catch(() => {
            this.postRemoteToast("warning", "Remote agent failed", {
              description: "The remote gxserver could not start that agent session.",
            });
          });
          this.setRemotePresentationSessionFocus({
            machineId: remoteGroup.machineId,
            projectId: createdProjectId,
            sessionId: createdSessionId,
          });
        }
        this.refreshRemotePresentationFromGxserver(remoteGroup.machineId).catch(() => undefined);
      }
      return;
    }
    const projectId = groupId
      ? parseGxserverPresentationProjectGroupId(groupId)
      : this.activeProjectId;
    if (projectId && !this.ensureLocalProjectPathAvailable(projectId)) {
      return;
    }
    if (T3CODE_ENABLED && agentId.trim() === "t3") {
      if (projectId) {
        this.postLocalT3SessionCreate(projectId);
      }
      return;
    }
    const agent = this.resolveSidebarAgent(agentId);
    if (!this.client || !projectId || !agent) {
      return;
    }
    if (!agent.command) {
      return;
    }
    let response: GpuiGxserverCreatedSessionResult;
    try {
      response = await this.client.rpc<GpuiGxserverCreatedSessionResult>(
        "/api/createAgentSession",
        {
          agentId: agent.agentId,
          launchSettings: {
            agentCommand: agent.command,
            icon: agent.icon,
          },
          projectId,
          runtimeSettings: this.createFirstPromptTitleRuntimeSettings(),
          surface: "workspace",
          title: createAgentSessionDefaultTitle(agent.name),
        },
      );
    } catch (error) {
      if (
        error instanceof GpuiGxserverRpcError &&
        error.code === "projectPathUnavailable" &&
        this.presentMissingProjectFolder(projectId)
      ) {
        void this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => undefined);
        return;
      }
      throw error;
    }
    const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (createdSessionId) {
      this.focusLocalWorkspaceSession(
        normalizeNonEmptyString(response.session?.projectId) ?? projectId,
        createdSessionId,
      );
    }
  }

  private async searchPreviousSessionsByText(): Promise<void> {
    const projectId = this.activeProjectId;
    if (!this.client || !projectId) {
      this.postSidebarActionToast("info", "Search by Text needs an active project.");
      return;
    }
    const response = await this.client
      .rpc<GpuiGxserverCreatedSessionResult>("/api/createSession", {
        kind: "terminal",
        lifecycleState: "running",
        projectId,
        runtimeSettings: {
          titleSource: "placeholder",
        },
        surface: "workspace",
        title: "Search by Text",
      })
      .catch(() => undefined);
    if (!response) {
      this.postSidebarActionToast("error", "Search by Text failed", {
        description: "gxserver could not create the search terminal.",
      });
      return;
    }
    const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (createdSessionId) {
      const createdProjectId = normalizeNonEmptyString(response.session?.projectId) ?? projectId;
      const started = await this.client
        .rpc("/api/startSessionProvider", {
          projectId: createdProjectId,
          sessionId: createdSessionId,
          startupText: gpuiWithAtuinIgnoredShellHistoryPrefix("gx f\r"),
        })
        .catch(() => undefined);
      if (!started) {
        this.postSidebarActionToast("error", "Search by Text failed", {
          description: "gxserver could not start the search terminal.",
        });
        return;
      }
      this.focusLocalWorkspaceSession(createdProjectId, createdSessionId);
    }
  }

  /*
  GPUI port of the macOS OS-integration sidebar router (`handleNativeCliCommand`
  "createQuickTerminal" / "openPaths" in native-sidebar.tsx). Rust owns URL and
  file parsing, the script Run/Edit/Cancel consent dialog, existence checks,
  and git-root resolution; this handler only registers daemon projects and
  creates/focuses sessions through existing reviewed paths. Payloads are
  first-party fixed shapes from the Rust bridge (bounded action enum plus
  path/command/title strings); unknown actions surface an honest toast instead
  of dropping silently.
  */
  private async handleGpuiOsIntegrationCommand(payload: unknown): Promise<void> {
    const record =
      payload && typeof payload === "object" ? (payload as Record<string, unknown>) : undefined;
    const action = normalizeNonEmptyString(record?.action);
    if (!record || !action) {
      return;
    }
    if (action === "createQuickTerminal") {
      await this.createOsIntegrationTerminal({
        command: normalizeNonEmptyString(record.command),
        cwd: normalizeNonEmptyString(record.cwd),
        title: normalizeNonEmptyString(record.title),
      });
      return;
    }
    if (action === "openProjectPaths") {
      await this.openOsIntegrationProjectPaths(
        Array.isArray(record.projects) ? record.projects : [],
      );
      return;
    }
    this.postSidebarActionToast("warning", "Unsupported OS integration action.");
  }

  /*
  `ghostex://terminal` parity note: macOS creates a client-side projectless
  Quick project per invocation; GPUI's sidebar is daemon-derived, so the
  terminal lands in the daemon project registered (or reused) at the resolved
  cwd. A provided command launches the session with it (the Search-by-Text
  `gx f` launcher contract) instead of macOS's typed `command\r` into a shell.
  */
  private async createOsIntegrationTerminal(input: {
    command?: string;
    cwd?: string;
    title?: string;
  }): Promise<void> {
    if (!this.client || !input.cwd) {
      this.postSidebarActionToast("warning", "Open Terminal failed", {
        description: "ghostex://terminal needs the local gxserver.",
      });
      return;
    }
    try {
      const project = await this.registerProjectPath({
        name: gpuiProjectNameFromPath(input.cwd),
        path: input.cwd,
      });
      this.focusProjectId(project.projectId);
      this.publishPresentation("patch");
      const title =
        input.title ??
        normalizeNonEmptyString(gpuiProjectNameFromPath(input.cwd)) ??
        DEFAULT_TERMINAL_SESSION_TITLE;
      const response = input.command
        ? await this.client.rpc<GpuiGxserverCreatedSessionResult>("/api/createAgentSession", {
            agentId: "os-integration-terminal",
            launchSettings: {
              agentCommand: input.command,
            },
            projectId: project.projectId,
            surface: "workspace",
            title,
          })
        : await this.client.rpc<GpuiGxserverCreatedSessionResult>("/api/createSession", {
            kind: "terminal",
            lifecycleState: "running",
            projectId: project.projectId,
            surface: "workspace",
            title,
          });
      const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
      if (createdSessionId) {
        this.focusLocalWorkspaceSession(
          normalizeNonEmptyString(response.session?.projectId) ?? project.projectId,
          createdSessionId,
        );
      }
    } catch {
      this.postSidebarActionToast("error", "Open Terminal failed", {
        description: "gxserver could not create the requested terminal.",
      });
    }
  }

  private async openOsIntegrationProjectPaths(entries: unknown[]): Promise<void> {
    if (!this.client) {
      this.postSidebarActionToast("warning", "Open failed", {
        description: "Opening paths needs the local gxserver.",
      });
      return;
    }
    let focusProjectId: string | undefined;
    let failedCount = 0;
    for (const entry of entries.slice(0, 16)) {
      const record =
        entry && typeof entry === "object" ? (entry as Record<string, unknown>) : undefined;
      const path = normalizeNonEmptyString(record?.path);
      if (!path) {
        continue;
      }
      try {
        const project = await this.registerProjectPath({
          name: gpuiProjectNameFromPath(path),
          path,
        });
        focusProjectId = project.projectId;
      } catch {
        failedCount += 1;
      }
    }
    if (failedCount > 0) {
      this.postSidebarActionToast("error", "Open failed", {
        description: "gxserver could not open a requested folder as a project.",
      });
    }
    if (focusProjectId) {
      this.focusProjectId(focusProjectId);
      this.publishPresentation("patch");
    }
  }

  private async setGroupSleeping(groupId: string, sleeping: boolean): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
      const memberIds =
        getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
          (group) => group.groupId === subgroup.groupId,
        )?.sessionIds ?? [];
      await this.setSessionsSleeping(
        memberIds.map((sessionId) =>
          remoteProject
            ? createGpuiRemotePresentationSessionId(
                remoteProject.machineId,
                remoteProject.projectId,
                sessionId,
              )
            : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId),
        ),
        sleeping,
      );
      return;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const scopedProjectId = createGpuiRemotePresentationProjectId(
        remoteGroup.machineId,
        remoteGroup.projectId,
      );
      const sessionIds = this.browserTabs
        .filter((tab) => tab.projectId === scopedProjectId)
        .map(gpuiBrowserSidebarSessionId)
        .concat(
          (presentation?.sessions ?? [])
            .filter((session) => session.projectId === remoteGroup.projectId)
            .map((session) =>
              createGpuiRemotePresentationSessionId(
                remoteGroup.machineId,
                remoteGroup.projectId,
                session.sessionId,
              ),
            ),
        );
      /*
      CDXC:GPUISidebarBulkSleep 2026-06-27-02:05:
      Group sleep shares the same native-parity pacing as explicit multi-select sleep, while Wake remains concurrent because restoring sessions does not need terminal teardown throttling.
      */
      await this.setSessionsSleeping(sessionIds, sleeping);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return;
    }
    const sessionIds = this.browserTabs
      .filter((tab) => tab.projectId === projectId)
      .map(gpuiBrowserSidebarSessionId)
      .concat(
        this.presentation.sessions
          .filter((session) => session.projectId === projectId)
          .map((session) =>
            createGxserverPresentationProjectSessionId(projectId, session.sessionId),
          ),
      );
    /*
    CDXC:GPUISidebarBulkSleep 2026-06-27-02:05:
    Local project group sleep uses the shared private-data-free pacing helper through setSessionsSleeping, preserving the existing per-session focus replacement behavior inside setSessionSleeping.
    */
    await this.setSessionsSleeping(sessionIds, sleeping);
  }

  private async setSessionsSleeping(
    sessionIds: readonly string[],
    sleeping: boolean,
  ): Promise<void> {
    if (!sleeping) {
      await Promise.all(sessionIds.map((sessionId) => this.setSessionSleeping(sessionId, false)));
      return;
    }
    /*
    CDXC:GPUISidebarBulkSleep 2026-06-27-02:05:
    GPUI sleep bulk actions must mirror native pacing by starting one sleep request at a time with a 350 ms interval. Use the shared aggregate-count helper so per-operation failures continue without exposing ids, titles, paths, commands, URLs, or user text.
    */
    await runGpuiSidebarBulkSleepPaced(sessionIds, async (sessionId) => {
      await this.setSessionSleeping(sessionId, true);
    });
  }

  private async setSessionSleeping(
    sessionId: string,
    sleeping: boolean,
    options?: { forceRemount?: boolean },
  ): Promise<void> {
    const browserTab = this.browserTabs.find(
      (candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId,
    );
    if (browserTab) {
      window.ghostexGpui?.postBrowserTabFocus?.(
        JSON.stringify({
          projectId: browserTab.projectId,
          sleeping,
          tabId: browserTab.tabId,
          type: "ghostex.gpui.sidebar.browserTabFocus",
          version: 1,
        }),
      );
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      await this.requestRemoteGxserver(
        remoteSession.machineId,
        sleeping ? "/api/sleepSession" : "/api/wakeSession",
        {
          projectId: remoteSession.projectId,
          reason: "gpui-sidebar",
          sessionId: remoteSession.sessionId,
        },
      );
      await this.refreshRemotePresentationFromGxserver(remoteSession.machineId);
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    const replacementFocusSessionId = sleeping
      ? this.resolveLocalProjectListTransitionFocusTarget(reference.projectId, reference.sessionId)
      : undefined;
    await this.client.rpc(sleeping ? "/api/sleepSession" : "/api/wakeSession", {
      projectId: reference.projectId,
      reason: "gpui-sidebar",
      sessionId: reference.sessionId,
    });
    if (sleeping) {
      this.patchPresentationSession(reference.projectId, reference.sessionId, {
        lifecycleState: "sleeping",
      });
      if (replacementFocusSessionId) {
        this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
        this.publishPresentation("patch");
      }
      return;
    }
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:34:
    A local sidebar Wake action is also a workspace activation in the macOS app: the row becomes running and the corresponding workspace terminal is selected/restored through the same focus path as a direct session click. GPUI must use the local focus bridge here, not gxserver `/api/focusSession`.
    */
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      lifecycleState: "running",
    });
    this.focusLocalWorkspaceSession(reference.projectId, reference.sessionId, options);
    this.publishPresentation("patch");
  }

  private async transitionSession(sessionId: string, action: "close" | "sleep"): Promise<void> {
    const browserTab = this.browserTabs.find(
      (candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId,
    );
    if (browserTab) {
      if (action === "close") {
        window.ghostexGpui?.postBrowserTabFocus?.(
          JSON.stringify({
            close: true,
            projectId: browserTab.projectId,
            tabId: browserTab.tabId,
            type: "ghostex.gpui.sidebar.browserTabFocus",
            version: 1,
          }),
        );
      }
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.postRemoteGxserverSidebarRequest(
        remoteSession.machineId,
        action === "close" ? "/api/killSession" : "/api/sleepSession",
        {
          projectId: remoteSession.projectId,
          reason: "gpui-sidebar",
          sessionId: remoteSession.sessionId,
        },
      );
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      if (action === "close" && this.isQuickAutomationsSidebarSessionId(sessionId)) {
        this.closeQuickAutomationsProject();
      }
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    const replacementFocusSessionId = this.resolveLocalProjectListTransitionFocusTarget(
      reference.projectId,
      reference.sessionId,
    );
    if (action === "close") {
      this.removePresentationSession(reference.projectId, reference.sessionId);
      if (replacementFocusSessionId) {
        this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
        this.publishPresentation("patch");
      }
      await this.client
        .rpc<GxserverSessionTransitionResult>("/api/transitionSession", {
          action,
          projectId: reference.projectId,
          reason: "gpui-sidebar",
          sessionId: reference.sessionId,
        })
        .catch(() => undefined);
      return;
    }
    const result = await this.client.rpc<GxserverSessionTransitionResult>(
      "/api/transitionSession",
      {
        action,
        projectId: reference.projectId,
        reason: "gpui-sidebar",
        sessionId: reference.sessionId,
      },
    );
    if (!shouldApplyGpuiLocalWorkspaceTransition(result, action)) {
      return;
    }
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      lifecycleState: "sleeping",
    });
    if (replacementFocusSessionId) {
      this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
      this.publishPresentation("patch");
    }
  }

  private copySessionDetails(
    message: Extract<SidebarToExtensionMessage, { type: "copySessionDetails" }>,
  ): void {
    const detailsText = normalizeNonEmptyString(message.detailsText);
    if (!detailsText) {
      this.handleUnsupportedSidebarMessage(message);
      return;
    }
    try {
      postAppModalHostMessage(
        { detailsText, type: "copySessionDetails" },
        "GPUISidebarActions:copySessionDetails",
      );
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  }

  private async fullReloadSession(sessionId: string): Promise<void> {
    /*
    CDXC:GPUIFullReload 2026-07-12:
    Full reload must really cycle the provider: `/api/sleepSession` zmx-kills
    the daemon (and the CLI inside it) and `/api/wakeSession` respawns it with
    the restore command. The local surface in the Rust workspace is now dead,
    but Rust only learns about the sleep through presentation snapshots, so a
    plain wake focus can race ahead and re-select the dead mounted terminal.
    `forceRemount` makes the wake focus tear down the stale local terminal
    owner synchronously before running the ordinary attach pipeline, so the
    reused tab deterministically re-attaches to the freshly restored daemon.
    */
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!remoteSession && (!reference || !this.client)) {
      return;
    }
    await this.setSessionSleeping(sessionId, true);
    await this.setSessionSleeping(sessionId, false, { forceRemount: true });
  }

  private async fullReloadProjectZmxSessions(groupId: string): Promise<void> {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const remoteSessionIds = (presentation?.sessions ?? [])
        .filter(
          (session) =>
            session.projectId === remoteGroup.projectId &&
            session.sessionPersistenceProvider === "zmx" &&
            isGpuiInactiveProjectPresentationSession(session),
        )
        .map((session) =>
          createGpuiRemotePresentationSessionId(
            remoteGroup.machineId,
            remoteGroup.projectId,
            session.sessionId,
          ),
        );
      for (const reloadSessionId of remoteSessionIds) {
        await this.fullReloadSession(reloadSessionId);
      }
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return;
    }
    const sessionIds = this.presentation.sessions
      .filter(
        (session) =>
          session.projectId === projectId &&
          session.sessionPersistenceProvider === "zmx" &&
          isGpuiInactiveProjectPresentationSession(session),
      )
      .map((session) => createGxserverPresentationProjectSessionId(projectId, session.sessionId));
    for (const reloadSessionId of sessionIds) {
      await this.fullReloadSession(reloadSessionId);
    }
  }

  private async fullReloadWorkspaceGroup(groupId: string): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      await this.fullReloadProjectZmxSessions(groupId);
      return;
    }
    const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
    const memberIds =
      getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
        (group) => group.groupId === subgroup.groupId,
      )?.sessionIds ?? [];
    for (const sessionId of memberIds) {
      await this.fullReloadSession(
        remoteProject
          ? createGpuiRemotePresentationSessionId(
              remoteProject.machineId,
              remoteProject.projectId,
              sessionId,
            )
          : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId),
      );
    }
  }

  private async closeInactiveProjectSessions(groupId: string): Promise<void> {
    const sessionIds = this.collectInactiveProjectSessionIds(groupId);
    await Promise.all(sessionIds.map((sessionId) => this.transitionSession(sessionId, "close")));
  }

  private async sleepInactiveProjectSessions(groupId: string): Promise<void> {
    const sessionIds = this.collectInactiveProjectSessionIds(groupId);
    await this.setSessionsSleeping(sessionIds, true);
  }

  private collectInactiveProjectSessionIds(groupId: string): string[] {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const scopedProjectId = createGpuiRemotePresentationProjectId(
        remoteGroup.machineId,
        remoteGroup.projectId,
      );
      return this.browserTabs
        .filter((tab) => tab.projectId === scopedProjectId && !tab.isSleeping && !tab.isVisible)
        .map(gpuiBrowserSidebarSessionId)
        .concat(
          (presentation?.sessions ?? [])
            .filter((session) => session.projectId === remoteGroup.projectId)
            .filter(isGpuiInactiveProjectPresentationSession)
            .map((session) =>
              createGpuiRemotePresentationSessionId(
                remoteGroup.machineId,
                remoteGroup.projectId,
                session.sessionId,
              ),
            ),
        );
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return [];
    }
    return this.browserTabs
      .filter((tab) => tab.projectId === projectId && !tab.isSleeping && !tab.isVisible)
      .map(gpuiBrowserSidebarSessionId)
      .concat(
        this.presentation.sessions
          .filter((session) => session.projectId === projectId)
          .filter(isGpuiInactiveProjectPresentationSession)
          .map((session) =>
            createGxserverPresentationProjectSessionId(projectId, session.sessionId),
          ),
      );
  }

  private async wakeProjectSleepingSessions(groupId: string): Promise<void> {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const scopedProjectId = createGpuiRemotePresentationProjectId(
        remoteGroup.machineId,
        remoteGroup.projectId,
      );
      const sessionIds = this.browserTabs
        .filter((tab) => tab.projectId === scopedProjectId && tab.isSleeping)
        .map(gpuiBrowserSidebarSessionId)
        .concat(
          (presentation?.sessions ?? [])
            .filter(
              (session) =>
                session.projectId === remoteGroup.projectId &&
                session.lifecycleState === "sleeping",
            )
            .map((session) =>
              createGpuiRemotePresentationSessionId(
                remoteGroup.machineId,
                remoteGroup.projectId,
                session.sessionId,
              ),
            ),
        );
      await this.setSessionsSleeping(sessionIds, false);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return;
    }
    this.focusProjectId(projectId);
    const sessionIds = this.browserTabs
      .filter((tab) => tab.projectId === projectId && tab.isSleeping)
      .map(gpuiBrowserSidebarSessionId)
      .concat(
        this.presentation.sessions
          .filter(
            (session) => session.projectId === projectId && session.lifecycleState === "sleeping",
          )
          .map((session) =>
            createGxserverPresentationProjectSessionId(projectId, session.sessionId),
          ),
      );
    await this.setSessionsSleeping(sessionIds, false);
  }

  /*
  CDXC:GPUIWorkspaceGroups 2026-07-02-03:49:
  GPUI sidebar named groups are a client-owned project overlay until gxserver exposes durable grouped workspace state.
  Route only local project/session ids through create, rename, close, move, and reorder operations; remote groups stay out of this path and localStorage mirrors macOS grouped workspace semantics.
  */
  private persistWorkspaceGroups(): void {
    writeStoredGpuiWorkspaceSessionGroupsState(this.workspaceGroups);
    this.scheduleWorkspaceGroupsServerSync();
  }

  /*
  CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
  gxserver now keeps a durable copy of this overlay so iOS/Android render the
  same named groups and ordering. localStorage stays the instant-edit source;
  the server copy is a debounced write-through so group editing never waits on
  an RPC. While a push is pending or failed, hydration must not clobber the
  newer local state.
  */
  private scheduleWorkspaceGroupsServerSync(): void {
    this.workspaceGroupsServerSyncPending = true;
    if (this.workspaceGroupsServerSyncTimeoutId !== undefined) {
      window.clearTimeout(this.workspaceGroupsServerSyncTimeoutId);
    }
    this.workspaceGroupsServerSyncTimeoutId = window.setTimeout(() => {
      this.workspaceGroupsServerSyncTimeoutId = undefined;
      void this.pushWorkspaceGroupsToGxserver();
    }, GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS);
  }

  private async pushWorkspaceGroupsToGxserver(): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    const pushed = this.workspaceGroups;
    try {
      await client.updateWorkspaceSessionGroups(pushed);
      if (this.workspaceGroups === pushed) {
        this.workspaceGroupsServerSyncPending = false;
      }
    } catch {
      if (
        this.client === client &&
        this.workspaceGroupsServerSyncTimeoutId === undefined &&
        this.workspaceGroupsServerSyncPending
      ) {
        this.workspaceGroupsServerSyncTimeoutId = window.setTimeout(() => {
          this.workspaceGroupsServerSyncTimeoutId = undefined;
          void this.pushWorkspaceGroupsToGxserver();
        }, GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS);
      }
    }
  }

  private adoptWorkspaceGroupsFromGxserver(serverState: unknown): void {
    if (serverState === undefined || this.workspaceGroupsServerSyncPending) {
      return;
    }
    const parsed = parseGpuiWorkspaceSessionGroupsState(serverState);
    if (isEmptyGpuiWorkspaceSessionGroupsState(parsed)) {
      if (!isEmptyGpuiWorkspaceSessionGroupsState(this.workspaceGroups)) {
        this.scheduleWorkspaceGroupsServerSync();
      }
      return;
    }
    if (JSON.stringify(parsed) === JSON.stringify(this.workspaceGroups)) {
      return;
    }
    this.workspaceGroups = parsed;
    writeStoredGpuiWorkspaceSessionGroupsState(parsed);
  }

  /*
  CDXC:SidebarProjectCollections 2026-07-18-00:00:
  Colored "Group N" project collections mirror the workspace-groups sync shape,
  but SidebarApp owns the localStorage overlay and the editing UI, so this
  runtime only relays: sidebar `updateSidebarProjectCollections` commands are
  debounced into gxserver write-throughs, and server state (startup snapshot,
  live sidebarProjectCollectionsChanged events, update acks) is forwarded back
  to SidebarApp for reconciliation. While a push is pending or failed, server
  forwards are suppressed so older server state cannot clobber newer local
  edits.
  */
  private queueSidebarProjectCollectionsServerSync(
    state: GxserverSidebarProjectCollectionsState,
  ): void {
    this.latestSidebarProjectCollectionsUpdate = state;
    this.sidebarProjectCollectionsServerSyncPending = true;
    if (this.sidebarProjectCollectionsServerSyncTimeoutId !== undefined) {
      window.clearTimeout(this.sidebarProjectCollectionsServerSyncTimeoutId);
    }
    this.sidebarProjectCollectionsServerSyncTimeoutId = window.setTimeout(() => {
      this.sidebarProjectCollectionsServerSyncTimeoutId = undefined;
      void this.pushSidebarProjectCollectionsToGxserver();
    }, GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_DELAY_MS);
  }

  private async pushSidebarProjectCollectionsToGxserver(): Promise<void> {
    const client = this.client;
    const pushed = this.latestSidebarProjectCollectionsUpdate;
    if (!client || !pushed) {
      return;
    }
    try {
      const normalized = await client.updateSidebarProjectCollections(pushed);
      if (this.latestSidebarProjectCollectionsUpdate === pushed) {
        this.sidebarProjectCollectionsServerSyncPending = false;
        if (isSidebarProjectCollectionsState(normalized)) {
          this.forwardSidebarProjectCollectionsFromGxserver(normalized);
        }
      }
    } catch {
      if (
        this.client === client &&
        this.sidebarProjectCollectionsServerSyncTimeoutId === undefined &&
        this.sidebarProjectCollectionsServerSyncPending
      ) {
        this.sidebarProjectCollectionsServerSyncTimeoutId = window.setTimeout(() => {
          this.sidebarProjectCollectionsServerSyncTimeoutId = undefined;
          void this.pushSidebarProjectCollectionsToGxserver();
        }, GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_RETRY_DELAY_MS);
      }
    }
  }

  private forwardSidebarProjectCollectionsFromGxserver(
    state: GxserverSidebarProjectCollectionsState,
  ): void {
    if (this.sidebarProjectCollectionsServerSyncPending) {
      return;
    }
    const stateJson = JSON.stringify(state);
    if (stateJson === this.lastForwardedSidebarProjectCollectionsJson) {
      return;
    }
    this.lastForwardedSidebarProjectCollectionsJson = stateJson;
    this.messageSource.postMessage({
      sidebarProjectCollections: state,
      type: "sidebarProjectCollectionsChanged",
    });
  }

  private forwardRemoteSidebarProjectCollectionsFromGxserver(
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState,
  ): void {
    const stateJson = JSON.stringify(state);
    if (
      this.lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId.get(remoteMachineId) ===
      stateJson
    ) {
      return;
    }
    this.lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId.set(
      remoteMachineId,
      stateJson,
    );
    this.messageSource.postMessage({
      remoteMachineId,
      sidebarProjectCollections: state,
      type: "sidebarProjectCollectionsChanged",
    });
  }

  private async updateRemoteSidebarProjectCollections(
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState,
  ): Promise<void> {
    const response = await this.requestRemoteGxserver<{
      sidebarProjectCollections?: unknown;
    }>(remoteMachineId, "/api/updateSidebarProjectCollections", { state });
    if (!isSidebarProjectCollectionsState(response.sidebarProjectCollections)) {
      throw new Error("Remote gxserver returned invalid project collections.");
    }
    const snapshot = this.remotePresentations.get(remoteMachineId);
    if (snapshot) {
      this.remotePresentations.set(remoteMachineId, {
        ...snapshot,
        sidebarProjectCollections: response.sidebarProjectCollections,
      });
    }
    this.forwardRemoteSidebarProjectCollectionsFromGxserver(
      remoteMachineId,
      response.sidebarProjectCollections,
    );
  }

  private createWorkspaceGroup(groupId?: string): void {
    const projectId = this.resolveWorkspaceGroupProjectId(groupId) ?? this.activeProjectId;
    if (!projectId) {
      return;
    }
    const result = createGpuiWorkspaceSessionSubgroup(this.workspaceGroups, projectId);
    if (!result.groupId) {
      this.postSidebarActionToast("info", "Group limit reached for this project.");
      return;
    }
    this.workspaceGroups = result.state;
    this.persistWorkspaceGroups();
    if (!parseGpuiRemotePresentationProjectId(projectId)) {
      this.activeProjectId = projectId;
    }
    this.activeGroupId = createGpuiWorkspaceSessionSubgroupId(projectId, result.groupId);
    this.refreshSidebarHudFromClient();
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  }

  private createWorkspaceGroupFromSession(sessionId: string): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const reference = remoteSession
      ? {
          projectId: createGpuiRemotePresentationProjectId(
            remoteSession.machineId,
            remoteSession.projectId,
          ),
          sessionId: remoteSession.sessionId,
        }
      : parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference) {
      return;
    }
    const result = createGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      reference.projectId,
      reference.sessionId,
    );
    if (!result.groupId) {
      this.postSidebarActionToast("info", "Group limit reached for this project.");
      return;
    }
    this.workspaceGroups = result.state;
    this.persistWorkspaceGroups();
    if (!remoteSession) {
      this.activeProjectId = reference.projectId;
    }
    this.activeGroupId = createGpuiWorkspaceSessionSubgroupId(reference.projectId, result.groupId);
    this.refreshSidebarHudFromClient();
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  }

  private resolveWorkspaceGroupProjectId(groupId: string | undefined): string | undefined {
    if (!groupId) {
      return undefined;
    }
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      return subgroup.projectId;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      return createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
    }
    return parseGxserverPresentationProjectGroupId(groupId);
  }

  private renameWorkspaceGroup(groupId: string, title: string): void {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const next = renameGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
      title,
    );
    if (next === this.workspaceGroups) {
      return;
    }
    this.workspaceGroups = next;
    this.persistWorkspaceGroups();
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  }

  private async closeWorkspaceGroup(groupId: string): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
    const memberIds = [
      ...(getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
        (group) => group.groupId === subgroup.groupId,
      )?.sessionIds ?? []),
    ];
    await Promise.all(
      memberIds.map((sessionId) =>
        this.transitionSession(
          remoteProject
            ? createGpuiRemotePresentationSessionId(
                remoteProject.machineId,
                remoteProject.projectId,
                sessionId,
              )
            : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId),
          "close",
        ),
      ),
    );
    this.workspaceGroups = removeGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
    );
    this.persistWorkspaceGroups();
    if (this.activeGroupId === groupId) {
      this.activeGroupId = remoteProject
        ? createGpuiRemotePresentationGroupId(remoteProject.machineId, remoteProject.projectId)
        : createGxserverPresentationProjectGroupId(subgroup.projectId);
    }
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  }

  private moveSessionToWorkspaceGroup(message: {
    groupId: string;
    sessionId: string;
    targetIndex?: number;
  }): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
    const reference = remoteSession
      ? {
          projectId: createGpuiRemotePresentationProjectId(
            remoteSession.machineId,
            remoteSession.projectId,
          ),
          sessionId: remoteSession.sessionId,
        }
      : parseGxserverPresentationProjectSessionId(message.sessionId);
    if (!reference) {
      return;
    }
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(message.groupId);
    if (subgroup) {
      if (subgroup.projectId !== reference.projectId) {
        return;
      }
      this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
        this.workspaceGroups,
        reference.projectId,
        reference.sessionId,
        subgroup.groupId,
        message.targetIndex,
      );
    } else {
      const remoteGroup = parseGpuiRemotePresentationGroupId(message.groupId);
      const projectId = remoteGroup
        ? createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId)
        : parseGxserverPresentationProjectGroupId(message.groupId);
      if (!projectId || projectId !== reference.projectId) {
        return;
      }
      this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
        this.workspaceGroups,
        reference.projectId,
        reference.sessionId,
        undefined,
      );
    }
    this.persistWorkspaceGroups();
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  }

  private syncWorkspaceGroupOrder(groupIds: readonly string[]): void {
    const remoteReferences = groupIds.map((groupId) => parseGpuiRemotePresentationGroupId(groupId));
    if (remoteReferences.some(Boolean)) {
      /*
      CDXC:RemoteGroupReorder 2026-07-12:
      A machine-scoped remote group reorder persists as an app-local order
      overlay for that machine's presentation projection. Mixed local/remote or
      cross-machine lists stay rejected.
      */
      const machineId = remoteReferences[0]?.machineId;
      if (!machineId || remoteReferences.some((reference) => reference?.machineId !== machineId)) {
        return;
      }
      this.remoteGroupOrderByMachineId.set(
        machineId,
        remoteReferences.map((reference) => reference!.projectId),
      );
      writeStoredGpuiRemoteGroupOrder(this.remoteGroupOrderByMachineId);
      if (this.presentation) {
        this.publishPresentation("patch");
      } else {
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const before = this.workspaceGroups;
    const projectIds = groupIds
      .map((groupId) => parseGxserverPresentationProjectGroupId(groupId))
      .filter((projectId): projectId is string => Boolean(projectId));
    if (projectIds.length > 0) {
      this.workspaceGroups = syncGpuiWorkspaceProjectOrder(
        this.workspaceGroups,
        this.normalizeWorkspaceProjectOrder(projectIds),
      );
    }
    const subgroupOrderByProject = new Map<string, string[]>();
    for (const groupId of groupIds) {
      const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
      if (subgroup) {
        const order = subgroupOrderByProject.get(subgroup.projectId) ?? [];
        order.push(subgroup.groupId);
        subgroupOrderByProject.set(subgroup.projectId, order);
      }
    }
    for (const [projectId, order] of subgroupOrderByProject) {
      this.workspaceGroups = syncGpuiWorkspaceSessionSubgroupOrder(
        this.workspaceGroups,
        projectId,
        order,
      );
    }
    if (this.workspaceGroups === before) {
      return;
    }
    this.persistWorkspaceGroups();
    this.publishPresentation("patch");
  }

  private normalizeWorkspaceProjectOrder(projectIds: readonly string[]): string[] {
    const projectIdSet = new Set(projectIds);
    const worktreeByProjectId = new Map<string, SidebarProjectWorktreeMetadata>();
    for (const group of this.latestGroups) {
      const projectId = parseGxserverPresentationProjectGroupId(group.groupId);
      const worktree = group.projectContext?.worktree;
      if (projectId && projectIdSet.has(projectId) && worktree) {
        worktreeByProjectId.set(projectId, worktree);
      }
    }

    if (this.presentation) {
      const projection = createGpuiPresentationProjectProjectionMetadata({
        domainProjects: this.domainProjects,
        presentation: this.presentation,
        projectOrder: projectIds,
        recentProjects: this.recentProjects,
      });
      for (const overlay of projection.projectOverlays) {
        if (projectIdSet.has(overlay.projectId) && overlay.worktree) {
          worktreeByProjectId.set(overlay.projectId, overlay.worktree);
        }
      }
    }

    return orderProjectsWithWorktrees(
      projectIds.map((projectId) => ({
        projectId,
        worktree: worktreeByProjectId.get(projectId),
      })),
    ).map((project) => project.projectId);
  }

  private syncWorkspaceSubgroupSessionOrder(groupId: string, sessionIds: readonly string[]): void {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const rawSessionIds = sessionIds
      .map((sessionId) => parseGxserverPresentationProjectSessionId(sessionId))
      .filter(
        (reference): reference is NonNullable<typeof reference> =>
          reference !== undefined && reference.projectId === subgroup.projectId,
      )
      .map((reference) => reference.sessionId);
    const next = syncGpuiWorkspaceSessionOrderInSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
      rawSessionIds,
    );
    if (next === this.workspaceGroups) {
      return;
    }
    this.workspaceGroups = next;
    this.persistWorkspaceGroups();
    this.publishPresentation("patch");
  }

  private workspaceSubgroupSidebarIdForSession(
    projectId: string,
    sessionId: string | undefined,
  ): string | undefined {
    if (!sessionId) {
      return undefined;
    }
    const subgroup = findGpuiWorkspaceSessionSubgroupForSession(
      this.workspaceGroups,
      projectId,
      sessionId,
    );
    return subgroup ? createGpuiWorkspaceSessionSubgroupId(projectId, subgroup.groupId) : undefined;
  }

  private toggleCloseAfterDone(sessionId: string): void {
    const session = this.findPresentationSessionRowForSidebarSessionId(sessionId);
    if (!session) {
      this.postSidebarActionToast(
        "info",
        "Close After Done is only available for terminal sessions.",
      );
      return;
    }
    if (this.closeAfterDoneTimersBySessionId.has(sessionId)) {
      this.clearCloseAfterDoneTimer(sessionId);
      this.publishPresentation("patch");
      this.postSidebarActionToast("info", "Close After Done canceled");
      return;
    }
    this.closeAfterDoneTimersBySessionId.set(sessionId, {});
    this.persistCloseAfterDoneSessionIds();
    this.refreshCloseAfterDoneTimer(sessionId, Date.now());
    this.publishPresentation("patch");
    this.postSidebarActionToast("info", "Close After Done enabled", {
      description: "Closes after Done stays visible for 3m.",
    });
  }

  private findPresentationSessionRowForSidebarSessionId(
    sessionId: string,
  ): GxserverPresentationSession | undefined {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      return this.findRemotePresentationSession(remoteSession);
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference) {
      return undefined;
    }
    return this.presentation?.sessions.find(
      (session) =>
        session.projectId === reference.projectId && session.sessionId === reference.sessionId,
    );
  }

  private refreshCloseAfterDoneTimers(): void {
    const nowMs = Date.now();
    for (const sessionId of [...this.closeAfterDoneTimersBySessionId.keys()]) {
      this.refreshCloseAfterDoneTimer(sessionId, nowMs);
    }
  }

  private refreshCloseAfterDoneTimer(sessionId: string, nowMs: number): void {
    const timer = this.closeAfterDoneTimersBySessionId.get(sessionId);
    if (!timer) {
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const snapshotAvailable = remoteSession
      ? this.remotePresentations.has(remoteSession.machineId)
      : this.presentation !== undefined;
    if (!snapshotAvailable) {
      this.resetCloseAfterDoneCountdown(sessionId, timer);
      return;
    }
    const session = this.findPresentationSessionRowForSidebarSessionId(sessionId);
    if (!session) {
      this.clearCloseAfterDoneTimer(sessionId);
      return;
    }
    if (!isGpuiCloseAfterDonePresentationSessionDone(session)) {
      this.resetCloseAfterDoneCountdown(sessionId, timer);
      return;
    }
    if (timer.deadlineAtMs !== undefined) {
      this.ensureCloseAfterDoneCountdownTicker();
      return;
    }
    const deadlineAtMs = nowMs + GPUI_CLOSE_AFTER_DONE_DELAY_MS;
    const timeoutId = window.setTimeout(() => {
      this.completeCloseAfterDoneTimer(sessionId, deadlineAtMs);
    }, GPUI_CLOSE_AFTER_DONE_DELAY_MS);
    this.closeAfterDoneTimersBySessionId.set(sessionId, {
      deadlineAtMs,
      doneSinceAtMs: nowMs,
      timeoutId,
    });
    this.ensureCloseAfterDoneCountdownTicker();
  }

  private resetCloseAfterDoneCountdown(sessionId: string, timer: GpuiCloseAfterDoneTimer): void {
    if (timer.timeoutId !== undefined) {
      window.clearTimeout(timer.timeoutId);
    }
    this.closeAfterDoneTimersBySessionId.set(sessionId, {});
    this.stopCloseAfterDoneCountdownTickerIfIdle();
  }

  private completeCloseAfterDoneTimer(sessionId: string, expectedDeadlineAtMs: number): void {
    const timer = this.closeAfterDoneTimersBySessionId.get(sessionId);
    if (!timer || timer.deadlineAtMs !== expectedDeadlineAtMs) {
      return;
    }
    const session = this.findPresentationSessionRowForSidebarSessionId(sessionId);
    if (!session || !isGpuiCloseAfterDonePresentationSessionDone(session)) {
      this.resetCloseAfterDoneCountdown(sessionId, timer);
      this.publishPresentation("patch");
      return;
    }
    this.clearCloseAfterDoneTimer(sessionId);
    void this.transitionSession(sessionId, "close");
  }

  private clearCloseAfterDoneTimer(sessionId: string): void {
    const timer = this.closeAfterDoneTimersBySessionId.get(sessionId);
    if (timer?.timeoutId !== undefined) {
      window.clearTimeout(timer.timeoutId);
    }
    this.closeAfterDoneTimersBySessionId.delete(sessionId);
    this.persistCloseAfterDoneSessionIds();
    this.stopCloseAfterDoneCountdownTickerIfIdle();
  }

  private persistCloseAfterDoneSessionIds(): void {
    writeStoredGpuiCloseAfterDoneSessionIds([...this.closeAfterDoneTimersBySessionId.keys()]);
  }

  private ensureCloseAfterDoneCountdownTicker(): void {
    if (this.closeAfterDoneCountdownTickerId !== undefined) {
      return;
    }
    this.closeAfterDoneCountdownTickerId = window.setInterval(() => {
      if (!this.hasActiveCloseAfterDoneCountdown()) {
        this.stopCloseAfterDoneCountdownTickerIfIdle();
        return;
      }
      this.publishPresentation("patch");
    }, 1_000);
  }

  private stopCloseAfterDoneCountdownTickerIfIdle(): void {
    if (
      this.hasActiveCloseAfterDoneCountdown() ||
      this.closeAfterDoneCountdownTickerId === undefined
    ) {
      return;
    }
    window.clearInterval(this.closeAfterDoneCountdownTickerId);
    this.closeAfterDoneCountdownTickerId = undefined;
  }

  private hasActiveCloseAfterDoneCountdown(): boolean {
    for (const timer of this.closeAfterDoneTimersBySessionId.values()) {
      if (timer.deadlineAtMs !== undefined) {
        return true;
      }
    }
    return false;
  }

  private getCloseAfterDoneProjection(
    sessionId: string,
  ): GxserverPresentationCloseAfterDoneProjection | undefined {
    const timer = this.closeAfterDoneTimersBySessionId.get(sessionId);
    if (!timer) {
      return undefined;
    }
    if (timer.deadlineAtMs === undefined) {
      return { armed: true };
    }
    const remainingMs = Math.max(0, timer.deadlineAtMs - Date.now());
    return {
      armed: true,
      deadlineAt: new Date(timer.deadlineAtMs).toISOString(),
      remainingLabel: formatGpuiCloseAfterDoneCountdown(remainingMs),
      remainingMs,
    };
  }

  private getDelayedSendProjection(
    sessionId: string,
  ): GxserverPresentationDelayedSendProjection | undefined {
    const delayedSend = this.workspaceSessionDelayedSends.get(sessionId);
    if (!delayedSend) {
      return undefined;
    }
    return {
      deadlineAt: delayedSend.delayedSendDeadlineAt,
      remainingLabel: delayedSend.delayedSendRemainingLabel,
      remainingMs: delayedSend.delayedSendRemainingMs,
      sendWhenAllProjectSessionsStopActive:
        delayedSend.sendWhenAllProjectSessionsStopActive === true ? true : undefined,
      sendWhenAgentStopsActive:
        delayedSend.sendWhenAgentStopsActive === true ? true : undefined,
    };
  }

  private transitionWorkspaceTerminalLifecycleClose(
    request: GpuiWorkspaceTerminalLifecycleRequest,
    fallbackReplacementSessionId: string | undefined,
  ): boolean {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:59:
    Rust-origin mapped Agents close matches macOS local-first behavior: hide/remove the SidebarApp row and focus the Rust-provided or project-list replacement locally, then attempt gxserver `/api/transitionSession` best-effort. Provider transition failure must not keep a retryable Ghostty close-confirm prompt or block the native tab close.
    */
    this.removePresentationSession(request.projectId, request.sessionId);
    const replacementProjectId = request.replacementProjectId ?? request.projectId;
    const replacementSessionId = request.replacementSessionId ?? fallbackReplacementSessionId;
    if (replacementSessionId) {
      this.focusLocalWorkspaceSession(replacementProjectId, replacementSessionId);
      this.publishPresentation("patch");
    }
    const client = this.client;
    if (client) {
      void client
        .rpc<GxserverSessionTransitionResult>("/api/transitionSession", {
          action: request.action,
          projectId: request.projectId,
          reason: "closeTerminal",
          sessionId: request.sessionId,
        })
        .catch(() => undefined);
    }
    return true;
  }

  private workspaceTerminalLifecycleResultBridgeReady(): boolean {
    return typeof window.ghostexGpui?.postWorkspaceTerminalLifecycleResult === "function";
  }

  private handleOrQueueWorkspaceTerminalLifecycleRequest(payload: unknown): void {
    const request = normalizeGpuiWorkspaceTerminalLifecycleRequest(payload);
    if (!request) {
      return;
    }
    if (!this.workspaceTerminalLifecycleResultBridgeReady()) {
      this.queuePendingWorkspaceTerminalLifecycleRequest(request);
      return;
    }
    void this.handleNormalizedWorkspaceTerminalLifecycleRequest(request);
  }

  private queuePendingWorkspaceTerminalLifecycleRequest(
    request: GpuiWorkspaceTerminalLifecycleRequest,
  ): void {
    const gpuiBridge = (window.ghostexGpui = window.ghostexGpui ?? {});
    const pending = Array.isArray(gpuiBridge.pendingWorkspaceTerminalLifecycleRequests)
      ? gpuiBridge.pendingWorkspaceTerminalLifecycleRequests
      : [];
    pending.push(request);
    gpuiBridge.pendingWorkspaceTerminalLifecycleRequests = pending;
    this.scheduleWorkspaceTerminalLifecycleBridgeRetry();
  }

  private scheduleWorkspaceTerminalLifecycleBridgeRetry(): void {
    if (this.workspaceTerminalLifecycleBridgeRetryId !== undefined) {
      return;
    }
    this.workspaceTerminalLifecycleBridgeRetryId = window.setTimeout(() => {
      this.workspaceTerminalLifecycleBridgeRetryId = undefined;
      this.drainPendingWorkspaceTerminalLifecycleRequests();
    }, GPUI_WORKSPACE_TERMINAL_LIFECYCLE_BRIDGE_RETRY_DELAY_MS);
  }

  private drainPendingWorkspaceTerminalLifecycleRequests(
    queuedRequests?: readonly unknown[],
  ): void {
    const gpuiBridge = (window.ghostexGpui = window.ghostexGpui ?? {});
    const pending = [
      ...(queuedRequests ?? []),
      ...(Array.isArray(gpuiBridge.pendingWorkspaceTerminalLifecycleRequests)
        ? gpuiBridge.pendingWorkspaceTerminalLifecycleRequests.splice(0)
        : []),
    ];
    if (pending.length === 0) {
      return;
    }
    if (!this.workspaceTerminalLifecycleResultBridgeReady()) {
      for (const payload of pending) {
        const request = normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest(payload);
        if (request) {
          this.queuePendingWorkspaceTerminalLifecycleRequest(request);
        }
      }
      return;
    }
    for (const payload of pending) {
      const request = normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest(payload);
      if (request) {
        void this.handleNormalizedWorkspaceTerminalLifecycleRequest(request);
      }
    }
  }

  private async handleNormalizedWorkspaceTerminalLifecycleRequest(
    request: GpuiWorkspaceTerminalLifecycleRequest,
  ): Promise<void> {
    let ok = false;
    try {
      ok = await this.applyWorkspaceTerminalLifecycleRequest(request);
    } catch {
      ok = false;
    }
    this.postWorkspaceTerminalLifecycleResult(request.requestId, ok);
  }

  private async applyWorkspaceTerminalLifecycleRequest(
    request: GpuiWorkspaceTerminalLifecycleRequest,
  ): Promise<boolean> {
    const remoteProject = parseGpuiRemotePresentationProjectId(request.projectId);
    if (remoteProject) {
      return this.applyRemoteWorkspaceTerminalLifecycleRequest(request, remoteProject);
    }
    const fallbackReplacementSessionId =
      request.replacementSessionId === undefined && !request.skipReplacementFallback
        ? this.resolveLocalProjectListTransitionFocusTarget(request.projectId, request.sessionId)
        : undefined;
    if (request.action === "close") {
      /*
      CDXC:GPUIWorkspaceLifecycle 2026-07-10:
      Close is local-first and must hide the sidebar row even when gxserver is
      disconnected. The provider transition is best-effort cleanup owned by
      transitionWorkspaceTerminalLifecycleClose; unlike Sleep and Wake, it is
      not a prerequisite for acknowledging the user's tab close.
      */
      return this.transitionWorkspaceTerminalLifecycleClose(request, fallbackReplacementSessionId);
    }
    if (!this.client) {
      return false;
    }
    if (request.action === "wake") {
      /*
      CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:24:
      Rust-origin mapped sleeping placeholder activation must mirror macOS wake ownership: SidebarApp/gxserver commits `/api/wakeSession`, the sidebar marks the row running, and only the result ack lets Rust move the native tab into Mounting. Do not post WorkspaceTerminalFocus from this branch or the wake request would re-enter Rust before its pending lifecycle mutation applies.
      */
      await this.client.rpc("/api/wakeSession", {
        projectId: request.projectId,
        reason: "gpui-sidebar",
        sessionId: request.sessionId,
      });
      this.patchPresentationSession(request.projectId, request.sessionId, {
        lifecycleState: "running",
      });
      this.setLocalPresentationSessionFocus(request.projectId, request.sessionId);
      this.publishPresentation("patch");
      return true;
    }
    const result = await this.client.rpc<GxserverSessionTransitionResult>(
      "/api/transitionSession",
      {
        action: request.action,
        projectId: request.projectId,
        reason: "sleepSession",
        sessionId: request.sessionId,
      },
    );
    if (!shouldApplyGpuiLocalWorkspaceTransition(result, request.action)) {
      return false;
    }
    this.patchPresentationSession(request.projectId, request.sessionId, {
      lifecycleState: "sleeping",
    });
    const replacementProjectId = request.replacementProjectId ?? request.projectId;
    const replacementSessionId = request.replacementSessionId ?? fallbackReplacementSessionId;
    if (replacementSessionId) {
      this.focusLocalWorkspaceSession(replacementProjectId, replacementSessionId);
      this.publishPresentation("patch");
    }
    return true;
  }

  private async applyRemoteWorkspaceTerminalLifecycleRequest(
    request: GpuiWorkspaceTerminalLifecycleRequest,
    remoteProject: GpuiRemoteProjectReference,
  ): Promise<boolean> {
    const scopedSessionId = createGpuiRemotePresentationSessionId(
      remoteProject.machineId,
      remoteProject.projectId,
      request.sessionId,
    );
    const replacementProject = request.replacementProjectId
      ? parseGpuiRemotePresentationProjectId(request.replacementProjectId)
      : undefined;
    const focusReplacement = (): void => {
      if (
        replacementProject &&
        request.replacementSessionId &&
        replacementProject.machineId === remoteProject.machineId
      ) {
        const replacementReference = {
          machineId: replacementProject.machineId,
          projectId: replacementProject.projectId,
          sessionId: request.replacementSessionId,
        };
        /*
        CDXC:GPUIRemoteWorkspaceLifecycle 2026-08-08:
        A remote direct close must focus its surviving terminal through the
        same native open/focus bridge as a remote sidebar session click. A
        presentation-only focus update selects the replacement row but never
        transfers AppKit/GPUI keyboard ownership, leaving both the Agents pane
        and project-editor companion unable to type until clicked.
        */
        this.postRemoteSessionNativeAction(
          "openRemoteSessionTerminal",
          replacementReference,
          {
            sessionId: createGpuiRemotePresentationSessionId(
              replacementReference.machineId,
              replacementReference.projectId,
              replacementReference.sessionId,
            ),
            type: "focusSession",
          },
        );
        this.setRemotePresentationSessionFocus(replacementReference);
      }
    };

    if (request.action === "close") {
      const presentation = this.remotePresentations.get(remoteProject.machineId);
      if (presentation) {
        this.remotePresentations.set(remoteProject.machineId, {
          ...presentation,
          sessions: presentation.sessions.filter(
            (session) =>
              session.projectId !== remoteProject.projectId ||
              session.sessionId !== request.sessionId,
          ),
        });
      }
      if (this.focusedSessionId === scopedSessionId) {
        this.focusedSessionId = undefined;
        this.visibleSessionIds.delete(scopedSessionId);
      }
      focusReplacement();
      this.publishRemotePresentationPatch();
      void this.requestRemoteGxserver(remoteProject.machineId, "/api/killSession", {
        projectId: remoteProject.projectId,
        reason: "closeTerminal",
        sessionId: request.sessionId,
      })
        .then(() => this.refreshRemotePresentationFromGxserver(remoteProject.machineId))
        .catch(() => undefined);
      return true;
    }

    await this.requestRemoteGxserver(
      remoteProject.machineId,
      request.action === "wake" ? "/api/wakeSession" : "/api/sleepSession",
      {
        projectId: remoteProject.projectId,
        reason: "gpui-sidebar",
        sessionId: request.sessionId,
      },
    );
    await this.refreshRemotePresentationFromGxserver(remoteProject.machineId);
    if (request.action === "wake") {
      this.setRemotePresentationSessionFocus({
        machineId: remoteProject.machineId,
        projectId: remoteProject.projectId,
        sessionId: request.sessionId,
      });
    } else {
      focusReplacement();
    }
    return true;
  }

  private postWorkspaceTerminalLifecycleResult(requestId: number, ok: boolean): void {
    const postResult = window.ghostexGpui?.postWorkspaceTerminalLifecycleResult;
    if (typeof postResult !== "function") {
      return;
    }
    const payload = JSON.stringify({
      ok,
      requestId,
      type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION,
    });
    postResult(payload);
  }

  private resolveLocalProjectListTransitionFocusTarget(
    projectId: string,
    removedSessionId: string,
  ): string | undefined {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:34:
    Sidebar-origin local close/sleep must follow the macOS project-list focus rule: background transitions do not steal focus, while closing or sleeping the focused session selects the next running row from the same displayed local project order and routes it through the workspace focus bridge.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedRemovedSessionId = normalizeNonEmptyString(removedSessionId);
    if (
      !normalizedProjectId ||
      !normalizedRemovedSessionId ||
      this.focusedSessionId !== normalizedRemovedSessionId
    ) {
      return undefined;
    }
    const orderedSessionIds = this.localProjectTransitionSessionIds(
      normalizedProjectId,
      normalizedRemovedSessionId,
    );
    const removedIndex = orderedSessionIds.indexOf(normalizedRemovedSessionId);
    const candidates =
      removedIndex >= 0
        ? [
            ...orderedSessionIds.slice(removedIndex + 1),
            ...orderedSessionIds.slice(0, removedIndex),
          ]
        : orderedSessionIds;
    const replacementSessionId = candidates.find(
      (candidateSessionId) =>
        candidateSessionId !== normalizedRemovedSessionId &&
        this.isRunningLocalPresentationSession(normalizedProjectId, candidateSessionId),
    );
    return replacementSessionId;
  }

  private localProjectTransitionSessionIds(projectId: string, removedSessionId: string): string[] {
    const orderedSessionIds: string[] = [];
    const addSessionId = (sessionId: string | undefined): void => {
      const normalizedSessionId = normalizeNonEmptyString(sessionId);
      if (!normalizedSessionId || orderedSessionIds.includes(normalizedSessionId)) {
        return;
      }
      orderedSessionIds.push(normalizedSessionId);
    };
    for (const group of this.latestGroups) {
      for (const session of group.sessions) {
        if (parseGpuiRemotePresentationSessionId(session.sessionId)) {
          continue;
        }
        const reference = parseGxserverPresentationProjectSessionId(session.sessionId);
        if (reference?.projectId === projectId) {
          addSessionId(reference.sessionId);
        }
      }
    }
    for (const session of this.presentation?.sessions ?? []) {
      if (session.projectId === projectId) {
        addSessionId(session.sessionId);
      }
    }
    addSessionId(removedSessionId);
    return orderedSessionIds;
  }

  private isRunningLocalPresentationSession(projectId: string, sessionId: string): boolean {
    return (
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === projectId &&
          session.sessionId === sessionId &&
          session.lifecycleState === "running",
      ) ?? false
    );
  }

  private isLocalPresentationT3Session(projectId: string, sessionId: string): boolean {
    return (
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === projectId &&
          session.sessionId === sessionId &&
          session.kind === "t3",
      ) ?? false
    );
  }

  private isSleepingLocalPresentationSession(projectId: string, sessionId: string): boolean {
    const presentationSleeping =
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === projectId &&
          session.sessionId === sessionId &&
          session.lifecycleState === "sleeping",
      ) ?? false;
    if (presentationSleeping) {
      return true;
    }
    const sidebarSessionId = createGxserverPresentationProjectSessionId(projectId, sessionId);
    if (this.sleepingLocalSidebarSessionIds.has(sidebarSessionId)) {
      return true;
    }
    return this.latestGroups.some((group) =>
      group.sessions.some(
        (session) =>
          session.sessionId === sidebarSessionId &&
          (session.lifecycleState === "sleeping" || session.isSleeping === true),
      ),
    );
  }

  private async forkSession(sessionId: string): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      /*
      CDXC:GPUIRemoteSessions 2026-06-24-17:19:
      Remote fork authority comes only from a machine-prefixed session id already present in the remote presentation snapshot. Route the project/session ids to `/api/forkSession` on that machine; do not derive ids from labels or terminal text.

      CDXC:GPUIForkParity 2026-07-10:
      Match macOS remote Fork exactly: the owning gxserver creates the fork and
      the refreshed remote presentation renders it without moving focus away
      from the session the user was viewing.
      */
      try {
        await this.requestRemoteGxserver(remoteSession.machineId, "/api/forkSession", {
          projectId: remoteSession.projectId,
          reason: "gpui-sidebar",
          sessionId: remoteSession.sessionId,
        });
        await this.refreshRemotePresentationFromGxserver(remoteSession.machineId).catch(
          () => undefined,
        );
      } catch (error) {
        this.postRemoteToast("error", "Remote fork failed", {
          description: error instanceof Error ? error.message : String(error),
        });
      }
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !this.client) {
      return;
    }
    if (
      !this.presentation?.sessions.some(
        (session) =>
          session.projectId === reference.projectId && session.sessionId === reference.sessionId,
      )
    ) {
      return;
    }

    const sourceGroupId =
      this.workspaceSubgroupSidebarIdForSession(reference.projectId, reference.sessionId) ??
      createGxserverPresentationProjectGroupId(reference.projectId);
    if (this.activeProjectId !== reference.projectId || this.activeGroupId !== sourceGroupId) {
      /*
      CDXC:GPUIForkParity 2026-07-10:
      macOS focuses the clicked session's project before awaiting gxserver.
      GPUI also activates its clicked sidebar subgroup so Rust has the source
      tab-group mapping before the fork result arrives.
      */
      this.activeProjectId = reference.projectId;
      this.activeGroupId = sourceGroupId;
      this.refreshSidebarHudFromClient();
      this.publishPresentation("patch");
    }

    try {
      /*
      CDXC:GPUIForkParity 2026-07-10:
      `/api/forkSession` returns `{ fork }`, exactly as the macOS gxserver
      client unwraps it. The previous GPUI code treated the result itself as
      the fork payload, so `response.session` was undefined and the action
      could not materialize or focus the returned G-session.
      */
      const { fork } = await this.client.rpc<{ fork: GxserverForkSessionResult }>(
        "/api/forkSession",
        {
          projectId: reference.projectId,
          reason: "gpui-sidebar",
          sessionId: reference.sessionId,
        },
      );
      const forkedSessionId = normalizeNonEmptyString(fork?.session.sessionId);
      if (!forkedSessionId) {
        throw new Error("gxserver did not return the forked session.");
      }

      const sourceSubgroup = parseGpuiWorkspaceSessionSubgroupId(sourceGroupId);
      if (sourceSubgroup) {
        this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
          this.workspaceGroups,
          reference.projectId,
          forkedSessionId,
          sourceSubgroup.groupId,
        );
        this.persistWorkspaceGroups();
      }

      this.setLocalPresentationSessionFocus(reference.projectId, forkedSessionId, sourceGroupId);
      this.publishPresentation("patch");
      /*
      The placement target is the clicked source session, not whichever pane
      happens to be focused when the RPC completes. Rust resolves this bounded
      id to the existing pane and appends the fork there before mounting the
      gxserver attach plan, matching macOS appendToTabGroup behavior.
      */
      this.postLocalWorkspaceTerminalFocus(
        reference.projectId,
        forkedSessionId,
        reference.sessionId,
      );
      await this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => undefined);
    } catch (error) {
      this.postSidebarActionToast("error", "Could not fork session", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private async renameSession(
    message: Extract<SidebarToExtensionMessage, { type: "renameSession" }>,
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
    if (remoteSession) {
      /*
      CDXC:SessionHistoryTitleSource 2026-07-29:
      Empty-title Generate Name is a local-transcript flow; a remote machine's
      transcripts are not readable here, and a blank direct rename would erase
      the remote title.
      */
      if (!message.title.trim()) {
        return;
      }
      this.postRemoteGxserverSidebarRequest(remoteSession.machineId, "/api/updateSession", {
        projectId: remoteSession.projectId,
        sessionId: remoteSession.sessionId,
        title: message.title,
      });
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(message.sessionId);
    if (!reference || !this.client) {
      return;
    }
    if (message.shouldGenerateTitle) {
      /*
      CDXC:GPUISidebarRename 2026-07-29:
      Generate Name reuses the first-message auto-title UX end to end:
      gxserver marks the session generating (the card shows the same
      "Generating title…" chrome), summarizes the pasted text with the chosen
      generation agent, stages the agent rename command through zmx with the
      same delayed real Enter, and applies the generated title. The long
      pasted text must never reach `/api/requestSessionRename` as a literal
      title.
      */
      const generationAgent = this.resolveSidebarAgent(message.agentId ?? "");
      const generationCommand = generationAgent?.command?.trim();
      await this.client.rpc("/api/generateSessionTitle", {
        ...(message.agentId ? { agentId: message.agentId } : {}),
        ...(generationCommand ? { command: generationCommand } : {}),
        projectId: reference.projectId,
        sessionId: reference.sessionId,
        text: message.title,
      });
      return;
    }
    const result = await this.client.rpc<GxserverSessionRenameRequestResult>(
      "/api/requestSessionRename",
      {
        agentName: message.agentId,
        projectId: reference.projectId,
        reason: "gpui-sidebar",
        sessionId: reference.sessionId,
        title: message.title,
        titleSource: "user",
      },
    );
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      title: message.title,
    });
    /*
    CDXC:GPUISidebarRename 2026-07-28:
    gxserver keeps agent-session renames pending until the Agent CLI itself is
    renamed, and it answers `shouldSendAgentRenameCommand` so the client stages
    `/rename <title>` (Pi uses `/name`; Hermes Agent uses `/title`) into the
    mapped terminal — the same contract macOS follows.
    */
    if (result.shouldSendAgentRenameCommand) {
      this.postLocalWorkspaceTerminalRenameCommand(
        reference.projectId,
        reference.sessionId,
        message.title,
      );
    }
  }

  private async updateSessionFlags(
    sessionId: string,
    flags: { isFavorite?: boolean; isPinned?: boolean; sessionTag?: SidebarSessionTag | null },
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.postRemoteGxserverSidebarRequest(remoteSession.machineId, "/api/updateSession", {
        ...flags,
        projectId: remoteSession.projectId,
        sessionId: remoteSession.sessionId,
      });
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !this.client) {
      return;
    }
    await this.client.rpc("/api/updateSession", {
      ...flags,
      projectId: reference.projectId,
      sessionId: reference.sessionId,
    });
    this.patchPresentationSession(reference.projectId, reference.sessionId, flags);
  }

  /*
  CDXC:SidebarV2Lifecycle 2026-07-29:
  One code path for settle/unsettle/snooze/unsnooze, local and remote.

  - Routing mirrors `updateSessionFlags`: a remote-prefixed sidebar session id
    resolves to (machineId, projectId, sessionId) and goes over the Rust remote
    bridge to THAT machine's daemon; anything else is local. The renderer never
    picks a daemon by anything other than the id the host itself minted.
  - The response is awaited (not fire-and-forget) so a guard rejection — settling
    a working session, snoozing a session that is blocked on the user, a wake
    time in the past — surfaces as a toast instead of a row that silently never
    moves. The toast carries no session title, path, or daemon body.
  - No local presentation patch: gxserver emits the delta, and inventing one
    here would fight the server's guards and desync the settled/snoozed shelves.
  */
  private async runSessionLifecycleCommand(
    sessionId: string,
    path: Extract<
      GxserverEndpointPath,
      | "/api/settleSession"
      | "/api/snoozeSession"
      | "/api/unsettleSession"
      | "/api/unsnoozeSession"
    >,
    params: Record<string, unknown>,
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    try {
      if (remoteSession) {
        await this.requestRemoteGxserver(remoteSession.machineId, path, {
          ...params,
          projectId: remoteSession.projectId,
          sessionId: remoteSession.sessionId,
        });
        return;
      }
      const reference = parseGxserverPresentationProjectSessionId(sessionId);
      if (!reference || !this.client) {
        return;
      }
      await this.client.rpc(path, {
        ...params,
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      });
    } catch {
      this.postSidebarActionToast("warning", SESSION_LIFECYCLE_FAILURE_TITLES[path], {
        description: "gxserver refused the change. The session may be working or waiting on you.",
      });
    }
  }

  private async syncSessionOrder(groupId: string, sessionIds: readonly string[]): Promise<void> {
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.client || !this.presentation) {
      return;
    }
    const gxserverSessionIds = sessionIds.flatMap((sessionId) => {
      const reference = parseGxserverPresentationProjectSessionId(sessionId);
      return reference?.projectId === projectId ? [reference.sessionId] : [];
    });
    if (gxserverSessionIds.length === 0) {
      return;
    }
    this.presentation = reorderPresentationProjectSessions(
      this.presentation,
      projectId as GxserverProjectId,
      gxserverSessionIds as GxserverSessionId[],
    );
    this.publishPresentation("patch");
    await this.client.rpc("/api/updateSessionOrder", {
      projectId,
      sessionIds: gxserverSessionIds,
    });
  }

  private async requestPreviousSessions(
    message: Extract<SidebarToExtensionMessage, { type: "requestPreviousSessions" }>,
  ): Promise<void> {
    const limit = message.limit ?? 80;
    const sessionTags = message.sessionTags;
    const remoteMachines = this.connectedRemotePreviousSessionMachines();
    try {
      const [localResponse, ...remoteResponses] = await Promise.all([
        this.client
          ? this.client
              .rpc<GxserverPresentationSearchResponse>("/api/listPreviousSessions", {
                cursor: message.cursor,
                includeActive: false,
                includePrevious: true,
                limit,
                query: message.query,
                sessionTags,
              })
              .catch(() => ({ results: [] }))
          : Promise.resolve({ results: [] }),
        ...remoteMachines.map((machine) =>
          this.requestRemoteGxserver<GxserverPresentationSearchResponse>(
            machine.machineId,
            "/api/listPreviousSessions",
            {
              cursor: message.cursor,
              includeActive: false,
              includePrevious: true,
              limit,
              query: message.query,
              sessionTags,
            },
          ).catch(() => ({ results: [] })),
        ),
      ]);
      /*
      CDXC:GPUIRemotePreviousSessions 2026-06-24-17:19:
      Previous-session list/search combines local gxserver rows with connected remote gxserver rows, but remote history ids are machine-prefixed so restore/delete can route back through Rust's tunnel owner. Keep only the current result page in memory and do not persist remote metadata in GPUI.
      */
      const remoteItems = remoteResponses.flatMap((response, index) =>
        response.results.map((result) =>
          gxserverSearchResultToPreviousSessionItem(result, {
            historyIdPrefix: `remote-gxserver:${remoteMachines[index]?.machineId ?? ""}`,
            projectNamePrefix: remoteMachines[index]?.machineName,
          }),
        ),
      );
      this.postPreviousSessionsResult(
        message.requestId,
        message.query,
        [...localResponse.results.map(gxserverSearchResultToPreviousSessionItem), ...remoteItems]
          .sort(comparePreviousSessionItemsByClosedTime),
        localResponse.cursor ?? remoteResponses.find((response) => response.cursor)?.cursor,
      );
    } catch {
      this.postPreviousSessionsResult(message.requestId, message.query, []);
    }
  }

  private async restorePreviousSession(historyId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePreviousSessionHistoryId(historyId);
    if (remoteReference) {
      await this.restoreRemotePreviousSession(remoteReference, historyId);
      return;
    }
    const reference = parseGpuiGxserverPreviousSessionHistoryId(historyId);
    if (!reference || !this.client) {
      return;
    }
    const previousSession = this.previousSessionsByHistoryId.get(historyId);
    if (previousSession && previousSession.isRestorable !== true) {
      return;
    }
    try {
      const response = await this.client.rpc<GpuiGxserverCreatedSessionResult>(
        "/api/createSession",
        {
          kind: "terminal",
          lifecycleState: "running",
          projectId: reference.projectId,
          restoredFromSessionId: reference.sessionId,
          ...(previousSession?.sessionTag ? { sessionTag: previousSession.sessionTag } : {}),
          ...(previousSession?.sidebarOrder !== undefined
            ? { sidebarOrder: previousSession.sidebarOrder }
            : {}),
          surface: "workspace",
          title: previousSessionTitle(previousSession),
        },
      );
      const restoredSessionId = normalizeNonEmptyString(response.session?.sessionId);
      if (restoredSessionId) {
        this.focusLocalWorkspaceSession(
          normalizeNonEmptyString(response.session?.projectId) ?? reference.projectId,
          restoredSessionId,
        );
      }
      await this.client
        .rpc("/api/removeSession", {
          projectId: reference.projectId,
          reason: "restorePreviousSession",
          sessionId: reference.sessionId,
        })
        .catch(() => undefined);
      this.removePreviousSessionFromCurrentResult(historyId);
    } catch {
      this.postRemoteToast("warning", "Previous session restore failed", {
        description: "gxserver could not restore that previous session.",
      });
    }
  }

  private async restoreRemotePreviousSession(
    reference: { machineId: string; projectId: string; sessionId: string },
    historyId: string,
  ): Promise<void> {
    const previousSession = this.previousSessionsByHistoryId.get(historyId);
    if (previousSession && previousSession.isRestorable !== true) {
      return;
    }
    /*
    CDXC:GPUIRemotePreviousSessions 2026-06-24-17:19:
    Restoring remote history recreates a real workspace session on the owning remote gxserver and then removes the stopped history row from that same machine. GPUI does not create a local terminal, synthesize resume commands, or trust visible previous-session labels as operation ids.

    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    When remote previous-session restore returns a new gxserver session id, GPUI may immediately ask Rust to attach that exact restored id through the same native remote terminal action as a direct session click. If gxserver does not return the new id, the restore remains server-only instead of guessing from labels or the old history id.
    */
    try {
      const response = await this.requestRemoteGxserver<{
        session?: { projectId?: string; sessionId?: string };
      }>(reference.machineId, "/api/createSession", {
        kind: "terminal",
        lifecycleState: "running",
        projectId: reference.projectId,
        restoredFromSessionId: reference.sessionId,
        ...(previousSession?.sessionTag ? { sessionTag: previousSession.sessionTag } : {}),
        ...(previousSession?.sidebarOrder !== undefined
          ? { sidebarOrder: previousSession.sidebarOrder }
          : {}),
        surface: "workspace",
        title: previousSessionTitle(previousSession),
      });
      await this.requestRemoteGxserver(reference.machineId, "/api/removeSession", {
        projectId: reference.projectId,
        reason: "restorePreviousSession",
        sessionId: reference.sessionId,
      }).catch(() => undefined);
      this.removePreviousSessionFromCurrentResult(historyId);
      const restoredSessionId = response.session?.sessionId;
      if (restoredSessionId) {
        const restoredReference = {
          machineId: reference.machineId,
          projectId: response.session?.projectId ?? reference.projectId,
          sessionId: restoredSessionId,
        };
        this.setRemotePresentationSessionFocus(restoredReference);
        this.postRemoteSessionNativeAction("openRemoteSessionTerminal", restoredReference, {
          historyId,
          type: "restorePreviousSession",
        });
      }
    } catch {
      this.postRemoteToast("warning", "Remote restore failed", {
        description: "The remote gxserver could not restore that previous session.",
      });
    }
  }

  private async deletePreviousSession(historyId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePreviousSessionHistoryId(historyId);
    if (remoteReference) {
      await this.requestRemoteGxserver(remoteReference.machineId, "/api/removeSession", {
        projectId: remoteReference.projectId,
        reason: "deletePreviousSession",
        sessionId: remoteReference.sessionId,
      }).catch(() => undefined);
      this.removePreviousSessionFromCurrentResult(historyId);
      return;
    }
    const reference = parseGpuiGxserverPreviousSessionHistoryId(historyId);
    if (!reference || !this.client) {
      return;
    }
    await this.client
      .rpc("/api/removeSession", {
        projectId: reference.projectId,
        reason: "deletePreviousSession",
        sessionId: reference.sessionId,
      })
      .catch(() => undefined);
    this.removePreviousSessionFromCurrentResult(historyId);
  }

  private connectedRemotePreviousSessionMachines(): Array<{
    machineId: string;
    machineName: string;
  }> {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    return settings.remoteMachines.flatMap((machine) =>
      this.remotePresentations.has(machine.id)
        ? [{ machineId: machine.id, machineName: machine.name }]
        : [],
    );
  }

  private postPreviousSessionsResult(
    requestId: string,
    query: string | undefined,
    previousSessions: SidebarPreviousSessionItem[],
    cursor?: string,
  ): void {
    this.previousSessionsResult = {
      cursor,
      previousSessions,
      query,
      requestId,
    };
    for (const session of previousSessions) {
      this.previousSessionsByHistoryId.set(session.historyId, session);
    }
    this.messageSource.postMessage({
      cursor,
      previousSessions,
      query,
      requestId,
      type: "previousSessionsResult",
    });
  }

  private removePreviousSessionFromCurrentResult(historyId: string): void {
    this.previousSessionsByHistoryId.delete(historyId);
    const previousResult = this.previousSessionsResult;
    if (!previousResult) {
      return;
    }
    this.postPreviousSessionsResult(
      previousResult.requestId,
      previousResult.query,
      previousResult.previousSessions.filter((session) => session.historyId !== historyId),
      previousResult.cursor,
    );
  }

  private async requestProjectWorktrees(
    message: Extract<SidebarToExtensionMessage, { type: "requestProjectWorktrees" }>,
  ): Promise<void> {
    const requestId = message.requestId.trim();
    if (!requestId) {
      return;
    }
    if (message.remoteMachineId?.trim()) {
      await this.requestRemoteProjectWorktrees(message, requestId);
      return;
    }
    const sourceProject = this.resolveDomainProjectScope(message) ?? this.activeDomainProject();
    if (!sourceProject || !this.client) {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: "No active gxserver project is available.",
        ok: false,
      });
      return;
    }
    const parentProject = this.resolveWorktreeFamilyParentProject(sourceProject) ?? sourceProject;
    try {
      const [worktreeResult, branchResult] = await Promise.all([
        this.client.rpc<GxserverTypedOperationResult>("/api/runWorktreeAction", {
          action: "list",
          projectId: parentProject.projectId,
        }),
        this.client.rpc<GxserverTypedOperationResult>("/api/runGitAction", {
          action: "listBranches",
          projectId: parentProject.projectId,
        }),
      ]);
      if (worktreeResult.exitCode !== 0 || branchResult.exitCode !== 0) {
        throw new Error("gxserver could not read worktree metadata.");
      }
      const worktrees = createGpuiExistingWorktreeOptions(
        worktreeResult.worktrees,
        parentProject,
        sourceProject,
        this.domainProjects,
      );
      this.trustedExistingWorktreeList = {
        parentProjectId: parentProject.projectId,
        paths: new Set(worktrees.map((worktree) => worktree.path)),
        sourceProjectId: sourceProject.projectId,
      };
      this.postProjectWorktreesResult(requestId, {
        branches: normalizeGpuiWorktreeBaseBranches(branchResult.branches),
        ok: true,
        worktrees,
      });
    } catch {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: "Could not load gxserver worktrees.",
        ok: false,
      });
    }
  }

  private async requestRemoteProjectWorktrees(
    message: Extract<SidebarToExtensionMessage, { type: "requestProjectWorktrees" }>,
    requestId: string,
  ): Promise<void> {
    const sourceProject = this.resolveRemotePresentationProjectScope({
      projectId: message.projectId,
      remoteMachineId: message.remoteMachineId,
    });
    if (!sourceProject) {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: "Reconnect the remote machine before loading worktrees.",
        ok: false,
      });
      return;
    }
    try {
      const result = await this.requestRemoteGxserver<GxserverProjectWorktreeListResult>(
        sourceProject.machineId,
        "/api/listProjectWorktrees",
        {
          projectId: sourceProject.projectId,
        },
        { timeoutMs: 30_000 },
      );
      const worktrees = normalizeGpuiExistingWorktreeOptions(result.worktrees);
      this.trustedExistingWorktreeList = {
        parentProjectId: result.parentProjectId,
        paths: new Set(worktrees.map((worktree) => worktree.path)),
        remoteMachineId: sourceProject.machineId,
        sourceProjectId: result.sourceProjectId,
        worktreeKeys: new Set(
          worktrees
            .map((worktree) => worktree.worktreeKey?.trim())
            .filter((key): key is string => Boolean(key)),
        ),
      };
      this.postProjectWorktreesResult(requestId, {
        branches: normalizeGpuiWorktreeBaseBranches(result.branches),
        ok: true,
        worktrees,
      });
    } catch {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: "Could not load remote gxserver worktrees.",
        ok: false,
      });
    }
  }

  private async createProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "createProjectWorktree" }>,
  ): Promise<void> {
    const mode =
      message.mode === "openExisting" ||
      normalizeGpuiProjectPath(message.existingWorktreePath) ||
      message.existingWorktreeKey?.trim()
        ? "openExisting"
        : "create";
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast(
      "info",
      mode === "openExisting" ? "Opening worktree" : "Creating worktree",
      {
        persistent: true,
        toastId,
      },
    );
    try {
      if (message.remoteMachineId?.trim()) {
        await this.createRemoteProjectWorktree(message);
        this.trustedExistingWorktreeList = undefined;
        this.postWorktreeToast("success", "Remote worktree ready", { toastId });
        return;
      }
      if (!this.client) {
        throw new Error("gxserver is unavailable.");
      }
      const sourceProject = this.resolveDomainProjectScope(message) ?? this.activeDomainProject();
      if (!sourceProject || !normalizeGpuiProjectPath(sourceProject.path)) {
        throw new Error("Open an active code project before creating a worktree.");
      }
      if (sourceProject.isRecentProject === true) {
        throw new Error("Restore the project before creating a worktree.");
      }

      if (mode === "openExisting") {
        await this.openExistingProjectWorktree(message, sourceProject);
      } else {
        await this.createNewProjectWorktree(message, sourceProject);
      }
      this.trustedExistingWorktreeList = undefined;
      await this.refreshDomainPresentationFromClient("patch").catch(() => undefined);
      this.postWorktreeToast("success", "Worktree ready", { toastId });
    } catch (error) {
      this.postWorktreeToast(
        "error",
        mode === "openExisting" ? "Could not open worktree" : "Could not create worktree",
        {
          description: gpuiWorktreeUserVisibleErrorMessage(error),
          toastId,
        },
      );
    }
  }

  private async createNewProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "createProjectWorktree" }>,
    sourceProject: GxserverProjectDomainState,
  ): Promise<{ projectId: string; session: GpuiCreatedProjectAgentSessionRecord }> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    const prompt = message.prompt?.trim() ?? "";
    const baseBranch = message.baseBranch?.trim() ?? "";
    const agent = this.resolveSidebarAgent(message.agentId?.trim() ?? "");
    if (!prompt) {
      throw new Error("Worktree prompt is empty.");
    }
    if (!baseBranch) {
      throw new Error("Choose a base branch.");
    }
    if (!agent?.command?.trim()) {
      throw new Error("Choose an agent with a configured command.");
    }

    const parentProject = this.resolveWorktreeFamilyParentProject(sourceProject) ?? sourceProject;
    const gxserverParentProject = await this.registerDomainProjectPath(parentProject);
    let gxserverOperationProject = gxserverParentProject;
    let gxserverSetupCommandProject = gxserverParentProject;
    if (
      normalizeGpuiProjectPath(sourceProject.path) !== normalizeGpuiProjectPath(parentProject.path)
    ) {
      gxserverOperationProject = await this.registerDomainProjectPath(sourceProject);
      gxserverSetupCommandProject = gxserverOperationProject;
    }

    const target = await this.resolveUniqueWorktreeTarget(gxserverOperationProject, prompt);
    const createResult = await this.client.rpc<GxserverTypedOperationResult>(
      "/api/runWorktreeAction",
      {
        action: "create",
        baseRef: baseBranch,
        branch: target.branch,
        projectId: gxserverOperationProject.projectId,
        worktreePath: target.path,
      },
    );
    if (createResult.exitCode !== 0) {
      throw new Error("git worktree add failed.");
    }

    const gxserverWorktreeProject = await this.registerProjectPath({
      name: `${gxserverParentProject.name || gpuiProjectNameFromPath(gxserverParentProject.path ?? "")}-${target.name}`,
      path: target.path,
    });
    if (!normalizeGpuiWorktreeParentProjectId(gxserverWorktreeProject.worktree)) {
      throw new Error("gxserver did not register the new checkout as a worktree project.");
    }
    await this.ensureWorktreeBeadsHooks(gxserverWorktreeProject);
    await this.runWorktreeSetupCommandIfConfigured(
      gxserverWorktreeProject,
      gxserverSetupCommandProject,
    );
    const session = await this.createAgentSessionRecordForProject(
      gxserverWorktreeProject,
      agent,
      prompt,
    );
    this.focusProjectId(gxserverWorktreeProject.projectId);
    return { projectId: gxserverWorktreeProject.projectId, session };
  }

  private async createRemoteProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "createProjectWorktree" }>,
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({
      projectId: message.projectId,
      remoteMachineId: message.remoteMachineId,
    });
    if (!remoteScope) {
      throw new Error("Reconnect the remote machine before creating a worktree.");
    }
    const mode =
      message.mode === "openExisting" || message.existingWorktreeKey?.trim()
        ? "openExisting"
        : "create";
    const prompt = message.prompt?.trim() ?? "";
    const agentId = message.agentId?.trim() ?? "";
    const agentTitle = createAgentSessionDefaultTitle(
      this.resolveSidebarAgent(agentId)?.name ?? agentId,
    );
    /*
    CDXC:RemoteWorktrees 2026-06-24-18:40:
    GPUI remote Add Worktree submits only the selected remote project id plus
    bounded create/open labels to gxserver. The remote daemon derives checkout
    paths, branch names, and open-existing worktree paths; GPUI preserves the
    shared modal's optional Open Existing prompt behavior by creating an agent
    session after the daemon returns a registered project id.
    */
    if (mode === "openExisting") {
      const worktreeKey = message.existingWorktreeKey?.trim() ?? "";
      if (!worktreeKey || !this.isTrustedRemoteExistingWorktreeKey(worktreeKey, remoteScope)) {
        throw new Error("Choose an existing remote worktree from the latest worktree list.");
      }
      const response = await this.requestRemoteGxserver<{
        project?: GxserverPresentationProject;
      }>(
        remoteScope.machineId,
        "/api/openProjectWorktree",
        {
          projectId: remoteScope.projectId,
          worktreeKey,
        },
        { timeoutMs: 45_000 },
      );
      const project = await this.resolveRemoteWorktreeMutationProject(
        remoteScope.machineId,
        response.project,
      );
      if (prompt) {
        if (!agentId) {
          throw new Error("Choose an agent before starting a remote worktree prompt.");
        }
        await this.createRemoteAgentSessionForProject(
          { machineId: remoteScope.machineId, projectId: project.projectId },
          agentId,
          prompt,
          agentTitle,
        );
      }
      return;
    }

    const baseRef = message.baseBranch?.trim() ?? "";
    if (!prompt) {
      throw new Error("Worktree prompt is empty.");
    }
    if (!baseRef) {
      throw new Error("Choose a base branch.");
    }
    if (!agentId) {
      throw new Error("Choose an agent before creating a remote worktree.");
    }
    const response = await this.requestRemoteGxserver<{
      project?: GxserverPresentationProject;
    }>(
      remoteScope.machineId,
      "/api/createProjectWorktree",
      {
        baseRef,
        nameHint: gpuiWorktreeSlugFromPrompt(prompt),
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 90_000 },
    );
    const project = await this.resolveRemoteWorktreeMutationProject(
      remoteScope.machineId,
      response.project,
    );
    await this.createRemoteAgentSessionForProject(
      { machineId: remoteScope.machineId, projectId: project.projectId },
      agentId,
      prompt,
      agentTitle,
    );
  }

  private async openExistingProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "createProjectWorktree" }>,
    sourceProject: GxserverProjectDomainState,
  ): Promise<void> {
    const existingWorktreePath = normalizeGpuiProjectPath(message.existingWorktreePath);
    if (!existingWorktreePath) {
      throw new Error("Choose an existing worktree.");
    }
    const parentProject = this.resolveWorktreeFamilyParentProject(sourceProject) ?? sourceProject;
    if (!this.isTrustedExistingWorktreePath(existingWorktreePath, sourceProject, parentProject)) {
      throw new Error("Choose an existing worktree from the latest worktree list.");
    }
    const gxserverWorktreeProject = await this.registerProjectPath({
      name: gpuiProjectNameFromPath(existingWorktreePath),
      path: existingWorktreePath,
    });
    if (!normalizeGpuiWorktreeParentProjectId(gxserverWorktreeProject.worktree)) {
      throw new Error("The selected checkout is not a registered worktree.");
    }
    await this.ensureWorktreeBeadsHooks(gxserverWorktreeProject);
    const prompt = message.prompt?.trim() ?? "";
    const agent = this.resolveSidebarAgent(message.agentId?.trim() ?? "");
    if (prompt && !agent?.command?.trim()) {
      throw new Error("Choose an agent with a configured command.");
    }
    if (prompt && agent) {
      await this.createAgentSessionForProject(gxserverWorktreeProject, agent, prompt);
    }
    this.focusProjectId(gxserverWorktreeProject.projectId);
  }

  private postProjectWorktreesResult(
    requestId: string,
    result: {
      branches?: unknown;
      error?: string;
      ok: boolean;
      worktrees?: unknown;
    },
  ): void {
    /*
    CDXC:SidebarV2Worktree 2026-07-29:
    The SAME answer also goes to the sidebar document, because Sidebar V2's
    worktree popover asks this question from inside the sidebar itself rather
    than from the app-modal window. Both listeners match on their own
    `requestId`, so each ignores the other's answers and neither had to grow a
    second host implementation of the branch/worktree probe.
    */
    this.messageSource.postMessage({
      branches: result.branches,
      error: result.error,
      ok: result.ok,
      requestId,
      type: "projectWorktreesResult",
      worktrees: result.worktrees,
    });
    // The Worktree modal lives in the native app-modal window, not in
    // SidebarApp, so the branch/worktree list answer must travel the app-modal
    // host route (the macOS reply path) to reach it.
    try {
      postAppModalHostMessage(
        {
          branches: result.branches,
          error: result.error,
          ok: result.ok,
          requestId,
          type: "projectWorktreesResult",
          worktrees: result.worktrees,
        },
        "AppModals:gpuiWorktree.projectWorktreesResult",
      );
    } catch {
      // Without the app-modal bridge there is no modal window waiting on this
      // request, so the answer has no destination.
    }
  }

  /*
  CDXC:SidebarV2Worktree 2026-07-29:
  Sidebar V2's worktree flow is ONE gxserver call, not a client-orchestrated
  sequence. The daemon creates the checkout, runs the project's setup command,
  spawns the session with cwd=worktree, sends the optional first prompt, and
  rolls the whole thing back if any step fails — so this method cannot leave a
  half-made worktree behind the way the older client-driven Add Worktree path
  could.

  Three deliberate choices here:
  - The sidebar id is the input, gxserver ids are derived. `message.projectId`
    is the V2 row's project/group id; only the host turns it into a daemon +
    project, exactly like the settle/snooze path.
  - REMOTE machines route to their OWN daemon over the Rust bridge, exactly
    like the settle/snooze path (CDXC:SidebarV2LogicalProjects 2026-07-29 — the
    bridge allow-list now carries both worktree endpoints with param shapers).
    The daemon that owns the repository is the only one that can cut a checkout
    in it, so the call goes to the machine, never to the local gxserver with a
    remote project id.
  - The created session is focused HERE (same helper quick-create uses) rather
    than by the sidebar, because only the host knows the workspace pane the
    session has to mount into.
  */
  private async createWorktreeSession(
    message: Extract<SidebarToExtensionMessage, { type: "createWorktreeSession" }>,
  ): Promise<void> {
    const requestId = message.requestId.trim();
    if (!requestId) {
      return;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(message.projectId);
    if (remoteGroup) {
      await this.createRemoteWorktreeSession(remoteGroup, message, requestId);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(message.projectId);
    if (!projectId || !this.client) {
      this.postWorktreeSessionResult(requestId, {
        error: "Open a code project before creating a worktree session.",
        ok: false,
      });
      return;
    }
    const existingWorktreePath = normalizeGpuiProjectPath(message.existingWorktreePath);
    const agentId = message.agentId?.trim() ?? "";
    const baseBranch = message.baseBranch?.trim() ?? "";
    const firstPrompt = message.firstPrompt?.trim() ?? "";
    try {
      const result = await this.client.rpc<GxserverCreateWorktreeSessionResult>(
        "/api/createWorktreeSession",
        {
          ...(agentId ? { agentId } : {}),
          ...(baseBranch ? { baseBranch } : {}),
          ...(existingWorktreePath ? { existingWorktree: { path: existingWorktreePath } } : {}),
          ...(firstPrompt ? { firstPrompt } : {}),
          projectId,
          ...(message.startFromOrigin === true ? { startFromOrigin: true } : {}),
        },
      );
      /*
      The session row arrives with the next presentation snapshot, so refresh
      before answering: the sidebar's pending state ends on a list that already
      contains the row it was waiting for.
      */
      await this.refreshDomainPresentationFromClient("patch").catch(() => undefined);
      const createdSessionId = normalizeNonEmptyString(result.sessionId);
      const createdProjectId = projectId;
      if (createdSessionId) {
        this.focusLocalWorkspaceSession(createdProjectId, createdSessionId);
      }
      this.postWorktreeSessionResult(requestId, {
        branch: normalizeNonEmptyString(result.branch),
        ok: true,
        sessionId: createdSessionId
          ? createGxserverPresentationProjectSessionId(createdProjectId, createdSessionId)
          : undefined,
        worktreePath: normalizeNonEmptyString(result.worktreePath),
      });
    } catch (error) {
      const description = gpuiWorktreeUserVisibleErrorMessage(error);
      this.postSidebarActionToast("warning", "Could not create worktree session", {
        description,
      });
      this.postWorktreeSessionResult(requestId, { error: description, ok: false });
    }
  }

  /*
  CDXC:SidebarV2LogicalProjects 2026-07-29:
  The remote half of the worktree create, kept as its own method because every
  step after the RPC differs: the presentation to refresh is that machine's, the
  focus helper is the remote one, and the sidebar session id is machine-scoped.
  Mirrors `runSessionLifecycleCommand`'s routing rule exactly — the machine is
  read out of the id the HOST minted, never guessed from anything the renderer
  supplied.
  */
  private async createRemoteWorktreeSession(
    remoteGroup: { machineId: string; projectId: string },
    message: Extract<SidebarToExtensionMessage, { type: "createWorktreeSession" }>,
    requestId: string,
  ): Promise<void> {
    const existingWorktreePath = normalizeGpuiProjectPath(message.existingWorktreePath);
    const agentId = message.agentId?.trim() ?? "";
    const baseBranch = message.baseBranch?.trim() ?? "";
    const firstPrompt = message.firstPrompt?.trim() ?? "";
    try {
      const result = await this.requestRemoteGxserver<GxserverCreateWorktreeSessionResult>(
        remoteGroup.machineId,
        "/api/createWorktreeSession",
        {
          ...(agentId ? { agentId } : {}),
          ...(baseBranch ? { baseBranch } : {}),
          ...(existingWorktreePath ? { existingWorktree: { path: existingWorktreePath } } : {}),
          ...(firstPrompt ? { firstPrompt } : {}),
          projectId: remoteGroup.projectId,
          ...(message.startFromOrigin === true ? { startFromOrigin: true } : {}),
        },
        /*
        Cutting a worktree runs a fetch, a `git worktree add`, and the project's
        own setup command on the far side of an SSH tunnel. The bridge's 20s
        default is a create-session budget, not a repository-clone budget.
        */
        { timeoutMs: 120_000 },
      );
      await this.refreshRemotePresentationFromGxserver(remoteGroup.machineId).catch(
        () => undefined,
      );
      const createdSessionId = normalizeNonEmptyString(result.sessionId);
      if (createdSessionId) {
        this.setRemotePresentationSessionFocus({
          machineId: remoteGroup.machineId,
          projectId: remoteGroup.projectId,
          sessionId: createdSessionId,
        });
      }
      this.postWorktreeSessionResult(requestId, {
        branch: normalizeNonEmptyString(result.branch),
        ok: true,
        sessionId: createdSessionId
          ? createGpuiRemotePresentationSessionId(
              remoteGroup.machineId,
              remoteGroup.projectId,
              createdSessionId,
            )
          : undefined,
        worktreePath: normalizeNonEmptyString(result.worktreePath),
      });
    } catch (error) {
      const description = gpuiWorktreeUserVisibleErrorMessage(error);
      this.postSidebarActionToast("warning", "Could not create worktree session", {
        description,
      });
      this.postWorktreeSessionResult(requestId, { error: description, ok: false });
    }
  }

  /*
  CDXC:SidebarV2Worktree 2026-07-29:
  Cleanup for the checkout whose last session just closed. gxserver answers a
  dirty worktree with `removed: false, dirty: true` — a REFUSAL, not a failure —
  and the sidebar re-asks with `force`. That decision stays server-side so the
  client never has to read git status to know whether it is safe to delete.

  CDXC:SidebarV2LogicalProjects 2026-07-29:
  Remote projects route to their own daemon rather than being refused. The
  worktree path travelling here came from that machine's own presentation
  (`session.cwd`), so the daemon is being handed back a path it published, and
  it still applies its own dirty check and its own path-safety normalization
  before deleting anything.
  */
  private async removeSessionWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "removeSessionWorktree" }>,
  ): Promise<void> {
    const requestId = message.requestId.trim();
    const worktreePath = normalizeGpuiProjectPath(message.worktreePath);
    if (!requestId || !worktreePath) {
      return;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(message.projectId);
    const projectId = remoteGroup
      ? remoteGroup.projectId
      : parseGxserverPresentationProjectGroupId(message.projectId);
    if (!projectId || (!remoteGroup && !this.client)) {
      this.postSessionWorktreeRemovalResult(requestId, worktreePath, {
        error: "gxserver is unavailable.",
        ok: false,
        removed: false,
      });
      return;
    }
    try {
      const params = {
        ...(message.force === true ? { force: true } : {}),
        projectId,
        worktreePath,
      };
      const result = remoteGroup
        ? await this.requestRemoteGxserver<GxserverRemoveSessionWorktreeResult>(
            remoteGroup.machineId,
            "/api/removeSessionWorktree",
            params,
            { timeoutMs: 60_000 },
          )
        : await this.client!.rpc<GxserverRemoveSessionWorktreeResult>(
            "/api/removeSessionWorktree",
            params,
          );
      const removed = result.removed === true;
      if (removed) {
        await (remoteGroup
          ? this.refreshRemotePresentationFromGxserver(remoteGroup.machineId)
          : this.refreshDomainPresentationFromClient("patch")
        ).catch(() => undefined);
      }
      this.postSessionWorktreeRemovalResult(requestId, worktreePath, {
        dirty: result.dirty === true,
        ok: true,
        removed,
        warnings: Array.isArray(result.warnings)
          ? result.warnings.filter(
              (warning): warning is string => typeof warning === "string" && warning.trim() !== "",
            )
          : undefined,
      });
    } catch (error) {
      const description = gpuiWorktreeUserVisibleErrorMessage(error);
      this.postSidebarActionToast("warning", "Could not remove worktree", { description });
      this.postSessionWorktreeRemovalResult(requestId, worktreePath, {
        error: description,
        ok: false,
        removed: false,
      });
    }
  }

  private postWorktreeSessionResult(
    requestId: string,
    result: {
      branch?: string;
      error?: string;
      ok: boolean;
      sessionId?: string;
      worktreePath?: string;
    },
  ): void {
    this.messageSource.postMessage({
      branch: result.branch,
      error: result.error,
      ok: result.ok,
      requestId,
      sessionId: result.sessionId,
      type: "worktreeSessionResult",
      worktreePath: result.worktreePath,
    });
  }

  private postSessionWorktreeRemovalResult(
    requestId: string,
    worktreePath: string,
    result: {
      dirty?: boolean;
      error?: string;
      ok: boolean;
      removed: boolean;
      warnings?: string[];
    },
  ): void {
    this.messageSource.postMessage({
      dirty: result.dirty,
      error: result.error,
      ok: result.ok,
      removed: result.removed,
      requestId,
      type: "sessionWorktreeRemovalResult",
      warnings: result.warnings,
      worktreePath,
    });
  }

  private async updateProjectWorktreeCommand(projectId: string, command: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedCommand = command.trim();
    await this.updateProjectDomainState(project.projectId, {
      gitConfig: {
        ...project.gitConfig,
        worktreeCommand: normalizedCommand || null,
      },
    });
  }

  private async updateProjectBeadsDisplayKey(projectId: string, displayKey: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedDisplayKey = displayKey
      .trim()
      .toUpperCase()
      .replace(/[^A-Z0-9]/gu, "")
      .slice(0, 3);
    await this.updateProjectDomainState(project.projectId, {
      gitConfig: {
        ...project.gitConfig,
        beadsDisplayKey: normalizedDisplayKey || null,
      },
      projectBoardConfig: {
        ...project.projectBoardConfig,
        beadsDisplayKey: normalizedDisplayKey || null,
      },
    });
  }

  private async updateProjectBeadsDirectory(projectId: string, directory: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedDirectory = directory.trim();
    await this.updateProjectDomainState(project.projectId, {
      projectBoardConfig: {
        ...project.projectBoardConfig,
        beadsDirectory: normalizedDirectory || null,
      },
    });
  }

  /*
  CDXC:DocsRootDirectory 2026-08-09:
  The Docs root override rides in the same per-project config object the Beads
  directory already uses, so Settings -> Projects keeps one storage seam and
  needs no new domain field, column, or migration. A blank value clears the
  override so the project falls back to the Global Default, then the repo root.
  */
  private async updateProjectDocsDirectory(projectId: string, directory: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedDirectory = directory.trim();
    await this.updateProjectDomainState(project.projectId, {
      projectBoardConfig: {
        ...project.projectBoardConfig,
        docsDirectory: normalizedDirectory || null,
      },
    });
  }

  private refreshGitStateForActiveProjectIfNeeded(): void {
    const project = this.activeDomainProject();
    if (!project || project.projectId === this.lastGitRefreshProjectId) {
      return;
    }
    this.lastGitRefreshProjectId = project.projectId;
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Project switching is on the critical path of terminal attach: every RPC this
    fires competes with the attach RPCs on the same daemon. A project the user
    switched away from seconds ago has not changed on disk, so publish its
    memoized state and issue nothing at all. Only a cold or stale project pays
    for a fan-out, and that fan-out leaves the GitHub CLI probe out of the
    burst.
    */
    const memoizedState = this.gitStateMemoByProjectId.get(project.projectId, Date.now());
    if (memoizedState) {
      this.gitState = this.applyLiveGitStateOverlays(project, memoizedState);
      this.publishHudPatch();
      return;
    }
    void this.refreshGitState({ deferGitHub: true, project, toastOnFailure: false });
  }

  /**
   * Re-apply the parts of a published Git state that the runtime mutates
   * outside a refresh, so a memoized state can never resurrect stale values:
   * Git preferences are patched straight onto `this.gitState` when the user
   * changes them, and GitHub CLI results carry their own longer-lived lease.
   */
  private applyLiveGitStateOverlays(
    project: GxserverProjectDomainState,
    state: SidebarGitState,
  ): SidebarGitState {
    const preferences = this.gitPreferencesForProject(project);
    const gitHubState = state.isRepo
      ? this.gitHubStateMemoByProjectId.peek(project.projectId)
      : undefined;
    return {
      ...state,
      ...(gitHubState ?? {}),
      confirmSuggestedCommit: preferences.confirmCommit,
      generateCommitBody: preferences.generateCommitBody,
      primaryAction: preferences.primaryAction,
    };
  }

  private async refreshGitState({
    deferGitHub = false,
    force = false,
    project = this.activeDomainProject(),
    publishBusy = false,
    toastOnFailure = false,
  }: {
    /**
     * Leave `gh --version` / `gh pr view` out of the fan-out and publish the
     * memoized GitHub state instead, scheduling a probe once the local Git
     * state is out. Only background and switch-driven refreshes set this;
     * every caller that reads `pr` / `hasGitHubCli` off the returned state
     * keeps the synchronous probe.
     */
    deferGitHub?: boolean;
    force?: boolean;
    project?: GxserverProjectDomainState;
    publishBusy?: boolean;
    toastOnFailure?: boolean;
  } = {}): Promise<SidebarGitState> {
    if (!project) {
      this.gitState = createDefaultSidebarGitState();
      this.publishHudPatch();
      return this.gitState;
    }
    if (force) {
      this.lastGitRefreshProjectId = project.projectId;
    }
    const nextState = await this.readSidebarGitState(project, {
      deferGitHub,
      publishBusy,
      toastOnFailure,
    });
    if (this.activeProjectId === project.projectId) {
      this.gitState = nextState;
      this.publishHudPatch();
    }
    return nextState;
  }

  private async refreshGitStateForMessage(
    message: Extract<SidebarToExtensionMessage, { type: "refreshGitState" }>,
  ): Promise<void> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-21:26:
    Reused Git controls can refresh from a scoped local or remote project row. Resolve that owner before reading Git state; unscoped callers keep the active-project behavior, but scoped remote rows must never refresh the active local project by accident.
    */
    const explicitScope = Boolean(message.groupId?.trim() || message.projectId?.trim());
    const remoteScope = this.resolveGitPreferenceRemoteScope(message);
    if (remoteScope) {
      const activeRemoteGroupId = createGpuiRemotePresentationGroupId(
        remoteScope.machineId,
        remoteScope.projectId,
      );
      if (this.activeGroupId === activeRemoteGroupId) {
        const preferences = this.gitPreferencesForPresentationProject(
          this.findRemotePresentationProject(remoteScope) ?? remoteScope.project,
        );
        this.gitState = {
          ...createDefaultSidebarGitState(
            preferences.primaryAction,
            preferences.confirmCommit,
            preferences.generateCommitBody,
          ),
          isBusy: true,
        };
        this.publishHudPatch();
      }
      const nextState = await this.readRemoteSidebarGitState(remoteScope);
      if (this.activeGroupId === activeRemoteGroupId) {
        this.gitState = nextState;
        this.publishHudPatch();
      }
      return;
    }
    if (explicitScope && this.isGitPreferenceRemoteScope(message)) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Reconnect the remote machine before refreshing Git state.",
      });
      return;
    }
    const project =
      this.resolveGitPreferenceLocalProject(message) ??
      (explicitScope ? undefined : this.activeDomainProject());
    if (!project) {
      this.postGitToast("warning", "Git unavailable", {
        description: "No active gxserver project is available.",
      });
      return;
    }
    await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
  }

  private async readSidebarGitState(
    project: GxserverProjectDomainState,
    options: { deferGitHub?: boolean; publishBusy?: boolean; toastOnFailure?: boolean } = {},
  ): Promise<SidebarGitState> {
    const baseState = createDefaultSidebarGitState(
      this.gitPreferencesForProject(project).primaryAction,
      this.gitPreferencesForProject(project).confirmCommit,
      this.gitPreferencesForProject(project).generateCommitBody,
    );
    if (
      !this.client ||
      project.isRecentProject === true ||
      isGpuiPresentationQuickDomainProject(project) ||
      !normalizeGpuiProjectPath(project.path)
    ) {
      return { ...baseState, hasCheckedGitHubRemote: true, isRepo: false };
    }
    if (options.publishBusy && this.activeProjectId === project.projectId) {
      this.gitState = { ...baseState, isBusy: true };
      this.publishHudPatch();
    }
    try {
      const repoCheck = await this.runGitAction(project, { action: "isInsideWorkTree" });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
        return this.memoizeSidebarGitState(project, {
          ...baseState,
          hasCheckedGitHubRemote: true,
          isRepo: false,
        });
      }

      /*
      CDXC:SidebarGitMemo 2026-07-29:
      `deferGitHub` keeps the two `gh` subprocesses (one of them a network call
      with a 120s server-side timeout) out of the burst a project switch fires.
      The switch publishes local Git state with the last known GitHub answer
      overlaid, and `scheduleDeferredGitHubProbe` fills in a fresh one shortly
      after, once the attach traffic has drained.
      */
      const [branch, status, diff, untrackedFiles, upstream, remotes, originRemote, gitHubState] =
        await Promise.all([
          this.runGitAction(project, { action: "branch" }),
          this.runGitAction(project, { action: "statusPorcelain" }),
          this.runGitAction(project, { action: "diffNumstat" }),
          this.runGitAction(project, { action: "listUntracked" }),
          this.runGitAction(project, { action: "upstreamCounts" }),
          this.runGitAction(project, { action: "listRemotes" }),
          this.runGitAction(project, { action: "getOriginRemoteUrl" }),
          options.deferGitHub === true
            ? this.memoizedGitHubState(project)
            : this.readGitHubState(project),
        ]);
      if (options.deferGitHub === true) {
        this.scheduleDeferredGitHubProbeIfStale(project);
      }
      const files = mergeGpuiGitChangedFiles([
        ...parseGpuiGitNumstatFiles(diff.stdout),
        ...parseGpuiGitStatusPorcelainFiles(status.stdout),
        ...parseGitZeroDelimitedPaths(untrackedFiles.stdout).flatMap((path) => {
          const normalizedPath = normalizeGpuiRelativeGitFilePath(path);
          return normalizedPath
            ? [
                {
                  additions: 0,
                  deletions: 0,
                  path: normalizedPath,
                },
              ]
            : [];
        }),
      ]);
      const totals = summarizeGpuiGitChangedFiles(files);
      const upstreamParts = upstream.exitCode === 0 ? upstream.stdout.trim().split(/\s+/) : [];
      return this.memoizeSidebarGitState(project, {
        ...baseState,
        additions: totals.additions,
        aheadCount: Number(upstreamParts[0] || 0) || 0,
        behindCount: Number(upstreamParts[1] || 0) || 0,
        branch: branch.stdout.trim() || null,
        deletions: totals.deletions,
        hasCheckedGitHubRemote: true,
        hasGitHubCli: gitHubState.hasGitHubCli,
        hasGitHubRemote:
          originRemote.exitCode === 0 &&
          normalizeGpuiGitHubRemoteUrl(originRemote.stdout) !== undefined,
        hasOriginRemote: remotes.stdout.split(/\s+/).includes("origin"),
        hasUpstream: upstream.exitCode === 0,
        hasWorkingTreeChanges: status.stdout.trim().length > 0,
        isBusy: false,
        isRepo: true,
        files,
        isWorktree: normalizeGpuiWorktreeParentProjectId(project.worktree) !== undefined,
        pr: gitHubState.pr,
        worktreeName: stringFromRecord(project.worktree, "name"),
      });
    } catch {
      if (options.toastOnFailure) {
        this.postGitToast("error", "Could not refresh Git state", {
          description: "gxserver could not inspect the selected project.",
        });
      }
      /*
      CDXC:SidebarGitMemo 2026-07-29:
      A failed probe is not a cacheable answer. Drop any memoized entry so the
      next switch re-probes instead of republishing a state gxserver could no
      longer confirm.
      */
      this.gitStateMemoByProjectId.delete(project.projectId);
      return { ...baseState, isBusy: false };
    }
  }

  /**
   * Remember a freshly computed Git state for this project so a switch back to
   * it inside the memo TTL publishes without issuing any RPC.
   */
  private memoizeSidebarGitState(
    project: GxserverProjectDomainState,
    state: SidebarGitState,
  ): SidebarGitState {
    this.gitStateMemoByProjectId.set(project.projectId, state, Date.now());
    return state;
  }

  /** Run the GitHub CLI probes and memoize the pair under the longer lease. */
  private async readGitHubState(
    project: GxserverProjectDomainState,
  ): Promise<GpuiSidebarGitHubState> {
    const [ghVersion, pr] = await Promise.all([
      this.runGitHubAction(project, { action: "version" }),
      this.runGitHubAction(project, { action: "prView" }),
    ]);
    const gitHubState: GpuiSidebarGitHubState = {
      hasGitHubCli: ghVersion.exitCode === 0,
      pr: parseGpuiGitHubPullRequest(pr.stdout, pr.exitCode === 0),
    };
    this.gitHubStateMemoByProjectId.set(project.projectId, gitHubState, Date.now());
    return gitHubState;
  }

  /**
   * GitHub state for a refresh that must not spawn `gh`: the memoized answer,
   * stale or not. A stale-but-known answer beats a blank one, because pull
   * request state moves on a human timescale and the previous badge is the
   * accurate one far more often than an empty badge would be. A project that
   * has never been probed publishes no GitHub affordances until the deferred
   * probe lands.
   */
  private memoizedGitHubState(project: GxserverProjectDomainState): GpuiSidebarGitHubState {
    return (
      this.gitHubStateMemoByProjectId.peek(project.projectId) ?? { hasGitHubCli: false, pr: null }
    );
  }

  /**
   * Queue the GitHub probe this refresh skipped, once its lease has run out.
   * Called after the local fan-out resolves so the probe delay is measured from
   * the moment the switch-time RPC burst is actually over.
   */
  private scheduleDeferredGitHubProbeIfStale(project: GxserverProjectDomainState): void {
    if (
      this.gitHubStateMemoByProjectId.isFreshKey(project.projectId, Date.now()) ||
      this.pendingGitHubProbeProjectIds.has(project.projectId)
    ) {
      return;
    }
    this.pendingGitHubProbeProjectIds.add(project.projectId);
    const timeoutId = window.setTimeout(() => {
      this.gitHubProbeTimeoutIds.delete(timeoutId);
      void this.runDeferredGitHubProbe(project);
    }, GPUI_SIDEBAR_GIT_HUB_DEFERRED_PROBE_DELAY_MS);
    this.gitHubProbeTimeoutIds.add(timeoutId);
  }

  private async runDeferredGitHubProbe(project: GxserverProjectDomainState): Promise<void> {
    try {
      if (!this.client) {
        return;
      }
      /*
      `readGitHubState` refreshes the GitHub lease, which is all a memoized Git
      state needs: `applyLiveGitStateOverlays` reads that lease every time a
      memoized state is published, so the local-Git entry keeps its own,
      shorter, untouched lease.
      */
      const gitHubState = await this.readGitHubState(project);
      if (this.activeProjectId === project.projectId && this.gitState.isRepo) {
        this.gitState = { ...this.gitState, ...gitHubState };
        this.publishHudPatch();
      }
    } catch {
      /*
      A failed `gh` probe leaves the previous lease in place; the next
      background or switch-driven refresh reschedules it.
      */
    } finally {
      this.pendingGitHubProbeProjectIds.delete(project.projectId);
    }
  }

  private async readRemoteSidebarGitState(
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<SidebarGitState> {
    const remotePreferences = this.gitPreferencesForPresentationProject(
      this.findRemotePresentationProject(remoteScope) ?? remoteScope.project,
    );
    const baseState = createDefaultSidebarGitState(
      remotePreferences.primaryAction,
      remotePreferences.confirmCommit,
      remotePreferences.generateCommitBody,
    );
    try {
      const repoCheck = await this.runRemoteGitAction(remoteScope, { action: "isInsideWorkTree" });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
        return { ...baseState, hasCheckedGitHubRemote: true, isRepo: false };
      }

      const [branch, status, diff, untrackedFiles, upstream, remotes, originRemote, ghVersion, pr] =
        await Promise.all([
          this.runRemoteGitAction(remoteScope, { action: "branch" }),
          this.runRemoteGitAction(remoteScope, { action: "statusPorcelain" }),
          this.runRemoteGitAction(remoteScope, { action: "diffNumstat" }),
          this.runRemoteGitAction(remoteScope, { action: "listUntracked" }),
          this.runRemoteGitAction(remoteScope, { action: "upstreamCounts" }),
          this.runRemoteGitAction(remoteScope, { action: "listRemotes" }),
          this.runRemoteGitAction(remoteScope, { action: "getOriginRemoteUrl" }),
          this.runRemoteGitHubAction(remoteScope, { action: "version" }),
          this.runRemoteGitHubAction(remoteScope, { action: "prView" }),
        ]);
      const files = mergeGpuiGitChangedFiles([
        ...parseGpuiGitNumstatFiles(diff.stdout),
        ...parseGpuiGitStatusPorcelainFiles(status.stdout),
        ...parseGitZeroDelimitedPaths(untrackedFiles.stdout).flatMap((path) => {
          const normalizedPath = normalizeGpuiRelativeGitFilePath(path);
          return normalizedPath
            ? [
                {
                  additions: 0,
                  deletions: 0,
                  path: normalizedPath,
                },
              ]
            : [];
        }),
      ]);
      const totals = summarizeGpuiGitChangedFiles(files);
      const upstreamParts = upstream.exitCode === 0 ? upstream.stdout.trim().split(/\s+/) : [];
      const presentationProject =
        this.findRemotePresentationProject(remoteScope) ?? remoteScope.project;
      return {
        ...baseState,
        additions: totals.additions,
        aheadCount: Number(upstreamParts[0] || 0) || 0,
        behindCount: Number(upstreamParts[1] || 0) || 0,
        branch: branch.stdout.trim() || null,
        deletions: totals.deletions,
        files,
        hasCheckedGitHubRemote: true,
        hasGitHubCli: ghVersion.exitCode === 0,
        hasGitHubRemote:
          originRemote.exitCode === 0 &&
          normalizeGpuiGitHubRemoteUrl(originRemote.stdout) !== undefined,
        hasOriginRemote: remotes.stdout.split(/\s+/).includes("origin"),
        hasUpstream: upstream.exitCode === 0,
        hasWorkingTreeChanges: status.stdout.trim().length > 0,
        isBusy: false,
        isRepo: true,
        isWorktree:
          normalizeGpuiWorktreeParentProjectId(presentationProject.worktree) !== undefined,
        pr: parseGpuiGitHubPullRequest(pr.stdout, pr.exitCode === 0),
        worktreeName:
          stringFromRecord(presentationProject.worktree, "name") ?? presentationProject.title,
      };
    } catch {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "The remote gxserver could not inspect the selected project.",
      });
      return { ...baseState, hasCheckedGitHubRemote: true, isBusy: false, isRepo: false };
    }
  }

  private async runRemoteSidebarGitAction(
    message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>,
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<void> {
    if (message.action === "multiRelease") {
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Multicommit & Release",
        GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
      );
      return;
    }
    if (message.action === "release") {
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Release",
        GPUI_GIT_RELEASE_ONLY_PROMPT,
      );
      return;
    }

    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Open a Git repository on the remote machine to use Git actions.",
      });
      return;
    }

    if (message.action === "syncMain") {
      if (!normalizeGpuiWorktreeParentProjectId(remoteScope.project.worktree)) {
        this.postRemoteToast("warning", "Remote worktree unavailable", {
          description: "Open a remote worktree project to sync with main.",
        });
        return;
      }
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Sync with Main",
        buildGpuiGitSyncWithMainPrompt(),
      );
      return;
    }

    if (message.action === "syncRemote") {
      if (!hasSidebarGitRemoteCommitDelta(gitState)) {
        this.postRemoteToast("info", "Remote already synced");
        return;
      }
      await this.runRemoteGitMutation(
        remoteScope,
        "Syncing remote",
        "Remote sync complete",
        async () => {
          await this.syncRemoteCurrentBranchWithRemote(remoteScope, gitState);
        },
      );
      return;
    }

    if (
      normalizeGpuiWorktreeMetadata(remoteScope.project.worktree) &&
      (message.action === "commit" || message.action === "push" || message.action === "pr")
    ) {
      this.promptRemoteSidebarGitActionReview(remoteScope, gitState, message.action);
      return;
    }

    if (message.action === "pr") {
      if (gitState.pr?.state === "open") {
        this.postRemoteProjectNativeAction(
          "openRemoteExistingPullRequestInBrowser",
          remoteScope,
          message,
        );
        return;
      }
      if (!gitState.hasGitHubCli) {
        this.postRemoteToast("warning", "Remote GitHub CLI unavailable", {
          description: "Install GitHub CLI on the remote machine before creating a pull request.",
        });
        return;
      }
      if (gitState.hasWorkingTreeChanges) {
        this.promptRemoteSidebarGitActionReview(remoteScope, gitState, "pr");
        return;
      }
      await this.runRemoteSidebarGitPullRequestAgentWorkflow({
        gitState,
        hasCommit: false,
        hasExplicitFileSelection: false,
        message: "",
        remoteScope,
      });
      return;
    }

    if (message.action === "commit") {
      if (!gitState.hasWorkingTreeChanges) {
        this.postRemoteToast("info", "No remote changes to commit");
        return;
      }
      this.promptRemoteSidebarGitActionReview(remoteScope, gitState, "commit");
      return;
    }

    if (message.action === "push") {
      if (gitState.hasWorkingTreeChanges) {
        this.promptRemoteSidebarGitActionReview(remoteScope, gitState, "push");
        return;
      }
      await this.runRemoteGitMutation(remoteScope, "Pushing", "Remote push complete", async () => {
        await this.pushRemoteCurrentBranch(remoteScope, gitState);
      });
    }
  }

  private promptRemoteSidebarGitActionReview(
    remoteScope: GpuiRemoteProjectScope,
    gitState: SidebarGitState,
    action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
  ): void {
    const requestId = `gpui-remote-git-action-${Date.now().toString(36)}`;
    const hasCommit = gitState.hasWorkingTreeChanges;
    this.pendingGitCommitRequests.set(requestId, {
      action,
      files: [...gitState.files],
      hasCommit,
      projectId: createGpuiRemotePresentationProjectId(
        remoteScope.machineId,
        remoteScope.projectId,
      ),
      remoteReference: {
        machineId: remoteScope.machineId,
        projectId: remoteScope.projectId,
      },
      remoteTitle: remoteScope.project.title || remoteScope.machineName || "Remote project",
      subject: "",
    });
    const modalDraft: SidebarPromptGitCommitMessage = {
      action,
      agentId: this.resolveDefaultPromptAgentId(),
      branch: gitState.branch,
      changedFiles: gitState.files,
      confirmLabel: resolveGpuiSidebarGitConfirmLabel(action, hasCommit),
      deleteWorktreeAfterDefault: false,
      description: hasCommit
        ? "Review and confirm your remote commit. Leave the message blank to auto-generate one."
        : resolveGpuiSidebarGitPromptDescription(action),
      isDefaultRef: gitState.branch === "main" || gitState.branch === "master",
      isWorktree: normalizeGpuiWorktreeMetadata(remoteScope.project.worktree) !== undefined,
      requestId,
      showCommitMessage: hasCommit,
      suggestedBody: undefined,
      suggestedSubject: "",
      type: "promptGitCommit",
      worktreeName:
        stringFromRecord(remoteScope.project.worktree, "name") ?? remoteScope.project.title,
    };
    this.openSidebarGitCommitReviewModal(modalDraft);
  }

  private async runSidebarGitAction(
    message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>,
  ): Promise<void> {
    const remoteReference = message.groupId
      ? parseGpuiRemotePresentationGroupId(message.groupId)
      : message.projectId
        ? parseGpuiRemotePresentationProjectId(message.projectId)
        : undefined;
    if (remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope({
        groupId: message.groupId,
        projectId: message.projectId,
      });
      if (!remoteScope) {
        this.postRemoteToast("warning", "Remote Git unavailable", {
          description: "Reconnect the remote machine before using Git actions.",
        });
        return;
      }
      await this.runRemoteSidebarGitAction(message, remoteScope);
      return;
    }
    const project = this.resolveGitProjectForMessage(message);
    if (!project) {
      this.postGitToast("warning", "Git unavailable", {
        description: "No active gxserver project is available.",
      });
      return;
    }

    if (message.action === "multiRelease") {
      await this.runSidebarGitPromptAction(
        project,
        "Multicommit & Release",
        GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
      );
      return;
    }
    if (message.action === "release") {
      await this.runSidebarGitPromptAction(project, "Release", GPUI_GIT_RELEASE_ONLY_PROMPT);
      return;
    }

    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository to use Git actions.",
      });
      return;
    }

    if (message.action === "syncMain") {
      if (!normalizeGpuiWorktreeParentProjectId(project.worktree)) {
        this.postGitToast("warning", "Worktree unavailable", {
          description: "Open a worktree project to sync with main.",
        });
        return;
      }
      await this.runSidebarGitPromptAction(
        project,
        "Sync with Main",
        buildGpuiGitSyncWithMainPrompt(),
      );
      return;
    }

    if (message.action === "syncRemote") {
      if (!hasSidebarGitRemoteCommitDelta(gitState)) {
        this.postGitToast("info", "Remote already synced");
        return;
      }
      await this.runGitMutation(project, "Syncing remote", "Remote sync complete", async () => {
        await this.syncCurrentBranchWithRemote(project, gitState);
      });
      return;
    }

    if (
      normalizeGpuiWorktreeMetadata(project.worktree) &&
      (message.action === "commit" || message.action === "push" || message.action === "pr")
    ) {
      this.promptSidebarGitActionReview(project, gitState, message.action);
      return;
    }

    if (message.action === "pr") {
      if (gitState.pr?.state === "open") {
        this.postNativeProjectPathAction(
          "openExistingPullRequestInBrowser",
          project.projectId,
          message,
        );
        return;
      }
      if (!gitState.hasGitHubCli) {
        this.postGitToast("warning", "GitHub CLI unavailable", {
          description: "Install GitHub CLI before creating a pull request.",
        });
        return;
      }
      if (gitState.hasWorkingTreeChanges) {
        this.promptSidebarGitActionReview(project, gitState, "pr");
        return;
      }
      await this.runSidebarGitPullRequestAgentWorkflow({
        gitState,
        hasCommit: false,
        hasExplicitFileSelection: false,
        message: "",
        project,
      });
      return;
    }

    if (message.action === "commit") {
      if (!gitState.hasWorkingTreeChanges) {
        this.postGitToast("info", "No changes to commit");
        return;
      }
      this.promptSidebarGitActionReview(project, gitState, "commit");
      return;
    }

    if (message.action === "push") {
      if (gitState.hasWorkingTreeChanges) {
        this.promptSidebarGitActionReview(project, gitState, "push");
        return;
      }
      await this.runGitMutation(project, "Pushing", "Push complete", async () => {
        await this.pushCurrentBranch(project, gitState);
      });
    }
  }

  private async confirmSidebarGitCommit(
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>,
  ): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(message.requestId);
    this.pendingGitCommitRequests.delete(message.requestId);
    if (!pending) {
      this.publishHudPatch();
      return;
    }
    if (pending.remoteReference) {
      await this.confirmRemoteSidebarGitCommit(pending, message);
      return;
    }
    const project = this.domainProjectById(pending.projectId);
    if (!project) {
      this.postGitToast("error", "Git action unavailable", {
        description: "The selected gxserver project is no longer available.",
      });
      this.publishHudPatch();
      return;
    }
    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository to use Git actions.",
      });
      return;
    }
    if (pending.action === "pr") {
      let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
      if (pending.hasCommit) {
        try {
          trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
            pending,
            message.filePaths,
          );
        } catch {
          this.postGitToast("warning", "Invalid file selection", {
            description: "Choose files from the current Git review before creating a pull request.",
          });
          this.gitState = { ...this.gitStateForHud(), isBusy: false };
          this.publishHudPatch();
          return;
        }
      }
      if (message.deleteWorktreeAfter !== true) {
        await this.runSidebarGitPullRequestAgentWorkflow({
          agentId: message.agentId,
          filePaths: trustedFileSelection?.filePaths,
          gitState,
          hasCommit: pending.hasCommit,
          hasExplicitFileSelection: trustedFileSelection?.explicit ?? false,
          message: message.message,
          project,
        });
        return;
      }
      let confirmedPullRequest = false;
      const completed = await this.runGitMutation(
        project,
        resolveGpuiSidebarGitStartedTitle("pr", pending.hasCommit),
        resolveGpuiSidebarGitFinishedTitle("pr"),
        async () => {
          if (pending.hasCommit) {
            await this.commitWithMessage(
              project,
              message.message,
              trustedFileSelection?.filePaths,
              {
                agentId: message.agentId,
                commitOnNewRef: message.commitOnNewRef === true,
              },
            );
          }
          const nextGitState = await this.refreshGitState({ force: true, project });
          await this.pushCurrentBranch(project, nextGitState);
          const result = await this.createPullRequest(project);
          if (!isGpuiConfirmedOpenPullRequest(result)) {
            throw new GpuiUserVisibleGitError(
              "GitHub CLI could not create or find an open pull request.",
            );
          }
          confirmedPullRequest = true;
          this.postNativeProjectPathAction(
            "openExistingPullRequestInBrowser",
            project.projectId,
            message,
          );
        },
      );
      if (completed && confirmedPullRequest) {
        await this.deleteWorktreeAfterCompletedGitAction(project);
      }
      if (completed && !confirmedPullRequest) {
        this.postGitToast("warning", "Worktree cleanup skipped", {
          description: "Pull request creation was not confirmed.",
        });
      }
      if (!completed) {
        this.postGitToast("warning", "Worktree cleanup skipped", {
          description: "Pull request creation did not complete.",
        });
      }
      return;
    }
    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postGitToast("warning", "Invalid file selection", {
          description: "Choose files from the current Git review before committing.",
        });
        this.gitState = { ...this.gitStateForHud(), isBusy: false };
        this.publishHudPatch();
        return;
      }
    }

    const completed = await this.runGitMutation(
      project,
      resolveGpuiSidebarGitStartedTitle(pending.action, pending.hasCommit),
      resolveGpuiSidebarGitFinishedTitle(pending.action),
      async () => {
        if (pending.hasCommit) {
          await this.commitWithMessage(project, message.message, trustedFileSelection?.filePaths, {
            agentId: message.agentId,
            commitOnNewRef: message.commitOnNewRef === true,
          });
        }
        if (pending.action === "push") {
          const nextState = await this.refreshGitState({ force: true, project });
          await this.pushCurrentBranch(project, nextState);
        }
      },
    );
    if (completed && message.deleteWorktreeAfter === true) {
      await this.deleteWorktreeAfterCompletedGitAction(project);
    }
  }

  private async confirmRemoteSidebarGitCommit(
    pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference },
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>,
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
    if (!remoteScope) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Reconnect the remote machine before confirming this Git action.",
      });
      return;
    }
    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Open a Git repository on the remote machine to use Git actions.",
      });
      return;
    }
    if (pending.action === "pr") {
      let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
      if (pending.hasCommit) {
        try {
          trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
            pending,
            message.filePaths,
          );
        } catch {
          this.postRemoteToast("warning", "Invalid file selection", {
            description:
              "Choose files from the current remote Git review before creating a pull request.",
          });
          return;
        }
      }
      if (message.deleteWorktreeAfter !== true) {
        await this.runRemoteSidebarGitPullRequestAgentWorkflow({
          agentId: message.agentId,
          filePaths: trustedFileSelection?.filePaths,
          gitState,
          hasCommit: pending.hasCommit,
          hasExplicitFileSelection: trustedFileSelection?.explicit ?? false,
          message: message.message,
          remoteScope,
        });
        return;
      }
      let confirmedPullRequest = false;
      const completed = await this.runRemoteGitMutation(
        remoteScope,
        resolveGpuiSidebarGitStartedTitle("pr", pending.hasCommit),
        resolveGpuiSidebarGitFinishedTitle("pr"),
        async () => {
          if (pending.hasCommit) {
            await this.commitRemoteWithMessage(
              remoteScope,
              message.message,
              trustedFileSelection?.filePaths,
              {
                agentId: message.agentId,
                commitOnNewRef: message.commitOnNewRef === true,
              },
            );
          }
          const nextGitState = await this.readRemoteSidebarGitState(remoteScope);
          await this.pushRemoteCurrentBranch(remoteScope, nextGitState);
          const result = await this.createRemotePullRequest(remoteScope);
          if (!isGpuiConfirmedOpenRemotePullRequest(result)) {
            throw new GpuiUserVisibleGitError(
              "GitHub CLI could not create or find an open remote pull request.",
            );
          }
          confirmedPullRequest = true;
          this.postRemoteProjectNativeAction(
            "openRemoteExistingPullRequestInBrowser",
            remoteScope,
            message,
          );
        },
      );
      if (completed && confirmedPullRequest) {
        await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
      }
      if (completed && !confirmedPullRequest) {
        this.postRemoteToast("warning", "Remote worktree cleanup skipped", {
          description: "Pull request creation was not confirmed.",
        });
      }
      if (!completed) {
        this.postRemoteToast("warning", "Remote worktree cleanup skipped", {
          description: "Pull request creation did not complete.",
        });
      }
      return;
    }

    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postRemoteToast("warning", "Invalid file selection", {
          description: "Choose files from the current remote Git review before committing.",
        });
        return;
      }
    }

    const completed = await this.runRemoteGitMutation(
      remoteScope,
      resolveGpuiSidebarGitStartedTitle(pending.action, pending.hasCommit),
      resolveGpuiSidebarGitFinishedTitle(pending.action),
      async () => {
        if (pending.hasCommit) {
          await this.commitRemoteWithMessage(
            remoteScope,
            message.message,
            trustedFileSelection?.filePaths,
            {
              agentId: message.agentId,
              commitOnNewRef: message.commitOnNewRef === true,
            },
          );
        }
        if (pending.action === "push") {
          const nextState = await this.readRemoteSidebarGitState(remoteScope);
          await this.pushRemoteCurrentBranch(remoteScope, nextState);
        }
      },
    );
    if (completed && message.deleteWorktreeAfter === true) {
      await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
    }
  }

  private async confirmSidebarGitDirectMerge(
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>,
  ): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(message.requestId);
    this.pendingGitCommitRequests.delete(message.requestId);
    if (!pending) {
      this.publishHudPatch();
      return;
    }
    if (pending.remoteReference) {
      await this.confirmRemoteSidebarGitDirectMerge(pending, message);
      return;
    }
    const project = this.domainProjectById(pending.projectId);
    if (!project) {
      this.postGitToast("error", "Direct merge unavailable", {
        description: "The selected gxserver project is no longer available.",
      });
      this.publishHudPatch();
      return;
    }
    const worktree = normalizeGpuiWorktreeMetadata(project.worktree);
    if (!worktree) {
      this.postGitToast("warning", "Worktree unavailable", {
        description: "Direct merge is only available from a gxserver worktree project.",
      });
      this.publishHudPatch();
      return;
    }
    const conflictAgent = this.resolveDefaultPromptAgent(message.agentId);
    if (!conflictAgent?.command?.trim()) {
      this.postGitToast("error", "Agent unavailable", {
        description: "Choose a configured prompt agent before merging.",
      });
      this.publishHudPatch();
      return;
    }

    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository before merging this worktree.",
      });
      return;
    }

    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postGitToast("warning", "Invalid file selection", {
          description: "Choose files from the current Git review before merging.",
        });
        this.gitState = { ...this.gitStateForHud(), isBusy: false };
        this.publishHudPatch();
        return;
      }
    }

    const toastId = createGpuiGitToastId();
    this.postGitToast("info", "Merging worktree into main", {
      persistent: true,
      toastId,
    });
    this.gitState = { ...this.gitStateForHud(), isBusy: true };
    this.publishHudPatch();
    try {
      if (pending.hasCommit) {
        await this.commitWithMessage(project, message.message, trustedFileSelection?.filePaths, {
          agentId: message.agentId,
        });
      }
      const nextGitState = await this.readSidebarGitState(project);
      const result = await this.mergeWorktreeIntoMain({
        branch: nextGitState.branch ?? worktree.branch,
        conflictAgent,
        deleteWorktreeAfter: message.deleteWorktreeAfter === true,
        worktreeProject: project,
      });
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      if (result === "conflicts") {
        this.postGitToast("warning", "Merge conflicts need resolution", { toastId });
        return;
      }
      await this.refreshDomainPresentationFromClient("patch").catch(() => undefined);
      this.postGitToast("success", "Worktree merged to main", { toastId });
    } catch (error) {
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      this.postGitToast("error", "Direct merge failed", {
        description: gpuiUserVisibleGitErrorMessage(
          error,
          "gxserver could not merge the selected worktree.",
        ),
        toastId,
      });
    }
  }

  private async confirmRemoteSidebarGitDirectMerge(
    pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference },
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>,
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
    if (!remoteScope) {
      this.postRemoteToast("warning", "Remote merge unavailable", {
        description: "Reconnect the remote machine before merging this worktree.",
      });
      return;
    }
    if (!normalizeGpuiWorktreeMetadata(remoteScope.project.worktree)) {
      this.postRemoteToast("warning", "Remote worktree unavailable", {
        description: "Direct merge is only available from a remote worktree project.",
      });
      return;
    }
    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postRemoteToast("warning", "Invalid file selection", {
          description: "Choose files from the current remote Git review before merging.",
        });
        return;
      }
    }
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", "Merging remote worktree", {
      persistent: true,
      toastId,
    });
    /*
    CDXC:RemoteGitBranching 2026-06-24-18:55:
    Remote direct merge and commit-on-new-branch must go through id-scoped gxserver operations so the daemon derives main, parent, and branch targets. GPUI may refresh presentation and create a conflict-resolution agent session, but it must not attach terminals, focus remote panes, open native apps, or expose branch/path/command details in status text.
    */
    try {
      if (pending.hasCommit) {
        await this.commitRemoteWithMessage(
          remoteScope,
          message.message,
          trustedFileSelection?.filePaths,
          {
            agentId: message.agentId,
          },
        );
      }
      const result = await this.mergeRemoteWorktreeIntoMain(remoteScope);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
        () => undefined,
      );
      if (result.status === "conflicts") {
        this.postGitToast("warning", "Remote merge conflicts need resolution", { toastId });
        const conflictAgentId = this.resolveDefaultPromptAgentId(message.agentId);
        if (conflictAgentId && result.parentProjectId) {
          await this.createRemoteAgentSessionForProject(
            { machineId: remoteScope.machineId, projectId: result.parentProjectId },
            conflictAgentId,
            GPUI_REMOTE_MERGE_CONFLICT_PROMPT,
            formatGpuiGitAgentWorkflowTitle("Merge Conflicts"),
          ).catch(() => undefined);
        }
        return;
      }
      this.postGitToast("success", "Remote worktree merged", { toastId });
      if (message.deleteWorktreeAfter === true) {
        await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
      }
    } catch (error) {
      this.postGitToast("error", "Remote direct merge failed", {
        description: gpuiUserVisibleGitErrorMessage(
          error,
          "Remote gxserver could not merge the selected worktree.",
        ),
        toastId,
      });
    }
  }

  private async mergeRemoteWorktreeIntoMain(
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<GxserverMergeWorktreeIntoMainResult> {
    return this.requestRemoteGxserver<GxserverMergeWorktreeIntoMainResult>(
      remoteScope.machineId,
      "/api/mergeWorktreeIntoMain",
      { projectId: remoteScope.projectId },
      { timeoutMs: 60_000 },
    );
  }

  private async mergeWorktreeIntoMain(input: {
    branch?: string | null;
    conflictAgent: SidebarAgentButton;
    deleteWorktreeAfter: boolean;
    worktreeProject: GxserverProjectDomainState;
  }): Promise<"conflicts" | "merged"> {
    const worktree = normalizeGpuiWorktreeMetadata(input.worktreeProject.worktree);
    if (!worktree) {
      throw new Error("Direct merge requires a worktree project.");
    }
    const branch = input.branch?.trim() || worktree.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before merging.");
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    if (
      !parentProject ||
      parentProject.projectId === input.worktreeProject.projectId ||
      parentProject.isRecentProject === true ||
      !normalizeGpuiProjectPath(parentProject.path)
    ) {
      throw new Error("The gxserver worktree parent project is unavailable.");
    }

    const mainCheck = await this.runGitAction(parentProject, {
      action: "verifyRef",
      ref: "main",
    });
    if (mainCheck.exitCode !== 0) {
      throw new Error('The parent project does not have a local "main" branch.');
    }
    const parentStatus = await this.runGitAction(parentProject, { action: "status" });
    if (parentStatus.exitCode !== 0) {
      throw new Error("Could not read parent project status.");
    }
    if (hasGpuiGxserverShortStatusChanges(parentStatus.stdout)) {
      throw new Error("Commit or stash changes in the main project before merging this worktree.");
    }

    const checkoutResult = await this.runGitAction(parentProject, {
      action: "checkout",
      branch: "main",
    });
    if (checkoutResult.exitCode !== 0) {
      throw new Error("Could not checkout main.");
    }
    const mergeResult = await this.runGitAction(parentProject, {
      action: "merge",
      branch,
    });
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Direct merge is the one Git flow whose writes land in a project other than
    the one the user is looking at: everything here mutates the *parent* repo
    while the flow only ever re-reads the worktree project. `runGitAction`
    drops the parent lease before each write, but that is not enough on its
    own here, because a parent read that was already in flight when the merge
    landed stores its pre-merge answer afterwards and would then be republished
    for the rest of the TTL. Invalidate once the merge has actually returned,
    for both outcomes: a conflicted merge still leaves the parent checked out
    on `main` with a merge in progress, and that path focuses the parent
    immediately.

    The GitHub lease goes with it. It is keyed by project but its content is
    per-branch, this flow checks the parent out onto `main`, and a merge can
    close the pull request the memo is describing. It also has no TTL check on
    the publish path (`applyLiveGitStateOverlays` peeks it), so a wrong entry
    here would otherwise survive until the next explicit probe.
    */
    this.gitStateMemoByProjectId.delete(parentProject.projectId);
    this.gitHubStateMemoByProjectId.delete(parentProject.projectId);
    if (mergeResult.exitCode !== 0) {
      await this.launchMergeConflictAgent({
        agent: input.conflictAgent,
        branch,
        mergeOutput: mergeResult.stderr.trim() || mergeResult.stdout.trim(),
        parentProject,
        worktree,
        worktreeProject: input.worktreeProject,
      });
      return "conflicts";
    }

    if (input.deleteWorktreeAfter) {
      await this.deleteWorktreeAfterCompletedGitAction(input.worktreeProject);
    }
    return "merged";
  }

  private async launchMergeConflictAgent(input: {
    agent: SidebarAgentButton;
    branch: string;
    mergeOutput: string;
    parentProject: GxserverProjectDomainState;
    worktree: GpuiWorktreeMetadata;
    worktreeProject: GxserverProjectDomainState;
  }): Promise<void> {
    this.focusProjectId(input.parentProject.projectId);
    await this.createAgentSessionForProject(
      input.parentProject,
      input.agent,
      buildGpuiMergeConflictPrompt(input),
      formatGpuiGitAgentWorkflowTitle("Merge Conflicts"),
    );
  }

  private async deleteWorktreeAfterCompletedGitAction(
    worktreeProject: GxserverProjectDomainState,
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const currentProject = this.domainProjectById(worktreeProject.projectId) ?? worktreeProject;
    const worktree = normalizeGpuiWorktreeMetadata(currentProject.worktree);
    if (!worktree) {
      this.postGitToast("warning", "Worktree cleanup skipped", {
        description: "The selected gxserver project is no longer a worktree.",
      });
      return;
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", "Removing worktree", {
      persistent: true,
      toastId,
    });
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Same reasoning as `confirmDeleteWorktree`: gxserver rewrites the parent
    repo's worktree list here and the flow focuses the parent afterwards. The
    parent lease was already dropped by the merge that got us here, but this
    keeps the invalidation attached to the write instead of to the caller, and
    it retires the removed project's own entries.
    */
    if (parentProject) {
      this.gitStateMemoByProjectId.delete(parentProject.projectId);
    }
    this.gitStateMemoByProjectId.delete(currentProject.projectId);
    this.gitHubStateMemoByProjectId.delete(currentProject.projectId);
    try {
      const result = await this.client.rpc<GxserverDeleteWorktreeProjectResult>(
        "/api/deleteWorktreeProject",
        {
          deleteLocalBranch: false,
          deleteRemoteBranch: false,
          projectId: currentProject.projectId,
        },
      );
      this.postGxserverWorktreeDeleteWarnings(result);
      this.domainProjects = this.domainProjects.filter(
        (project) => project.projectId !== currentProject.projectId,
      );
      if (parentProject) {
        this.focusProjectId(parentProject.projectId);
      } else if (this.activeProjectId === currentProject.projectId) {
        const fallbackProjectId = this.domainProjects[0]?.projectId;
        this.activeProjectId = fallbackProjectId;
        this.activeGroupId = fallbackProjectId
          ? createGxserverPresentationProjectGroupId(fallbackProjectId)
          : GPUI_GXSERVER_CHATS_GROUP_ID;
      }
      await this.refreshDomainPresentationFromClient("patch").catch(() => {
        this.publishHudPatch();
      });
      this.postGitToast("success", "Worktree removed", { toastId });
    } catch {
      this.postGitToast("error", "Could not remove worktree", {
        description: "gxserver worktree cleanup failed.",
        toastId,
      });
    }
  }

  private async deleteRemoteWorktreeAfterCompletedGitAction(
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<void> {
    const currentProject = this.findRemotePresentationProject(remoteScope) ?? remoteScope.project;
    const worktree = normalizeGpuiWorktreeMetadata(currentProject.worktree);
    if (!worktree) {
      this.postRemoteToast("warning", "Remote worktree cleanup skipped", {
        description: "The selected remote project is no longer a worktree.",
      });
      return;
    }
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", "Removing remote worktree", {
      persistent: true,
      toastId,
    });
    try {
      const result = await this.requestRemoteGxserver<GxserverDeleteWorktreeProjectResult>(
        remoteScope.machineId,
        "/api/deleteWorktreeProject",
        {
          deleteLocalBranch: false,
          deleteRemoteBranch: false,
          projectId: remoteScope.projectId,
        },
        { timeoutMs: 45_000 },
      );
      this.postGxserverWorktreeDeleteWarnings(result);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
        () => undefined,
      );
      this.postGitToast("success", "Remote worktree removed", { toastId });
    } catch {
      this.postGitToast("error", "Could not remove remote worktree", {
        description: "Remote gxserver worktree cleanup failed.",
        toastId,
      });
    }
  }

  private async promptDeleteWorktreeForGroup(groupId: string): Promise<void> {
    if (await this.promptDeleteRemoteWorktreeForGroup(groupId)) {
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    const project = projectId ? this.domainProjectById(projectId) : undefined;
    const worktree = normalizeGpuiWorktreeMetadata(project?.worktree);
    if (!project || !worktree) {
      this.postWorktreeToast("warning", "Not a worktree", {
        description: "Only worktree projects can be deleted.",
      });
      return;
    }
    try {
      const [branch, status] = await Promise.all([
        this.runGitAction(project, { action: "branch" }),
        this.runGitAction(project, { action: "status" }),
      ]);
      if (branch.exitCode !== 0 || status.exitCode !== 0) {
        throw new Error("Could not read worktree status.");
      }
      const branchName = normalizeGpuiWorktreeDeleteBranchName(branch.stdout, worktree.branch);
      const branchMetadata = await resolveGpuiWorktreeDeleteBranchMetadata(
        branchName,
        (remoteName, remoteBranchName) =>
          this.runGitAction(project, {
            action: "remoteBranchExists",
            branch: remoteBranchName,
            remoteName,
          }),
      );
      // Delete Worktree opens only after gxserver collects fresh Git status,
      // so dirty checkouts can offer Commit before the destructive removal.
      postAppModalHostMessage(
        {
          modal: "deleteWorktree",
          type: "open",
          worktreeDeleteDraft: {
            ...branchMetadata,
            groupId,
            hasChanges: hasGpuiGitShortStatusChanges(status.stdout),
            projectId: project.projectId,
            statusSummary: status.stdout.trim(),
            worktreeName: project.name || worktree.name || "worktree",
          },
        },
        "AppModals:gpuiDeleteWorktree",
      );
    } catch (error) {
      this.postWorktreeToast("error", "Could not inspect worktree", {
        description: error instanceof Error ? error.message : "git status failed.",
      });
    }
  }

  private async promptDeleteRemoteWorktreeForGroup(groupId: string): Promise<boolean> {
    if (!parseGpuiRemotePresentationGroupId(groupId)) {
      return false;
    }
    const remoteScope = this.resolveRemotePresentationProjectScope({ groupId });
    const presentationProject = remoteScope
      ? (this.findRemotePresentationProject(remoteScope) ?? remoteScope.project)
      : undefined;
    const worktree = normalizeGpuiWorktreeMetadata(presentationProject?.worktree);
    if (!remoteScope || !presentationProject || !worktree) {
      this.postRemoteToast("warning", "Remote worktree unavailable", {
        description: "Reconnect the remote machine and try deleting the worktree again.",
      });
      return true;
    }
    try {
      const [branch, status] = await Promise.all([
        this.runRemoteGitAction(remoteScope, { action: "branch" }),
        this.runRemoteGitAction(remoteScope, { action: "status" }),
      ]);
      if (branch.exitCode !== 0 || status.exitCode !== 0) {
        throw new Error("Could not read remote worktree status.");
      }
      const branchName = normalizeGpuiWorktreeDeleteBranchName(branch.stdout, worktree.branch);
      const branchMetadata = await resolveGpuiWorktreeDeleteBranchMetadata(
        branchName,
        (remoteName, remoteBranchName) =>
          this.runRemoteGitAction(remoteScope, {
            action: "remoteBranchExists",
            branch: remoteBranchName,
            remoteName,
          }),
      );
      postAppModalHostMessage(
        {
          modal: "deleteWorktree",
          type: "open",
          worktreeDeleteDraft: {
            ...branchMetadata,
            groupId,
            hasChanges: hasGpuiGitShortStatusChanges(status.stdout),
            projectId: createGpuiRemotePresentationProjectId(
              remoteScope.machineId,
              remoteScope.projectId,
            ),
            statusSummary: status.stdout.trim(),
            worktreeName: presentationProject.title || worktree.name || "worktree",
          },
        },
        "AppModals:gpuiDeleteWorktree.remote",
      );
    } catch (error) {
      this.postRemoteToast("error", "Could not inspect remote worktree", {
        description: error instanceof Error ? error.message : "git status failed.",
      });
    }
    return true;
  }

  private async confirmDeleteWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "confirmDeleteWorktree" }>,
  ): Promise<void> {
    if (parseGpuiRemotePresentationProjectId(message.projectId)) {
      await this.confirmDeleteRemoteWorktree(message);
      return;
    }
    const project = this.domainProjectById(message.projectId);
    const worktree = normalizeGpuiWorktreeMetadata(project?.worktree);
    if (!project || !worktree || !this.client) {
      this.postWorktreeToast("warning", "Worktree unavailable", {
        description: "The selected worktree no longer exists.",
      });
      return;
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast("info", "Deleting worktree", {
      description: project.name,
      persistent: true,
      toastId,
    });
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Worktree removal is a Git write that does not go through the `runGitAction`
    chokepoint: gxserver removes the worktree from the parent repo and, when
    asked, deletes the branch there too. This flow then focuses the parent, so
    without this the parent could republish a memoized state taken while the
    branch still existed. The removed project's own entries go as well, so a
    later project registered under the same id cannot inherit a dead worktree's
    Git state. Deleting before the RPC mirrors `runGitAction` and covers a
    removal that fails partway through.
    */
    if (parentProject) {
      this.gitStateMemoByProjectId.delete(parentProject.projectId);
    }
    this.gitStateMemoByProjectId.delete(project.projectId);
    this.gitHubStateMemoByProjectId.delete(project.projectId);
    try {
      const result = await this.client.rpc<GxserverDeleteWorktreeProjectResult>(
        "/api/deleteWorktreeProject",
        {
          deleteLocalBranch: message.deleteLocalBranch === true,
          deleteRemoteBranch: message.deleteRemoteBranch === true,
          projectId: project.projectId,
        },
      );
      this.postGxserverWorktreeDeleteWarnings(result);
      this.domainProjects = this.domainProjects.filter(
        (candidate) => candidate.projectId !== project.projectId,
      );
      if (parentProject) {
        this.focusProjectId(parentProject.projectId);
      } else if (this.activeProjectId === project.projectId) {
        const fallbackProjectId = this.domainProjects[0]?.projectId;
        this.activeProjectId = fallbackProjectId;
        this.activeGroupId = fallbackProjectId
          ? createGxserverPresentationProjectGroupId(fallbackProjectId)
          : GPUI_GXSERVER_CHATS_GROUP_ID;
      }
      await this.refreshDomainPresentationFromClient("patch").catch(() => {
        this.publishHudPatch();
      });
      this.postWorktreeToast("success", "Worktree deleted", {
        description: project.name,
        toastId,
      });
    } catch {
      this.postWorktreeToast("error", "Could not delete worktree", {
        description: "gxserver worktree removal failed.",
        toastId,
      });
    }
  }

  private async confirmDeleteRemoteWorktree(
    message: Extract<SidebarToExtensionMessage, { type: "confirmDeleteWorktree" }>,
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({
      projectId: message.projectId,
    });
    if (!remoteScope) {
      this.postRemoteToast("warning", "Remote worktree unavailable", {
        description: "Reconnect the remote machine and try deleting the worktree again.",
      });
      return;
    }
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast("info", "Deleting remote worktree", {
      persistent: true,
      toastId,
    });
    try {
      const result = await this.requestRemoteGxserver<GxserverDeleteWorktreeProjectResult>(
        remoteScope.machineId,
        "/api/deleteWorktreeProject",
        {
          deleteLocalBranch: message.deleteLocalBranch === true,
          deleteRemoteBranch: message.deleteRemoteBranch === true,
          projectId: remoteScope.projectId,
        },
        { timeoutMs: 45_000 },
      );
      this.postGxserverWorktreeDeleteWarnings(result);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
        () => undefined,
      );
      this.postWorktreeToast("success", "Remote worktree deleted", { toastId });
    } catch {
      this.postWorktreeToast("error", "Could not delete remote worktree", {
        description: "Remote gxserver worktree removal failed.",
        toastId,
      });
    }
  }

  private postGxserverWorktreeDeleteWarnings(result: GxserverDeleteWorktreeProjectResult): void {
    for (const warning of result.warnings) {
      switch (warning.kind) {
        case "localBranchDeleteFailed":
        case "localBranchNotResolved":
          this.postGitToast(
            "warning",
            "Worktree removed, but local branch cleanup needs attention",
          );
          break;
        case "remoteBranchDeleteFailed":
        case "remoteBranchNotResolved":
          this.postGitToast(
            "warning",
            "Worktree removed, but remote branch cleanup needs attention",
          );
          break;
        case "pruneFailed":
          this.postGitToast(
            "warning",
            "Worktree removed, but stale metadata cleanup needs attention",
          );
          break;
      }
    }
  }

  private async runSidebarGitMultipleCommits(requestId: string, agentId?: string): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(requestId);
    this.pendingGitCommitRequests.delete(requestId);
    if (pending?.remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
      if (!remoteScope) {
        this.postRemoteToast("warning", "Remote Git unavailable", {
          description: "Reconnect the remote machine before starting this Git workflow.",
        });
        return;
      }
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Multiple Commits",
        GPUI_GIT_MULTIPLE_COMMITS_PROMPT,
        agentId,
      );
      return;
    }
    const project = pending
      ? this.domainProjectById(pending.projectId)
      : this.activeDomainProject();
    if (!project) {
      this.postGitToast("warning", "Git unavailable", {
        description: "No active gxserver project is available.",
      });
      this.publishHudPatch();
      return;
    }
    await this.runSidebarGitPromptAction(
      project,
      "Multiple Commits",
      GPUI_GIT_MULTIPLE_COMMITS_PROMPT,
      agentId,
    );
  }

  private promptSidebarGitActionReview(
    project: GxserverProjectDomainState,
    gitState: SidebarGitState,
    action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
  ): void {
    const requestId = `gpui-git-action-${Date.now().toString(36)}`;
    const hasCommit = gitState.hasWorkingTreeChanges;
    /*
    CDXC:GPUISidebarGit 2026-06-24-15:22:
    GPUI commit review stores the gxserver-derived changed-file list with the request id. Later modal selections and diff clicks may only reference those paths, so CEF cannot stage or inspect arbitrary renderer-supplied paths.
    Treat the modal's all-selected case as that stored review list instead of a fresh unbounded add-all, so files created after review opens cannot slip into the confirmed commit.
    */
    this.pendingGitCommitRequests.set(requestId, {
      action,
      files: [...gitState.files],
      hasCommit,
      projectId: project.projectId,
      subject: "",
    });
    const modalDraft: SidebarPromptGitCommitMessage = {
      action,
      agentId: this.resolveDefaultPromptAgent()?.agentId,
      branch: gitState.branch,
      changedFiles: gitState.files,
      confirmLabel: resolveGpuiSidebarGitConfirmLabel(action, hasCommit),
      deleteWorktreeAfterDefault: false,
      description: hasCommit
        ? "Review and confirm your commit. Leave the message blank to auto-generate one."
        : resolveGpuiSidebarGitPromptDescription(action),
      isDefaultRef: gitState.branch === "main" || gitState.branch === "master",
      isWorktree: normalizeGpuiWorktreeMetadata(project.worktree) !== undefined,
      requestId,
      showCommitMessage: hasCommit,
      suggestedBody: undefined,
      suggestedSubject: "",
      type: "promptGitCommit",
      worktreeName: stringFromRecord(project.worktree, "name"),
    };
    this.openSidebarGitCommitReviewModal(modalDraft);
    this.gitState = { ...gitState, isBusy: false };
    this.publishHudPatch();
  }

  private openSidebarGitCommitReviewModal(draft: SidebarPromptGitCommitMessage): void {
    openAppModal({
      gitCommitDraft: draft,
      modal: "gitCommit",
      type: "open",
    });
  }

  private async openSidebarGitChangedFileDiff(filePath: string, requestId?: string): Promise<void> {
    const request = requestId ? this.pendingGitCommitRequests.get(requestId) : undefined;
    if (request?.remoteReference) {
      await this.openRemoteSidebarGitChangedFileDiff(request.remoteReference, filePath, requestId);
      return;
    }
    const project = request ? this.domainProjectById(request.projectId) : undefined;
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(filePath);
    if (!requestId || !request || !project || !normalizedFilePath) {
      return;
    }
    const reviewFile = request.files.find((file) => file.path === normalizedFilePath);
    if (!reviewFile) {
      return;
    }
    try {
      const [stagedDiff, unstagedDiff] = await Promise.all([
        this.runGitAction(project, {
          action: "diffCachedNoExt",
          filePath: normalizedFilePath,
        }),
        this.runGitAction(project, {
          action: "diffNoExt",
          filePath: normalizedFilePath,
        }),
      ]);
      const patchParts = [stagedDiff.stdout.trimEnd(), unstagedDiff.stdout.trimEnd()].filter(
        (part) => part.trim().length > 0,
      );
      let patch = patchParts.join("\n\n");
      if (!patch.trim()) {
        const untracked = await this.runGitAction(project, {
          action: "isUntrackedFile",
          filePath: normalizedFilePath,
        });
        if (untracked.stdout.trim()) {
          const noIndexDiff = await this.runGitAction(project, {
            action: "diffNoIndexAgainstNull",
            filePath: normalizedFilePath,
          });
          patch = noIndexDiff.stdout.trimEnd() || noIndexDiff.stderr.trimEnd();
        }
      }
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: patch.trim() || `No diff is available for ${normalizedFilePath}.`,
      });
    } catch {
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: `No diff is available for ${normalizedFilePath}.`,
      });
    }
  }

  private async openRemoteSidebarGitChangedFileDiff(
    remoteReference: GpuiRemoteProjectReference,
    filePath: string,
    requestId?: string,
  ): Promise<void> {
    const request = requestId ? this.pendingGitCommitRequests.get(requestId) : undefined;
    const remoteScope = this.resolveRemotePresentationProjectScope(remoteReference);
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(filePath);
    if (!requestId || !request || !remoteScope || !normalizedFilePath) {
      return;
    }
    const reviewFile = request.files.find((file) => file.path === normalizedFilePath);
    if (!reviewFile) {
      return;
    }
    try {
      const [stagedDiff, unstagedDiff] = await Promise.all([
        this.runRemoteGitAction(remoteScope, {
          action: "diffCachedNoExt",
          filePath: normalizedFilePath,
        }),
        this.runRemoteGitAction(remoteScope, {
          action: "diffNoExt",
          filePath: normalizedFilePath,
        }),
      ]);
      const patchParts = [stagedDiff.stdout.trimEnd(), unstagedDiff.stdout.trimEnd()].filter(
        (part) => part.trim().length > 0,
      );
      let patch = patchParts.join("\n\n");
      if (!patch.trim()) {
        const untracked = await this.runRemoteGitAction(remoteScope, {
          action: "isUntrackedFile",
          filePath: normalizedFilePath,
        });
        if (untracked.stdout.trim()) {
          const noIndexDiff = await this.runRemoteGitAction(remoteScope, {
            action: "diffNoIndexAgainstNull",
            filePath: normalizedFilePath,
          });
          patch = noIndexDiff.stdout.trimEnd() || noIndexDiff.stderr.trimEnd();
        }
      }
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: patch.trim() || `No diff is available for ${normalizedFilePath}.`,
      });
    } catch {
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: `No diff is available for ${normalizedFilePath}.`,
      });
    }
  }

  private async openSidebarGitChangedFileInIde(
    message: Extract<SidebarToExtensionMessage, { type: "openSidebarGitChangedFile" }>,
  ): Promise<void> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-21:26:
    Changed-file IDE opens reuse the shared SidebarApp file row. GPUI sends Rust only the gxserver project id and a normalized relative file candidate already present in the current HUD or review request; Rust remains authoritative and re-validates the file against gxserver before resolving an absolute path.
    Scoped non-review opens must re-read the owning local or remote gxserver project instead of using the active local HUD file list, so remote rows cannot open stale or cross-project file candidates.
    */
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(message.filePath);
    const request = message.requestId
      ? this.pendingGitCommitRequests.get(message.requestId)
      : undefined;
    if (request?.remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope(request.remoteReference);
      if (
        !normalizedFilePath ||
        !remoteScope ||
        !request.files.some((file) => file.path === normalizedFilePath)
      ) {
        this.postRemoteToast("warning", "Remote file open unavailable", {
          description: "Choose a changed file from the current remote Git review.",
        });
        return;
      }
      this.postRemoteProjectNativeAction(
        "openRemoteSidebarGitChangedFileInIde",
        remoteScope,
        message,
        {
          filePath: normalizedFilePath,
        },
      );
      return;
    }
    if (!request) {
      const remoteScope = this.resolveGitPreferenceRemoteScope(message);
      if (remoteScope) {
        if (!normalizedFilePath) {
          this.postRemoteToast("warning", "Remote file open unavailable", {
            description: "Choose a changed file from the current remote Git state.",
          });
          return;
        }
        const gitState = await this.readRemoteSidebarGitState(remoteScope);
        if (!gitState.files.some((file) => file.path === normalizedFilePath)) {
          this.postRemoteToast("warning", "Remote file open unavailable", {
            description: "Choose a changed file from the current remote Git state.",
          });
          return;
        }
        this.postRemoteProjectNativeAction(
          "openRemoteSidebarGitChangedFileInIde",
          remoteScope,
          message,
          {
            filePath: normalizedFilePath,
          },
        );
        return;
      }
      if (this.isGitPreferenceRemoteScope(message)) {
        this.postRemoteToast("warning", "Remote file open unavailable", {
          description: "Reconnect the remote machine before opening changed files.",
        });
        return;
      }
    }
    const project = request
      ? this.domainProjectById(request.projectId)
      : this.activeDomainProject();
    const explicitScope = !request && Boolean(message.groupId?.trim() || message.projectId?.trim());
    const scopedProject = request
      ? project
      : (this.resolveGitPreferenceLocalProject(message) ?? (explicitScope ? undefined : project));
    const trustedFiles =
      request?.files ??
      (scopedProject && scopedProject.projectId !== this.activeProjectId
        ? (await this.readSidebarGitState(scopedProject)).files
        : this.gitState.files);
    if (
      !normalizedFilePath ||
      !scopedProject ||
      scopedProject.isRecentProject === true ||
      !trustedFiles.some((file) => file.path === normalizedFilePath)
    ) {
      this.postGitToast("warning", "Open file unavailable", {
        description: "Choose a changed file from the current Git state.",
      });
      return;
    }
    this.postNativeProjectPathAction(
      "openSidebarGitChangedFileInIde",
      scopedProject.projectId,
      message,
      {
        filePath: normalizedFilePath,
      },
    );
  }

  private postSidebarGitFileDiff(requestId: string, draft: SidebarGitFileDiffDraft): void {
    postAppModalHostMessage(
      {
        gitFileDiff: draft,
        modal: "gitFileDiff",
        requestId,
        type: "open",
      },
      "AppModals:gpuiGitFileDiff",
    );
  }

  private resolveTrustedGitReviewFileSelection(
    request: GpuiPendingGitCommitRequest,
    filePaths?: readonly string[],
  ): GpuiTrustedGitReviewFileSelection {
    const explicit = filePaths !== undefined;
    const candidatePaths = explicit ? filePaths : request.files.map((file) => file.path);
    const allowedPaths = new Map(request.files.map((file) => [file.path, file.path]));
    const selectedPaths: string[] = [];
    for (const filePath of candidatePaths) {
      const normalizedPath = normalizeGpuiRelativeGitFilePath(filePath);
      const trustedPath = normalizedPath ? allowedPaths.get(normalizedPath) : undefined;
      if (!trustedPath) {
        throw new Error("Selected file is not part of the current Git review.");
      }
      if (!selectedPaths.includes(trustedPath)) {
        selectedPaths.push(trustedPath);
      }
    }
    if (selectedPaths.length === 0) {
      throw new Error("Select at least one changed file.");
    }
    return { explicit, filePaths: selectedPaths };
  }

  private async runGitMutation(
    project: GxserverProjectDomainState,
    startedTitle: string,
    finishedTitle: string,
    operation: () => Promise<void>,
  ): Promise<boolean> {
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", startedTitle, { persistent: true, toastId });
    this.gitState = { ...this.gitStateForHud(), isBusy: true };
    this.publishHudPatch();
    try {
      await operation();
      await this.refreshGitState({ force: true, project });
      this.postGitToast("success", finishedTitle, { toastId });
      return true;
    } catch (error) {
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      this.postGitToast("error", `${startedTitle} failed`, {
        description: gpuiUserVisibleGitErrorMessage(error, "gxserver Git operation failed."),
        toastId,
      });
      return false;
    }
  }

  private async commitWithMessage(
    project: GxserverProjectDomainState,
    message: string,
    filePaths?: readonly string[],
    options: { agentId?: string; commitOnNewRef?: boolean } = {},
  ): Promise<void> {
    const parsedMessage = parseGpuiSidebarGitCommitMessage(message);
    let resolvedMessage = parsedMessage;
    if (parsedMessage.subject) {
      const addResult = await this.runGitAction(project, {
        action: "addAll",
        filePaths,
      });
      if (addResult.exitCode !== 0) {
        throw new Error("Could not stage changes.");
      }
    } else {
      resolvedMessage = await this.generateCommitMessage(project, filePaths, options.agentId);
    }
    if (options.commitOnNewRef) {
      await this.checkoutSidebarGitFeatureBranch(project, resolvedMessage.subject);
    }
    const commitResult = await this.runGitAction(project, {
      action: "commit",
      messageBody: resolvedMessage.body,
      messageSubject: resolvedMessage.subject,
      noVerify: await this.shouldBypassMissingBeadsDatabasePreCommitHook(project),
    });
    if (commitResult.exitCode !== 0) {
      throw new Error("Could not commit changes.");
    }
  }

  private async generateCommitMessage(
    project: GxserverProjectDomainState,
    filePaths: readonly string[] | undefined,
    agentId?: string,
  ): Promise<{ body: string; subject: string }> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    if (!filePaths || filePaths.length === 0) {
      throw new Error("Select at least one changed file before generating a commit message.");
    }
    const agent = this.resolveDefaultPromptAgent(agentId);
    if (!agent?.command?.trim()) {
      throw new GpuiUserVisibleGitError(
        "Choose a configured prompt agent before generating a commit message.",
      );
    }
    if (!supportsGpuiBackgroundCommitMessageGeneration(agent)) {
      throw new GpuiUserVisibleGitError(
        "Selected prompt agent does not support background commit message generation.",
      );
    }
    return this.client.rpc<GxserverGenerateCommitMessageResult>("/api/generateCommitMessage", {
      agentId: agent.agentId,
      filePaths: [...filePaths],
      projectId: project.projectId,
    });
  }

  private async generateRemoteCommitMessage(
    remoteScope: GpuiRemoteProjectScope,
    filePaths: readonly string[] | undefined,
    agentId?: string,
  ): Promise<{ body: string; subject: string }> {
    if (!filePaths || filePaths.length === 0) {
      throw new Error("Select at least one changed file before generating a commit message.");
    }
    const resolvedAgentId = this.resolveDefaultPromptAgentId(agentId);
    if (!resolvedAgentId) {
      throw new GpuiUserVisibleGitError(
        "Choose a prompt agent before generating a remote commit message.",
      );
    }
    return this.requestRemoteGxserver<GxserverGenerateCommitMessageResult>(
      remoteScope.machineId,
      "/api/generateCommitMessage",
      {
        agentId: resolvedAgentId,
        filePaths: [...filePaths],
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 125_000 },
    );
  }

  private async checkoutSidebarGitFeatureBranch(
    project: GxserverProjectDomainState,
    subject: string,
  ): Promise<string> {
    const baseName = sanitizeGpuiSidebarGitBranchName(subject);
    for (let index = 0; index < 20; index += 1) {
      const candidate = index === 0 ? baseName : `${baseName}-${index + 1}`;
      const exists = await this.runGitAction(project, {
        action: "verifyRef",
        ref: candidate,
      });
      if (exists.exitCode !== 0) {
        const checkout = await this.runGitAction(project, {
          action: "checkoutNewBranch",
          branch: candidate,
        });
        if (checkout.exitCode !== 0) {
          throw new Error("Could not create a new branch.");
        }
        return candidate;
      }
    }
    throw new Error("Could not create a unique branch.");
  }

  private async pushCurrentBranch(
    project: GxserverProjectDomainState,
    gitState: Pick<SidebarGitState, "branch" | "behindCount" | "hasOriginRemote" | "hasUpstream">,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before pushing.");
    }
    if (gitState.behindCount > 0) {
      throw new Error("Branch is behind upstream.");
    }
    const push = gitState.hasUpstream
      ? await this.runGitAction(project, { action: "push" })
      : gitState.hasOriginRemote
        ? await this.runGitAction(project, { action: "pushSetUpstream", branch })
        : undefined;
    if (!push) {
      throw new Error('Add an "origin" remote before pushing.');
    }
    if (push.exitCode !== 0) {
      throw new Error("Could not push branch.");
    }
  }

  private async syncCurrentBranchWithRemote(
    project: GxserverProjectDomainState,
    gitState: SidebarGitState,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before syncing.");
    }
    if (gitState.hasUpstream) {
      const pull = await this.runGitAction(project, { action: "pullFastForward" });
      if (pull.exitCode !== 0) {
        throw new Error("Could not pull branch.");
      }
      const nextGitState = await this.refreshGitState({ force: true, project });
      if (nextGitState.aheadCount > 0) {
        await this.pushCurrentBranch(project, nextGitState);
      }
      return;
    }
    await this.pushCurrentBranch(project, gitState);
  }

  private async commitRemoteWithMessage(
    remoteScope: GpuiRemoteProjectScope,
    message: string,
    filePaths?: readonly string[],
    options: { agentId?: string; commitOnNewRef?: boolean } = {},
  ): Promise<void> {
    const parsedMessage = parseGpuiSidebarGitCommitMessage(message);
    let resolvedMessage = parsedMessage;
    if (parsedMessage.subject) {
      const addResult = await this.runRemoteGitAction(remoteScope, {
        action: "addAll",
        filePaths,
      });
      if (addResult.exitCode !== 0) {
        throw new Error("Could not stage remote changes.");
      }
    } else {
      resolvedMessage = await this.generateRemoteCommitMessage(
        remoteScope,
        filePaths,
        options.agentId,
      );
    }
    if (options.commitOnNewRef) {
      await this.checkoutRemoteSidebarGitFeatureBranch(remoteScope, resolvedMessage.subject);
    }
    const commitResult = await this.runRemoteGitAction(remoteScope, {
      action: "commit",
      messageBody: resolvedMessage.body,
      messageSubject: resolvedMessage.subject,
      noVerify: await this.shouldBypassRemoteMissingBeadsDatabasePreCommitHook(remoteScope),
    });
    if (commitResult.exitCode !== 0) {
      throw new Error("Could not commit remote changes.");
    }
  }

  private async checkoutRemoteSidebarGitFeatureBranch(
    remoteScope: GpuiRemoteProjectScope,
    subject: string,
  ): Promise<void> {
    const result = await this.requestRemoteGxserver<GxserverCheckoutProjectNewBranchResult>(
      remoteScope.machineId,
      "/api/checkoutProjectNewBranch",
      {
        branchLabel: subject,
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 30_000 },
    );
    if (result.checkedOut !== true) {
      throw new Error("Could not create a new remote branch.");
    }
  }

  private async pushRemoteCurrentBranch(
    remoteScope: GpuiRemoteProjectScope,
    gitState: Pick<SidebarGitState, "branch" | "behindCount" | "hasOriginRemote" | "hasUpstream">,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before pushing.");
    }
    if (gitState.behindCount > 0) {
      throw new Error("Remote branch is behind upstream.");
    }
    const push = gitState.hasUpstream
      ? await this.runRemoteGitAction(remoteScope, { action: "push" })
      : gitState.hasOriginRemote
        ? await this.runRemoteGitAction(remoteScope, { action: "pushSetUpstreamCurrent" })
        : undefined;
    if (!push) {
      throw new Error('Add an "origin" remote before pushing.');
    }
    if (push.exitCode !== 0) {
      throw new Error("Could not push remote branch.");
    }
  }

  private async syncRemoteCurrentBranchWithRemote(
    remoteScope: GpuiRemoteProjectScope,
    gitState: SidebarGitState,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before syncing.");
    }
    if (gitState.hasUpstream) {
      const pull = await this.runRemoteGitAction(remoteScope, { action: "pullFastForward" });
      if (pull.exitCode !== 0) {
        throw new Error("Could not pull remote branch.");
      }
      const nextGitState = await this.readRemoteSidebarGitState(remoteScope);
      if (nextGitState.aheadCount > 0) {
        await this.pushRemoteCurrentBranch(remoteScope, nextGitState);
      }
      return;
    }
    await this.pushRemoteCurrentBranch(remoteScope, gitState);
  }

  private async shouldBypassRemoteMissingBeadsDatabasePreCommitHook(
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<boolean> {
    const beadsStorage = await this.runRemoteBeadsAction(remoteScope, { action: "storageExists" });
    if (beadsStorage.exitCode !== 0 || beadsStorage.stdout.trim() !== "true") {
      return false;
    }
    try {
      const status = await this.runRemoteBeadsAction(remoteScope, { action: "status" });
      return (
        status.exitCode !== 0 &&
        isMissingGpuiBeadsDatabaseError(`${status.stderr}\n${status.stdout}`)
      );
    } catch {
      return false;
    }
  }

  private async shouldBypassMissingBeadsDatabasePreCommitHook(
    project: GxserverProjectDomainState,
  ): Promise<boolean> {
    const beadsStorage = await this.runBeadsAction(project, { action: "storageExists" });
    if (beadsStorage.exitCode !== 0 || beadsStorage.stdout.trim() !== "true") {
      return false;
    }
    try {
      const status = await this.runBeadsAction(project, { action: "status" });
      return (
        status.exitCode !== 0 &&
        isMissingGpuiBeadsDatabaseError(`${status.stderr}\n${status.stdout}`)
      );
    } catch {
      return false;
    }
  }

  private async runSidebarGitPromptAction(
    project: GxserverProjectDomainState,
    title: string,
    prompt: string,
    agentId?: string,
  ): Promise<void> {
    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository to use this workflow.",
      });
      return;
    }
    const agent = this.resolveDefaultPromptAgent(agentId);
    if (!agent?.command?.trim()) {
      this.postGitToast("error", "Agent unavailable", {
        description: "Choose a configured prompt agent before starting this Git workflow.",
      });
      return;
    }
    await this.createAgentSessionForProject(
      project,
      agent,
      prompt,
      formatGpuiGitAgentWorkflowTitle(title),
    );
    this.postGitToast("success", "Git workflow started");
  }

  private async runRemoteSidebarGitPromptAction(
    remoteScope: GpuiRemoteProjectScope,
    title: string,
    prompt: string,
    agentId?: string,
  ): Promise<void> {
    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Open a Git repository on the remote machine to use this workflow.",
      });
      return;
    }
    const resolvedAgentId = this.resolveDefaultPromptAgentId(agentId);
    try {
      await this.createRemoteAgentSessionForProject(
        remoteScope,
        resolvedAgentId,
        prompt,
        formatGpuiGitAgentWorkflowTitle(title),
      );
      this.postRemoteToast("success", "Remote Git workflow started");
    } catch {
      this.postRemoteToast("error", "Remote Git workflow failed", {
        description: "The remote gxserver could not start the selected prompt agent.",
      });
    }
  }

  private async runSidebarGitPullRequestAgentWorkflow(input: {
    agentId?: string;
    filePaths?: readonly string[];
    gitState: SidebarGitState;
    hasExplicitFileSelection: boolean;
    hasCommit: boolean;
    message: string;
    project: GxserverProjectDomainState;
  }): Promise<void> {
    const agent = this.resolveDefaultPromptAgent(input.agentId);
    if (!agent?.command?.trim()) {
      this.postGitToast("error", "Agent unavailable", {
        description: "Choose a configured prompt agent before creating a pull request.",
      });
      return;
    }
    /*
    CDXC:GPUISidebarGit 2026-06-24-16:45:
    Visible PR-agent workflows are for user-observable, non-delete PR creation only. The terminal session can report gxserver lifecycle/activity, but it cannot prove that `gh pr create` produced an open PR; delete-after cleanup must stay on the direct gxserver PR result path.
    */
    const prompt = buildGpuiGitPullRequestAgentPrompt({
      filePaths: input.filePaths,
      hasExplicitFileSelection: input.hasExplicitFileSelection,
      hasCommit: input.hasCommit,
      message: input.message.trim(),
      selectedFiles:
        input.filePaths && input.filePaths.length > 0
          ? input.filePaths
          : input.gitState.files.map((file) => file.path),
    });
    try {
      await this.createAgentSessionForProject(
        input.project,
        agent,
        prompt,
        formatGpuiGitAgentWorkflowTitle("Commit, Push & PR"),
      );
      this.postGitToast("success", "Pull request workflow started");
    } catch {
      this.postGitToast("error", "Pull request workflow failed", {
        description: "gxserver could not start the selected prompt agent.",
      });
    }
  }

  private async runRemoteSidebarGitPullRequestAgentWorkflow(input: {
    agentId?: string;
    filePaths?: readonly string[];
    gitState: SidebarGitState;
    hasExplicitFileSelection: boolean;
    hasCommit: boolean;
    message: string;
    remoteScope: GpuiRemoteProjectScope;
  }): Promise<void> {
    const resolvedAgentId = this.resolveDefaultPromptAgentId(input.agentId);
    const prompt = buildGpuiGitPullRequestAgentPrompt({
      filePaths: input.filePaths,
      hasExplicitFileSelection: input.hasExplicitFileSelection,
      hasCommit: input.hasCommit,
      message: input.message.trim(),
      selectedFiles:
        input.filePaths && input.filePaths.length > 0
          ? input.filePaths
          : input.gitState.files.map((file) => file.path),
    });
    try {
      await this.createRemoteAgentSessionForProject(
        input.remoteScope,
        resolvedAgentId,
        prompt,
        formatGpuiGitAgentWorkflowTitle("Commit, Push & PR"),
      );
      this.postRemoteToast("success", "Remote pull request workflow started");
    } catch {
      this.postRemoteToast("error", "Remote pull request workflow failed", {
        description: "The remote gxserver could not start the selected prompt agent.",
      });
    }
  }

  private async persistGitPreferences(
    updates: Partial<GpuiGitPreferences>,
    scopeMessage?: {
      groupId?: string;
      projectId?: string;
    },
  ): Promise<void> {
    const explicitScope = Boolean(scopeMessage?.groupId?.trim() || scopeMessage?.projectId?.trim());
    const remoteScope = this.resolveGitPreferenceRemoteScope(scopeMessage);
    if (remoteScope) {
      await this.persistRemoteGitPreferences(remoteScope, updates);
      return;
    }
    if (explicitScope && this.isGitPreferenceRemoteScope(scopeMessage)) {
      this.postRemoteToast("warning", "Remote Git preferences unavailable", {
        description: "Reconnect the remote machine before changing Git preferences.",
      });
      return;
    }

    const scopedProject = this.resolveGitPreferenceLocalProject(scopeMessage);
    if (explicitScope && !scopedProject) {
      this.postGitToast("warning", "Git preferences unavailable", {
        description: "Choose a current project before changing Git preferences.",
      });
      return;
    }
    const currentPreferences = this.gitPreferencesForProject(
      scopedProject ?? this.activeDomainProject(),
    );
    const nextPreferences: GpuiGitPreferences = {
      ...currentPreferences,
      ...updates,
      primaryAction: normalizeSidebarGitAction(
        updates.primaryAction ?? currentPreferences.primaryAction,
      ),
    };
    if (scopedProject && this.client) {
      const nextProject = await this.updateProjectDomainState(scopedProject.projectId, {
        gitConfig: {
          ...scopedProject.gitConfig,
          confirmCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        },
      });
      if (
        this.activeProjectId === scopedProject.projectId ||
        this.activeProjectId === nextProject?.projectId
      ) {
        this.gitState = {
          ...this.gitState,
          confirmSuggestedCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        };
        this.publishHudPatch();
      }
      return;
    }
    if (!this.client || this.domainProjects.length === 0) {
      this.gitState = {
        ...this.gitState,
        confirmSuggestedCommit: nextPreferences.confirmCommit,
        generateCommitBody: nextPreferences.generateCommitBody,
        primaryAction: nextPreferences.primaryAction,
      };
      this.publishHudPatch();
      return;
    }
    await Promise.all(
      this.domainProjects.map((project) =>
        this.updateProjectDomainState(project.projectId, {
          gitConfig: {
            ...project.gitConfig,
            confirmCommit: nextPreferences.confirmCommit,
            generateCommitBody: nextPreferences.generateCommitBody,
            primaryAction: nextPreferences.primaryAction,
          },
        }),
      ),
    );
    this.gitState = {
      ...this.gitState,
      confirmSuggestedCommit: nextPreferences.confirmCommit,
      generateCommitBody: nextPreferences.generateCommitBody,
      primaryAction: nextPreferences.primaryAction,
    };
    this.publishHudPatch();
  }

  private resolveGitPreferenceRemoteScope(scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): GpuiRemoteProjectScope | undefined {
    if (!scopeMessage) {
      return undefined;
    }
    if (scopeMessage.groupId && parseGpuiRemotePresentationGroupId(scopeMessage.groupId)) {
      return this.resolveRemotePresentationProjectScope({ groupId: scopeMessage.groupId });
    }
    const remoteProject = scopeMessage.projectId
      ? parseGpuiRemotePresentationProjectId(scopeMessage.projectId)
      : undefined;
    return remoteProject ? this.resolveRemotePresentationProjectScope(remoteProject) : undefined;
  }

  private isGitPreferenceRemoteScope(scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): boolean {
    return Boolean(
      (scopeMessage?.groupId && parseGpuiRemotePresentationGroupId(scopeMessage.groupId)) ||
      (scopeMessage?.projectId && parseGpuiRemotePresentationProjectId(scopeMessage.projectId)),
    );
  }

  private resolveGitPreferenceLocalProject(scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): GxserverProjectDomainState | undefined {
    if (scopeMessage?.groupId) {
      const projectId = this.resolveProjectIdForGroup(scopeMessage.groupId);
      return projectId ? this.domainProjectById(projectId) : undefined;
    }
    if (scopeMessage?.projectId) {
      return this.domainProjectById(scopeMessage.projectId);
    }
    return undefined;
  }

  private async persistRemoteGitPreferences(
    remoteScope: GpuiRemoteProjectScope,
    updates: Partial<GpuiGitPreferences>,
  ): Promise<void> {
    const currentPreferences = this.gitPreferencesForPresentationProject(
      this.findRemotePresentationProject(remoteScope) ?? remoteScope.project,
    );
    const nextPreferences: GpuiGitPreferences = {
      ...currentPreferences,
      ...updates,
      primaryAction: normalizeSidebarGitAction(
        updates.primaryAction ?? currentPreferences.primaryAction,
      ),
    };
    /*
    CDXC:GPUIRemoteGit 2026-06-24-18:22:
    Remote Git preference writes use only the selected machine id, gxserver project id, and the three known preference keys. Rust owns the tunnel and response shaping; the renderer never sends paths, labels, branch names, command text, URLs, tokens, stdout/stderr, or raw daemon bodies as write authority.
    */
    try {
      const response = await this.requestRemoteGxserver<{
        project?: GxserverPresentationProject;
      }>(remoteScope.machineId, "/api/updateProject", {
        gitConfig: {
          confirmCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        },
        projectId: remoteScope.projectId,
      });
      if (response.project) {
        this.upsertRemotePresentationProject(remoteScope.machineId, response.project);
      } else {
        await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
          () => undefined,
        );
      }
      if (
        this.activeGroupId ===
        createGpuiRemotePresentationGroupId(remoteScope.machineId, remoteScope.projectId)
      ) {
        this.gitState = {
          ...this.gitState,
          confirmSuggestedCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        };
      }
      this.publishRemotePresentationPatch();
    } catch {
      this.postRemoteToast("warning", "Remote Git preferences unavailable", {
        description: "The remote gxserver could not save that Git preference.",
      });
    }
  }

  private resolveGitProjectForMessage(
    message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>,
  ): GxserverProjectDomainState | undefined {
    const projectId = message.groupId
      ? this.resolveProjectIdForGroup(message.groupId)
      : (message.projectId ?? this.activeProjectId);
    const project = projectId ? this.domainProjectById(projectId) : this.activeDomainProject();
    if (project && this.activeProjectId !== project.projectId) {
      this.focusProjectId(project.projectId);
      this.publishPresentation("patch");
    }
    return project;
  }

  private gitStateForHud(): SidebarGitState {
    const preferences = this.gitPreferencesForProject(this.activeDomainProject());
    return {
      ...this.gitState,
      confirmSuggestedCommit: preferences.confirmCommit,
      generateCommitBody: preferences.generateCommitBody,
      primaryAction: preferences.primaryAction,
    };
  }

  private gitPreferencesForProject(
    project: GxserverProjectDomainState | undefined,
  ): GpuiGitPreferences {
    return {
      confirmCommit: booleanFromRecord(project?.gitConfig, "confirmCommit") ?? false,
      generateCommitBody: booleanFromRecord(project?.gitConfig, "generateCommitBody") ?? true,
      primaryAction: normalizeSidebarGitAction(
        stringFromRecord(project?.gitConfig, "primaryAction"),
      ),
    };
  }

  private gitPreferencesForPresentationProject(
    project: GxserverPresentationProject | undefined,
  ): GpuiGitPreferences {
    return {
      confirmCommit: booleanFromRecord(project?.gitConfig, "confirmCommit") ?? false,
      generateCommitBody: booleanFromRecord(project?.gitConfig, "generateCommitBody") ?? true,
      primaryAction: normalizeSidebarGitAction(
        stringFromRecord(project?.gitConfig, "primaryAction"),
      ),
    };
  }

  private resolveDefaultPromptAgent(agentId?: string): SidebarAgentButton | undefined {
    const requestedAgentId = this.resolveDefaultPromptAgentId(agentId);
    return this.resolveSidebarAgent(requestedAgentId);
  }

  private resolveDefaultPromptAgentId(agentId?: string): string {
    return (
      agentId?.trim() ||
      this.latestHud.settings?.defaultPromptAgentId?.trim() ||
      DEFAULT_GPUI_PROMPT_AGENT_ID
    );
  }

  private async runGitAction(
    project: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Invalidate at the single chokepoint every Git write goes through, so no
    caller can commit, push, or switch branches and then have a switch back to
    that project republish the pre-mutation state. Deleting before the RPC also
    covers a write that fails halfway.
    */
    if (GPUI_MUTATING_GIT_ACTIONS.has(String(params.action ?? ""))) {
      this.gitStateMemoByProjectId.delete(project.projectId);
    }
    return this.client.rpc<GxserverTypedOperationResult>("/api/runGitAction", {
      ...params,
      projectId: project.projectId,
    });
  }

  private async runRemoteGitAction(
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      "/api/runGitAction",
      {
        ...params,
        projectId: remoteScope.projectId,
      },
    );
  }

  private async runGitHubAction(
    project: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    return this.client.rpc<GxserverTypedOperationResult>("/api/runGitHubAction", {
      ...params,
      projectId: project.projectId,
    });
  }

  private async runRemoteGitHubAction(
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      "/api/runGitHubAction",
      {
        ...params,
        projectId: remoteScope.projectId,
      },
    );
  }

  private async createPullRequest(
    project: GxserverProjectDomainState,
  ): Promise<GxserverCreatePullRequestResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    /*
    CDXC:GPUISidebarGit 2026-06-24-16:28:
    Direct GPUI PR creation must use a gxserver completion result before opening
    the PR or deleting a worktree. The renderer sends only the trusted project
    id; gxserver owns `gh pr create --fill`, current-branch PR lookup, and
    validated state/URL return data.

    CDXC:SidebarGitMemo 2026-07-29:
    This is the sidebar's only pull-request write, so it is the one place the
    long GitHub lease must be torn down: otherwise the badge could keep saying
    "no pull request" for minutes after the user just created one.
    */
    this.gitHubStateMemoByProjectId.delete(project.projectId);
    this.gitStateMemoByProjectId.delete(project.projectId);
    return this.client.rpc<GxserverCreatePullRequestResult>("/api/createPullRequest", {
      projectId: project.projectId,
    });
  }

  private async createRemotePullRequest(
    remoteScope: GpuiRemoteProjectReference,
  ): Promise<GpuiRemoteCreatePullRequestResult> {
    return this.requestRemoteGxserver<GpuiRemoteCreatePullRequestResult>(
      remoteScope.machineId,
      "/api/createPullRequest",
      {
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 45_000 },
    );
  }

  private async runBeadsAction(
    project: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    return this.client.rpc<GxserverTypedOperationResult>("/api/runBeadsAction", {
      ...params,
      projectId: project.projectId,
    });
  }

  private async runRemoteBeadsAction(
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      "/api/runBeadsAction",
      {
        ...params,
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 60_000 },
    );
  }

  private async runRemoteGitMutation(
    remoteScope: GpuiRemoteProjectScope,
    startedTitle: string,
    finishedTitle: string,
    operation: () => Promise<void>,
  ): Promise<boolean> {
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", startedTitle, { persistent: true, toastId });
    try {
      await operation();
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
        () => undefined,
      );
      this.postGitToast("success", finishedTitle, { toastId });
      return true;
    } catch (error) {
      this.postGitToast("error", `${startedTitle} failed`, {
        description: gpuiUserVisibleGitErrorMessage(error, "Remote gxserver Git operation failed."),
        toastId,
      });
      return false;
    }
  }

  private postGitToast(
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description, {
          persistent: options.persistent,
          toastId: options.toastId,
        }),
        "AppModals:gpuiGitToast",
      );
    } catch {
      /*
      CDXC:GPUISidebarGit 2026-06-24-15:22:
      Git mutations and agent workflows must not depend on toast-host availability. Missing toast presentation is not a reason to fake success or skip gxserver-owned Git state changes.
      */
    }
  }

  private postAppShotToast(
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
    } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description),
        "AppModals:gpuiAppShotToast",
      );
    } catch {
      /*
      CDXC:GPUIAppShots 2026-06-25-23:07:
      App Shots user feedback must not depend on toast-host availability and must not log raw app names, window titles, image paths, project paths, command text, terminal content, URLs, or tokens when presentation is unavailable.
      */
    }
  }

  private reconnectRemoteMachine(remoteMachineId: string, installApproved: boolean): void {
    this.clearRemoteStartupReconnectTimeout(remoteMachineId);
    try {
      postAppModalHostMessage(
        {
          installApproved,
          remoteMachineId,
          type: "reconnectRemoteMachine",
        },
        "GPUISidebarRemoteMachines:reconnect",
      );
      this.messageSource.postMessage({
        machineId: remoteMachineId,
        state: "connecting",
        type: "remoteMachineStatus",
      });
    } catch {
      this.scheduleRemoteStartupReconnect(remoteMachineId);
      this.postRemoteToast("warning", "Remote connect unavailable", {
        description: "GPUI could not reach the native remote-machine bridge.",
      });
    }
  }

  private scheduleRemoteStartupReconnect(remoteMachineId: string): void {
    const normalizedMachineId = normalizeNonEmptyString(remoteMachineId);
    if (
      !normalizedMachineId ||
      !this.startupRemoteMachineIds.has(normalizedMachineId) ||
      this.remoteStartupReconnectTimeouts.has(normalizedMachineId)
    ) {
      return;
    }
    const retryAttempts = this.remoteStartupReconnectAttempts.get(normalizedMachineId) ?? 0;
    if (retryAttempts >= GPUI_REMOTE_MACHINE_STARTUP_MAX_RETRIES) {
      this.finishRemoteStartupReconnects(normalizedMachineId);
      return;
    }
    const timeout = window.setTimeout(() => {
      this.remoteStartupReconnectTimeouts.delete(normalizedMachineId);
      const isStillSaved = createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.some(
        (machine) => machine.id === normalizedMachineId,
      );
      if (!isStillSaved) {
        this.finishRemoteStartupReconnects(normalizedMachineId);
        return;
      }
      this.remoteStartupReconnectAttempts.set(normalizedMachineId, retryAttempts + 1);
      this.reconnectRemoteMachine(normalizedMachineId, false);
    }, GPUI_REMOTE_MACHINE_STARTUP_RECONNECT_DELAY_MS);
    this.remoteStartupReconnectTimeouts.set(normalizedMachineId, timeout);
  }

  private clearRemoteStartupReconnectTimeout(remoteMachineId: string): void {
    const timeout = this.remoteStartupReconnectTimeouts.get(remoteMachineId);
    if (timeout === undefined) {
      return;
    }
    window.clearTimeout(timeout);
    this.remoteStartupReconnectTimeouts.delete(remoteMachineId);
  }

  private finishRemoteStartupReconnects(remoteMachineId: string): void {
    this.clearRemoteStartupReconnectTimeout(remoteMachineId);
    this.remoteStartupReconnectAttempts.delete(remoteMachineId);
    this.startupRemoteMachineIds.delete(remoteMachineId);
  }

  private scheduleRemoteGxserverPresentationRecovery(remoteMachineId: string): void {
    const normalizedMachineId = normalizeNonEmptyString(remoteMachineId);
    if (!normalizedMachineId || this.remotePresentationRecoveryTimeouts.has(normalizedMachineId)) {
      return;
    }
    const timeout = window.setTimeout(() => {
      this.remotePresentationRecoveryTimeouts.delete(normalizedMachineId);
      void this.refreshRemotePresentationFromGxserver(normalizedMachineId).finally(() =>
        this.startRemoteGxserverPresentationSubscription(normalizedMachineId),
      );
    }, GPUI_REMOTE_GXSERVER_PRESENTATION_RECOVERY_DELAY_MS);
    this.remotePresentationRecoveryTimeouts.set(normalizedMachineId, timeout);
  }

  private clearRemotePresentationRecoveryTimeout(remoteMachineId: string): void {
    const timeout = this.remotePresentationRecoveryTimeouts.get(remoteMachineId);
    if (timeout === undefined) {
      return;
    }
    window.clearTimeout(timeout);
    this.remotePresentationRecoveryTimeouts.delete(remoteMachineId);
  }

  private startRemoteGxserverPresentationSubscription(remoteMachineId: string): void {
    const normalizedMachineId = normalizeNonEmptyString(remoteMachineId);
    if (!normalizedMachineId) {
      return;
    }
    const snapshot = this.remotePresentations.get(normalizedMachineId);
    const requestId = `remote-presentation-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    try {
      postAppModalHostMessage(
        {
          clientId: `${GPUI_SIDEBAR_DEFAULT_CLIENT_ID}:${normalizedMachineId}`,
          ...(snapshot ? { lastRevision: snapshot.revision } : {}),
          remoteMachineId: normalizedMachineId,
          requestId,
          type: "remoteGxserverSubscribePresentation",
        },
        "GPUISidebarRemoteMachines:subscribePresentation",
      );
    } catch {
      this.postRemoteToast("warning", "Remote sidebar stream unavailable", {
        description: "GPUI could not reach the native remote presentation bridge.",
      });
    }
  }

  private openRemoteCloneRepository(remoteMachineId: string): void {
    /*
    CDXC:RemoteClone 2026-06-24-19:35:
    GPUI remote machine headers reuse the shared Clone Repository modal, but only after the selected machine has a live Rust-delivered gxserver presentation. The renderer may carry the saved machine id/name into the modal; clone preview, Git execution, project registration, and presentation refresh remain Rust/remote-gxserver owned.
    */
    const normalizedMachineId = remoteMachineId.trim();
    if (!normalizedMachineId || !this.remotePresentations.has(normalizedMachineId)) {
      this.postRemoteToast("warning", "Remote clone unavailable", {
        description: "Reconnect the remote machine before cloning a repository.",
      });
      return;
    }
    try {
      openAppModal({
        modal: "addRepository",
        remoteMachineId: normalizedMachineId,
        remoteMachineName: this.remoteMachineName(normalizedMachineId) ?? "Remote",
        type: "open",
      });
    } catch {
      this.postRemoteToast("warning", "Remote clone unavailable", {
        description: "GPUI could not open the shared Clone Repository modal.",
      });
    }
  }

  private requestRemoteGxserver<TResult = unknown>(
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
    options: { timeoutMs?: number } = {},
  ): Promise<TResult> {
    const requestId = `remote-${Date.now().toString(36)}-${++this.remoteGxserverRequestSequence}`;
    const timeoutMs = Math.min(Math.max(options.timeoutMs ?? 20_000, 1_000), 130_000);
    return new Promise<TResult>((resolve, reject) => {
      const timeoutId = window.setTimeout(() => {
        this.pendingRemoteGxserverRequests.delete(requestId);
        reject(new Error("Remote gxserver request timed out."));
      }, timeoutMs + 2_000);
      this.pendingRemoteGxserverRequests.set(requestId, {
        reject,
        resolve: (result) => resolve(result as TResult),
        timeoutId,
      });
      try {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-17:19:
        Response-capable remote sidebar RPCs still carry only a bounded request id plus the allowlisted endpoint params into Rust. Rust owns the live tunnel, token, endpoint allowlist, response sanitization, and presentation refresh; renderer code must not receive tokens, SSH details, command text, URLs, or raw daemon bodies.
        */
        postAppModalHostMessage(
          {
            params,
            path,
            remoteMachineId,
            requestId,
            timeoutMs,
            type: "gpuiRemoteGxserverSidebarRequest",
          },
          "GPUISidebarRemoteMachines:request",
        );
      } catch (error) {
        window.clearTimeout(timeoutId);
        this.pendingRemoteGxserverRequests.delete(requestId);
        reject(error instanceof Error ? error : new Error("Remote gxserver bridge failed."));
      }
    });
  }

  private resolveRemoteGxserverRequest(event: GpuiSidebarRemoteGxserverResponseEvent): void {
    const pending = this.pendingRemoteGxserverRequests.get(event.requestId);
    if (!pending) {
      return;
    }
    window.clearTimeout(pending.timeoutId);
    this.pendingRemoteGxserverRequests.delete(event.requestId);
    if (event.ok) {
      pending.resolve(event.result);
      return;
    }
    pending.reject(new Error(event.error || "Remote gxserver request failed."));
  }

  private postRemoteGxserverSidebarRequest(
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
  ): void {
    try {
      postAppModalHostMessage(
        {
          params,
          path,
          remoteMachineId,
          type: "gpuiRemoteGxserverSidebarRequest",
        },
        "GPUISidebarRemoteMachines:request",
      );
    } catch {
      this.postRemoteToast("warning", "Remote action unavailable", {
        description: "GPUI could not reach the native remote gxserver bridge.",
      });
    }
  }

  private findRemotePresentationSession(reference: {
    machineId: string;
    projectId: string;
    sessionId: string;
  }): GxserverPresentationSession | undefined {
    return this.remotePresentations
      .get(reference.machineId)
      ?.sessions.find(
        (session) =>
          session.projectId === reference.projectId && session.sessionId === reference.sessionId,
      );
  }

  private postRemoteToast(
    level: AppToastLevel,
    title: string,
    options: { description?: string } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description),
        "GPUISidebarRemoteMachines:toast",
      );
    } catch {
      /*
      CDXC:GPUIRemoteMachines 2026-06-24-16:48:
      Remote-machine operations must never depend on toast-host availability. If the shared app-modal toast bridge is missing, keep the native-owned request/status path honest and avoid logging payloads, SSH details, tokens, paths, daemon responses, or renderer contents.
      */
    }
  }

  private resolveRemotePresentationProjectScope(
    input:
      | {
          groupId?: string;
          projectId?: string;
          remoteMachineId?: string;
        }
      | GpuiRemoteProjectReference,
  ): GpuiRemoteProjectScope | undefined {
    const groupReference =
      "groupId" in input && input.groupId
        ? parseGpuiRemotePresentationGroupId(input.groupId)
        : undefined;
    const projectReference =
      !groupReference && "projectId" in input && input.projectId
        ? parseGpuiRemotePresentationProjectId(input.projectId)
        : undefined;
    const machineId =
      groupReference?.machineId ??
      projectReference?.machineId ??
      ("remoteMachineId" in input ? input.remoteMachineId?.trim() : undefined) ??
      ("machineId" in input ? input.machineId : undefined);
    const projectId =
      groupReference?.projectId ??
      projectReference?.projectId ??
      ("projectId" in input ? input.projectId?.trim() : undefined);
    if (!machineId || !projectId) {
      return undefined;
    }
    const presentation = this.remotePresentations.get(machineId);
    const project = presentation?.projects.find((candidate) => candidate.projectId === projectId);
    if (!project) {
      return undefined;
    }
    return {
      machineId,
      machineName: this.remoteMachineName(machineId),
      project,
      projectId,
    };
  }

  private findRemotePresentationProject(
    reference: GpuiRemoteProjectReference,
  ): GxserverPresentationProject | undefined {
    return this.remotePresentations
      .get(reference.machineId)
      ?.projects.find((project) => project.projectId === reference.projectId);
  }

  private upsertRemotePresentationProject(
    remoteMachineId: string,
    nextProject: GxserverPresentationProject,
  ): void {
    const presentation = this.remotePresentations.get(remoteMachineId);
    if (!presentation) {
      return;
    }
    const existingIndex = presentation.projects.findIndex(
      (project) => project.projectId === nextProject.projectId,
    );
    const projects =
      existingIndex >= 0
        ? presentation.projects.map((project, index) =>
            index === existingIndex ? nextProject : project,
          )
        : [...presentation.projects, nextProject];
    this.remotePresentations.set(remoteMachineId, {
      ...presentation,
      projects,
    });
  }

  private removeRemotePresentationProject(remoteMachineId: string, projectId: string): void {
    const presentation = this.remotePresentations.get(remoteMachineId);
    if (!presentation) {
      return;
    }
    this.remotePresentations.set(remoteMachineId, {
      ...presentation,
      groups: presentation.groups.filter((group) => group.projectId !== projectId),
      projects: presentation.projects.filter((project) => project.projectId !== projectId),
      sessions: presentation.sessions.filter((session) => session.projectId !== projectId),
    });
  }

  private remoteMachineName(machineId: string): string | undefined {
    return createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.find(
      (machine) => machine.id === machineId,
    )?.name;
  }

  private resolveRemoteWorktreeFamilyParentProjectFromPresentation(
    sourceProject: GpuiRemoteProjectScope,
  ): GpuiRemoteProjectScope | undefined {
    const parentProjectId = normalizeGpuiWorktreeParentProjectId(sourceProject.project.worktree);
    if (!parentProjectId) {
      return sourceProject;
    }
    const parentProject = this.remotePresentations
      .get(sourceProject.machineId)
      ?.projects.find((project) => project.projectId === parentProjectId);
    return parentProject
      ? {
          machineId: sourceProject.machineId,
          machineName: sourceProject.machineName,
          project: parentProject,
          projectId: parentProject.projectId,
        }
      : undefined;
  }

  private isTrustedRemoteExistingWorktreeKey(
    worktreeKey: string,
    sourceProject: GpuiRemoteProjectScope,
  ): boolean {
    const trusted = this.trustedExistingWorktreeList;
    return Boolean(
      trusted &&
      trusted.remoteMachineId === sourceProject.machineId &&
      trusted.sourceProjectId === sourceProject.projectId &&
      trusted.worktreeKeys?.has(worktreeKey.trim()),
    );
  }

  private async resolveRemoteWorktreeMutationProject(
    remoteMachineId: string,
    project: GxserverPresentationProject | undefined,
  ): Promise<GxserverPresentationProject> {
    if (!project?.projectId) {
      throw new Error("Remote gxserver did not return a worktree project.");
    }
    this.upsertRemotePresentationProject(remoteMachineId, project);
    this.publishRemotePresentationPatch();
    await this.refreshRemotePresentationFromGxserver(remoteMachineId).catch(() => undefined);
    return (
      this.findRemotePresentationProject({
        machineId: remoteMachineId,
        projectId: project.projectId,
      }) ?? project
    );
  }

  private async refreshRemotePresentationFromGxserver(remoteMachineId: string): Promise<void> {
    const response = await this.requestRemoteGxserver<{ snapshot?: unknown }>(
      remoteMachineId,
      "/api/readPresentationSnapshot",
      {},
    );
    if (isPresentationSnapshot(response.snapshot)) {
      const previous = this.remotePresentations.get(remoteMachineId);
      const previousSessions = previous?.sessions ?? [];
      const snapshot = this.projectRemotePresentationAttentionAcknowledgementGuards(
        remoteMachineId,
        response.snapshot,
      );
      if (previous && previous.revision > snapshot.revision) {
        return;
      }
      this.remotePresentations.set(remoteMachineId, snapshot);
      this.pruneRemoteWorkspaceGroupAssignments(remoteMachineId, snapshot);
      this.syncRemotePresentationAttentionTracking(
        remoteMachineId,
        previousSessions,
        snapshot.sessions,
      );
      this.publishRemotePresentationPatch();
    }
  }

  private async registerDomainProjectPath(
    project: GxserverProjectDomainState,
  ): Promise<GxserverProjectDomainState> {
    const path = normalizeGpuiProjectPath(project.path);
    if (!path) {
      throw new Error("Project has no registered path.");
    }
    return this.registerProjectPath({
      name: project.name || gpuiProjectNameFromPath(path),
      path,
    });
  }

  private async registerProjectPath(input: {
    name: string;
    path: string;
  }): Promise<GxserverProjectDomainState> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    const response = await this.client.rpc<{ project: GxserverProjectDomainState }>(
      "/api/addProjectPath",
      {
        name: input.name,
        path: input.path,
      },
    );
    this.upsertDomainProject(response.project);
    return response.project;
  }

  private async ensureWorktreeBeadsHooks(project: GxserverProjectDomainState): Promise<void> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    const result = await this.client.rpc<GxserverTypedOperationResult>("/api/runWorktreeAction", {
      action: "ensureBeadsHooks",
      projectId: project.projectId,
    });
    if (result.exitCode !== 0) {
      throw new Error("Could not prepare Beads hooks for this worktree.");
    }
  }

  private async runWorktreeSetupCommandIfConfigured(
    worktreeProject: GxserverProjectDomainState,
    setupCommandProject: GxserverProjectDomainState,
  ): Promise<void> {
    /*
     * CDXC:GlobalProjectDefaults 2026-08-02:
     * This gate decides whether to call the setup endpoint at all, so it has to
     * see the Global Default the same way gxserver does. Without that, a project
     * inheriting its worktree command would return here and the configured
     * command would never run. gxserver still resolves the command it executes.
     */
    const setupCommand =
      stringFromRecord(setupCommandProject.gitConfig, "worktreeCommand") ??
      normalizeghostexSettings(this.runtimeSettings?.settings).globalWorktreeCommand;
    if (!setupCommand.trim() || !this.client) {
      return;
    }
    const result = await this.client.rpc<GxserverTypedOperationResult>(
      "/api/runProjectSetupCommand",
      {
        action: "worktreeSetupCommand",
        projectId: worktreeProject.projectId,
        setupCommandProjectId: setupCommandProject.projectId,
      },
    );
    if (result.exitCode !== 0) {
      throw new Error("Worktree setup command failed.");
    }
  }

  private async createAgentSessionForProject(
    project: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string,
    title = createAgentSessionDefaultTitle(agent.name),
  ): Promise<string> {
    const defaultTitle = createAgentSessionDefaultTitle(agent.name);
    const renameTitle = title.trim() !== defaultTitle ? title.trim() : undefined;
    /*
    CDXC:GPUIGitAgentWorkflows 2026-07-11-06:14:
    Match macOS `runSidebarGitPromptAction` + `stageNativeAgentPrompt`: create
    Git helpers as fresh neutral agent sessions, start the provider, then submit
    the provider-specific title command, wait for that command to settle, and
    only then submit the workflow prompt. Persisting `Git: Release` or
    `Git: Multiple Commits` before startup makes the missing-provider attach
    path treat a brand-new row as a trusted resume title; a failed lookup then
    leaves the workflow prompt in a plain shell.
    */
    const created = await this.createAgentSessionRecordForProject(project, agent, prompt, {
      renameTitleAfterStart: renameTitle,
      title: defaultTitle,
    });
    return created.sessionId;
  }

  private async createAgentSessionRecordForProject(
    project: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string,
    options: { errorMessage?: string; renameTitleAfterStart?: string; title?: string } = {},
  ): Promise<GpuiCreatedProjectAgentSessionRecord> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    const response = await this.client.rpc<{
      session?: {
        agentSessionId?: string;
        agentSessionPath?: string;
        runtimeSettings?: { agentSessionId?: string; agentSessionPath?: string };
        sessionId?: string;
        zmxName?: string;
      };
    }>("/api/createAgentSession", {
      agentId: agent.agentId,
      launchSettings: {
        agentCommand: agent.command,
        icon: agent.icon,
      },
      projectId: project.projectId,
      runtimeSettings: this.createFirstPromptTitleRuntimeSettings(
        options.renameTitleAfterStart ? undefined : prompt,
      ),
      surface: "workspace",
      title: options.title ?? createAgentSessionDefaultTitle(agent.name),
    });
    const session = response.session;
    const sessionId = normalizeNonEmptyString(session?.sessionId);
    if (!sessionId) {
      throw new Error(options.errorMessage ?? "Could not create an agent session in the worktree.");
    }
    this.focusLocalWorkspaceSession(project.projectId, sessionId);
    const renameTitle = normalizeNonEmptyString(options.renameTitleAfterStart);
    if (normalizeNonEmptyString(prompt) || renameTitle) {
      const renameCommand = renameTitle
        ? `/${gpuiWorkspaceTerminalTitleCommandForAgent(agent.agentId)} ${renameTitle}`
        : undefined;
      await this.startLocalAgentSessionAndSendPrompt(
        project.projectId,
        sessionId,
        prompt,
        renameCommand,
      );
    }
    return {
      agentSessionId:
        normalizeNonEmptyString(session?.agentSessionId) ??
        normalizeNonEmptyString(session?.runtimeSettings?.agentSessionId),
      agentSessionPath:
        normalizeNonEmptyString(session?.agentSessionPath) ??
        normalizeNonEmptyString(session?.runtimeSettings?.agentSessionPath),
      projectId: project.projectId,
      sessionId,
      zmxName: normalizeNonEmptyString(session?.zmxName),
    };
  }

  private async createRemoteAgentSessionForProject(
    remoteScope: GpuiRemoteProjectReference,
    agentId: string,
    prompt: string,
    title: string,
  ): Promise<void> {
    const response = await this.requestRemoteGxserver<GpuiGxserverCreatedSessionResult>(
      remoteScope.machineId,
      "/api/createAgentSession",
      {
        agentId,
        projectId: remoteScope.projectId,
        requireLaunchCommand: true,
        runtimeSettings: this.createFirstPromptTitleRuntimeSettings(prompt),
        surface: "workspace",
        title,
      },
      { timeoutMs: 20_000 },
    );
    const sessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (sessionId) {
      const projectId =
        normalizeNonEmptyString(response.session?.projectId) ?? remoteScope.projectId;
      await this.startRemoteAgentSessionAndSendPrompt(
        remoteScope.machineId,
        projectId,
        sessionId,
        prompt,
      ).catch(() => {
        this.postRemoteToast("warning", "Remote agent prompt failed", {
          description:
            "The remote gxserver could not start that agent session or deliver its prompt.",
        });
      });
      this.setRemotePresentationSessionFocus({
        machineId: remoteScope.machineId,
        projectId,
        sessionId,
      });
    }
    await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
  }

  private async resolveUniqueWorktreeTarget(
    project: GxserverProjectDomainState,
    prompt: string,
  ): Promise<{ branch: string; name: string; path: string }> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    const sourcePath = normalizeGpuiProjectPath(project.path);
    if (!sourcePath) {
      throw new Error("Project has no registered path.");
    }
    const parentDirectory = gpuiDirname(sourcePath);
    const projectFolderName = gpuiProjectNameFromPath(sourcePath);
    const baseSlug = gpuiWorktreeSlugFromPrompt(prompt);
    const registeredPaths = new Set(
      this.domainProjects
        .map((candidate) => normalizeGpuiProjectPath(candidate.path))
        .filter((path): path is string => Boolean(path)),
    );
    for (let index = 0; index < 50; index += 1) {
      const name = index === 0 ? baseSlug : `${baseSlug}-${index + 1}`;
      const branch = name;
      const path = `${parentDirectory}/${projectFolderName}-${name}`;
      const [branchCheck, pathCheck] = await Promise.all([
        this.client.rpc<GxserverTypedOperationResult>("/api/runGitAction", {
          action: "verifyRef",
          projectId: project.projectId,
          ref: `refs/heads/${branch}`,
        }),
        this.client.rpc<GxserverTypedOperationResult>("/api/runWorktreeAction", {
          action: "pathExists",
          projectId: project.projectId,
          worktreePath: path,
        }),
      ]);
      if (branchCheck.exitCode !== 0 && pathCheck.exitCode !== 0 && !registeredPaths.has(path)) {
        return { branch, name, path };
      }
    }
    throw new Error("Could not find an unused worktree name.");
  }

  private async saveSidebarAgent(
    message: Extract<SidebarToExtensionMessage, { type: "saveSidebarAgent" }>,
  ): Promise<void> {
    const name = message.name.trim();
    const command = message.command.trim();
    if (!name || !command || !this.client || this.domainProjects.length === 0) {
      return;
    }
    await this.mutateSidebarHudSettings({
      acceptAllMode: message.acceptAllMode,
      activeProjectId: this.activeProjectId,
      agentId: message.agentId,
      command,
      icon: message.icon,
      name,
      operation: "save",
      target: "agent",
    });
  }

  private async deleteSidebarAgent(agentId: string): Promise<void> {
    if (!this.client || this.domainProjects.length === 0) {
      return;
    }
    await this.mutateSidebarHudSettings({
      activeProjectId: this.activeProjectId,
      agentId,
      operation: "delete",
      target: "agent",
    });
  }

  private async syncSidebarAgentOrder(
    requestId: string,
    agentIds: readonly string[],
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const result = await this.mutateSidebarHudSettings({
      activeProjectId: this.activeProjectId,
      agentIds,
      operation: "order",
      target: "agent",
    });
    this.messageSource.postMessage({
      itemIds: result?.itemIds ?? [],
      kind: "agent",
      requestId,
      status: "success",
      type: "sidebarOrderSyncResult",
    });
  }

  private async saveSidebarCommand(
    message: Extract<SidebarToExtensionMessage, { type: "saveSidebarCommand" }>,
  ): Promise<void> {
    const project = this.activeDomainProject();
    if (!project || !this.client) {
      return;
    }
    const name = message.name.trim();
    const command = message.command?.trim();
    const url = message.url?.trim();
    if (!name && !message.icon) {
      return;
    }
    if (message.actionType === "browser" && !url) {
      return;
    }
    if (message.actionType === "terminal" && !command) {
      return;
    }
    await this.mutateSidebarHudSettings({
      actionType: message.actionType,
      activeProjectId: project.projectId,
      closeTerminalOnExit: message.actionType === "terminal" ? message.closeTerminalOnExit : false,
      command,
      commandId: message.commandId,
      icon: message.icon,
      links:
        message.actionType === "terminal" ? normalizeSidebarCommandLinks(message.links) : undefined,
      name,
      playCompletionSound: message.actionType === "terminal" ? message.playCompletionSound : false,
      operation: "save",
      showOnProjectRow: message.showOnProjectRow,
      target: "command",
      url,
    });
  }

  private async deleteSidebarCommand(commandId: string): Promise<void> {
    const project = this.activeDomainProject();
    if (!project || !this.client) {
      return;
    }
    await this.mutateSidebarHudSettings({
      activeProjectId: project.projectId,
      commandId,
      operation: "delete",
      target: "command",
    });
  }

  private async syncSidebarCommandOrder(
    requestId: string,
    commandIds: readonly string[],
  ): Promise<void> {
    const project = this.activeDomainProject();
    if (!project || !this.client) {
      return;
    }
    const result = await this.mutateSidebarHudSettings({
      activeProjectId: project.projectId,
      commandIds,
      operation: "order",
      target: "command",
    });
    this.messageSource.postMessage({
      itemIds: result?.itemIds ?? [],
      kind: "command",
      requestId,
      status: "success",
      type: "sidebarOrderSyncResult",
    });
  }

  /*
  CDXC:GlobalActions 2026-08-01:
  Global Action writes are not project writes: they carry no activeProjectId,
  and they do not require an active project to exist. A user with every project
  closed can still edit the actions that apply to all of them. Validation
  mirrors the project path so a save that gxserver would reject never leaves
  the renderer.
  */
  private async saveGlobalSidebarCommand(
    message: Extract<SidebarToExtensionMessage, { type: "saveGlobalSidebarCommand" }>,
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const name = message.name.trim();
    const command = message.command?.trim();
    const url = message.url?.trim();
    if (!name && !message.icon) {
      return;
    }
    if (message.actionType === "browser" && !url) {
      return;
    }
    if (message.actionType === "terminal" && !command) {
      return;
    }
    await this.mutateSidebarHudSettings({
      actionType: message.actionType,
      closeTerminalOnExit: message.actionType === "terminal" ? message.closeTerminalOnExit : false,
      command,
      commandId: message.commandId,
      icon: message.icon,
      links:
        message.actionType === "terminal" ? normalizeSidebarCommandLinks(message.links) : undefined,
      name,
      playCompletionSound: message.actionType === "terminal" ? message.playCompletionSound : false,
      operation: "save",
      /*
      CDXC:GlobalActions 2026-08-07:
      gxserver stores showOnProjectRow for both lists, so a global save that
      omits it writes the flag back as false and the Settings toggle never
      sticks. Forward it exactly like the project save above.
      */
      showOnProjectRow: message.showOnProjectRow,
      target: "globalCommand",
      url,
    });
  }

  private async deleteGlobalSidebarCommand(commandId: string): Promise<void> {
    if (!this.client) {
      return;
    }
    await this.mutateSidebarHudSettings({
      commandId,
      operation: "delete",
      target: "globalCommand",
    });
  }

  private async syncGlobalSidebarCommandOrder(
    requestId: string,
    commandIds: readonly string[],
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const result = await this.mutateSidebarHudSettings({
      commandIds,
      operation: "order",
      target: "globalCommand",
    });
    this.messageSource.postMessage({
      itemIds: result?.itemIds ?? [],
      kind: "command",
      requestId,
      status: "success",
      type: "sidebarOrderSyncResult",
    });
  }

  private pickWorkspaceFolder(originalMessage: SidebarToExtensionMessage): void {
    try {
      postAppModalHostMessage(
        { type: "pickWorkspaceFolder" },
        "GPUISidebarWorkspaceProjects:pickWorkspaceFolder",
      );
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
    }
  }

  private async handleGpuiWorkspaceFolderPicked(payload: unknown): Promise<void> {
    const replacement = normalizeGpuiReplacementProjectFolderPick(payload);
    if (replacement) {
      await this.relocateProjectFolder(replacement.projectId, replacement.path);
      return;
    }
    const pick = normalizeGpuiWorkspaceFolderPick(payload);
    if (!pick) {
      return;
    }
    if (!this.client) {
      this.postSidebarActionToast("error", "Add Project failed", {
        description: "gxserver is not connected.",
      });
      return;
    }
    try {
      const response = await this.client.rpc<{ project?: GxserverProjectDomainState }>(
        "/api/addProjectPath",
        pick.name ? { name: pick.name, path: pick.path } : { path: pick.path },
      );
      const project = response.project;
      if (!project) {
        throw new Error("gxserver did not return the added project.");
      }
      this.upsertDomainProject(project);
      this.focusProjectId(project.projectId);
      await this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => {
        this.publishHudPatch();
      });
    } catch {
      this.postSidebarActionToast("error", "Add Project failed", {
        description: "Ghostex could not add the selected folder.",
      });
    }
  }

  private ensureLocalProjectPathAvailable(projectId: string): boolean {
    const group = this.latestGroups.find(
      (candidate) =>
        candidate.remoteMachineContext === undefined &&
        candidate.projectContext?.editor.projectId === projectId,
    );
    const state = group?.projectContext?.pathState;
    if (state === undefined || state === "available") {
      return true;
    }
    this.presentMissingProjectFolder(projectId);
    return false;
  }

  private presentMissingProjectFolder(projectId: string): boolean {
    const group = this.latestGroups.find(
      (candidate) =>
        candidate.remoteMachineContext === undefined &&
        candidate.projectContext?.editor.projectId === projectId,
    );
    const projectPath = normalizeNonEmptyString(group?.projectContext?.path);
    if (!group || !projectPath) {
      this.postSidebarActionToast("warning", "Project folder unavailable", {
        description: "Ghostex could not resolve this project's saved folder.",
      });
      return false;
    }
    openAppModal({
      modal: "missingProjectFolder",
      projectId,
      projectName: group.title,
      projectPath,
      type: "open",
    });
    return true;
  }

  private async relocateProjectFolder(projectId: string, path: string): Promise<void> {
    if (!this.client) {
      this.postSidebarActionToast("error", "Could not update project folder", {
        description: "gxserver is not connected.",
      });
      return;
    }
    try {
      const response = await this.client.rpc<{ project: GxserverProjectDomainState }>(
        "/api/relocateProject",
        { path, projectId },
      );
      this.upsertDomainProject(response.project);
      await this.refreshDomainPresentationSnapshotFromClient("patch");
      postAppModalHostMessage({ type: "close" }, "GPUIMissingProjectFolder:resolved");
      this.postSidebarActionToast("info", "Project folder updated");
    } catch (error) {
      this.postSidebarActionToast("error", "Could not update project folder", {
        description:
          error instanceof Error ? error.message : "Ghostex could not use the selected folder.",
      });
    }
  }

  private postSidebarActionToast(
    level: AppToastLevel,
    title: string,
    options: { description?: string } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description),
        "GPUISidebarActions:toast",
      );
    } catch {
      // Toast-host availability must never gate the underlying action.
    }
  }

  private async removeProject(projectId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteReference) {
      await this.removeRemoteProject(remoteReference);
      return;
    }
    if (!this.client) {
      return;
    }
    await this.client.rpc("/api/removeProject", {
      projectId,
    });
  }

  private async restoreRecentProject(projectId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteReference) {
      await this.restoreRemoteRecentProject(remoteReference);
      return;
    }
    if (!this.client) {
      return;
    }
    const response = await this.client.rpc<{
      project?: GxserverProjectDomainState;
      recentProjects: GxserverRecentProjectDomainState[];
    }>("/api/restoreRecentProject", {
      projectId,
    });
    /*
    CDXC:GPUIRecentProjects 2026-06-25-19:22:
    Local Recent Project restore must mirror macOS by treating `/api/restoreRecentProject` as the authoritative recent-row mutation, activating the restored local project id, and applying a fresh gxserver presentation so the normal group returns promptly without synthesized drawer rows.
    */
    if (response.project) {
      this.upsertDomainProject(response.project);
    }
    this.recentProjects = [...response.recentProjects];
    this.focusProjectId(projectId);
    await this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => {
      this.publishHudPatch();
    });
  }

  private async removeRecentProject(projectId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteReference) {
      await this.removeRemoteRecentProject(remoteReference);
      return;
    }
    if (!this.client) {
      return;
    }
    const response = await this.client.rpc<{
      recentProjects: GxserverRecentProjectDomainState[];
    }>("/api/removeRecentProject", {
      projectId,
    });
    this.domainProjects = this.domainProjects.filter((project) => project.projectId !== projectId);
    this.recentProjects = [...response.recentProjects];
    this.publishHudPatch();
  }

  private async closeRemoteProjectForGroup(
    remoteScope: GpuiRemoteProjectScope,
    groupId: string,
  ): Promise<void> {
    /*
    CDXC:GPUIRemoteProjects 2026-06-27-19:37:
    Remote Recent Projects are client-app state, not local Mac gxserver state
    and not the remote daemon's shared project state. GPUI parks a
    machine-scoped row in its own CEF storage so macOS and GPUI can connect to
    and organize the same remote machine independently.
    */
    const presentation = this.remotePresentations.get(remoteScope.machineId);
    const recentProject: GxserverRecentProjectDomainState = {
      path: remoteScope.project.path ?? "",
      projectId: remoteScope.projectId as GxserverProjectId,
      recentClosedAt: new Date().toISOString(),
      sessionCount: presentation
        ? countGpuiRemotePresentationProjectSessions(presentation, remoteScope.projectId)
        : 0,
      title: remoteScope.project.title,
    };
    const previousProjects = this.remoteRecentProjectsByMachineId.get(remoteScope.machineId) ?? [];
    this.remoteRecentProjectsByMachineId.set(
      remoteScope.machineId,
      orderGpuiRecentProjects([
        recentProject,
        ...previousProjects.filter((project) => project.projectId !== remoteScope.projectId),
      ]),
    );
    writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
    if (this.activeGroupId === groupId) {
      this.activeGroupId = undefined;
    }
    this.publishRemotePresentationPatch();
  }

  private async restoreRemoteRecentProject(
    remoteReference: GpuiRemoteProjectReference,
  ): Promise<void> {
    this.remoteRecentProjectsByMachineId.set(
      remoteReference.machineId,
      (this.remoteRecentProjectsByMachineId.get(remoteReference.machineId) ?? []).filter(
        (project) => project.projectId !== remoteReference.projectId,
      ),
    );
    writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
    this.activeGroupId = createGpuiRemotePresentationGroupId(
      remoteReference.machineId,
      remoteReference.projectId,
    );
    if (!this.remotePresentations.has(remoteReference.machineId)) {
      this.reconnectRemoteMachine(remoteReference.machineId, false);
    }
    this.publishRemotePresentationPatch();
  }

  private async removeRemoteRecentProject(
    remoteReference: GpuiRemoteProjectReference,
  ): Promise<void> {
    this.remoteRecentProjectsByMachineId.set(
      remoteReference.machineId,
      (this.remoteRecentProjectsByMachineId.get(remoteReference.machineId) ?? []).filter(
        (project) => project.projectId !== remoteReference.projectId,
      ),
    );
    writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
    this.publishRemotePresentationPatch();
  }

  private async removeRemoteProject(remoteReference: GpuiRemoteProjectReference): Promise<void> {
    try {
      await this.requestRemoteGxserver(remoteReference.machineId, "/api/removeProject", {
        projectId: remoteReference.projectId,
      });
      this.removeRemotePresentationProject(remoteReference.machineId, remoteReference.projectId);
      this.remoteRecentProjectsByMachineId.set(
        remoteReference.machineId,
        (this.remoteRecentProjectsByMachineId.get(remoteReference.machineId) ?? []).filter(
          (project) => project.projectId !== remoteReference.projectId,
        ),
      );
      writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
      this.publishRemotePresentationPatch();
    } catch {
      this.postRemoteToast("warning", "Remote project removal failed", {
        description: "The remote gxserver could not remove that project.",
      });
    }
  }

  private async closeProjectForGroup(groupId: string): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({ groupId });
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      if (!remoteScope) {
        this.postRemoteToast("warning", "Remote project close unavailable", {
          description: "Reconnect the remote machine before closing the project.",
        });
        return;
      }
      await this.closeRemoteProjectForGroup(remoteScope, groupId);
      return;
    }
    if (!this.client) {
      return;
    }
    const projectId = this.resolveProjectIdForGroup(groupId);
    if (!projectId) {
      return;
    }
    /*
    CDXC:GPUIRecentProjects 2026-06-24-12:38:
    GPUI reuses SidebarApp's macOS close/remove split. Close must call the gxserver park endpoint with the project id resolved from the live presentation group, then consume gxserver's authoritative parked row; never synthesize a Recent Project row or map Close to hard delete when resolution or the daemon mutation fails.
    */
    const response = await this.client.rpc<{
      project: GxserverProjectDomainState;
      recentProjects: GxserverRecentProjectDomainState[];
    }>("/api/closeProjectToRecent", {
      projectId,
    });
    this.upsertDomainProject(response.project);
    this.recentProjects = [...response.recentProjects];
    if (this.activeGroupId === groupId || this.activeProjectId === projectId) {
      this.activeGroupId = undefined;
      this.activeProjectId = undefined;
    }
    this.removeLocalPresentationProject(projectId);
    if (this.presentation) {
      this.publishPresentation("patch");
      return;
    }
    this.publishHudPatch();
  }

  private async removeProjectForGroup(groupId: string): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({ groupId });
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      if (!remoteScope) {
        this.postRemoteToast("warning", "Remote project removal unavailable", {
          description: "Reconnect the remote machine before removing the project.",
        });
        return;
      }
      await this.removeRemoteProject(remoteScope);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (projectId) {
      await this.removeProject(projectId);
    }
  }

  private resolveProjectIdForGroup(groupId: string): string | undefined {
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      return undefined;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId) {
      return undefined;
    }
    const group = this.latestGroups.find((candidate) => candidate.groupId === groupId);
    if (group?.projectContext) {
      return projectId;
    }
    return undefined;
  }

  private postProjectPathActionForGroup(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      "copyWorkspaceProjectPath" | "openWorkspaceProjectInFinder" | "openWorkspaceProjectInIde"
    >,
    groupId: string,
    originalMessage: SidebarToExtensionMessage,
  ): void {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      if (action === "copyWorkspaceProjectPath") {
        this.postRemoteProjectNativeAction("copyRemoteProjectPath", remoteGroup, originalMessage);
        return;
      }
      if (action === "openWorkspaceProjectInIde") {
        this.postRemoteProjectNativeAction(
          "openRemoteWorkspaceProjectInIde",
          remoteGroup,
          originalMessage,
        );
        return;
      }
      this.postRemoteToast("warning", "Remote project open unavailable", {
        description: "GPUI does not open remote project paths in local Finder.",
      });
      return;
    }
    const projectId = this.resolveProjectIdForGroup(groupId);
    if (!projectId) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    this.postNativeProjectPathAction(action, projectId, originalMessage);
  }

  private postActiveProjectPathAction(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      | "openActiveWorkspaceProjectInFinder"
      | "openActiveWorkspaceProjectInVscode"
      | "openActiveWorkspaceProjectInZed"
    >,
    originalMessage: SidebarToExtensionMessage,
  ): void {
    const remoteGroup = this.activeGroupId
      ? parseGpuiRemotePresentationGroupId(this.activeGroupId)
      : undefined;
    if (remoteGroup) {
      if (action === "openActiveWorkspaceProjectInVscode") {
        this.postRemoteProjectNativeAction(
          "openRemoteWorkspaceProjectInVscode",
          remoteGroup,
          originalMessage,
        );
        return;
      }
      if (action === "openActiveWorkspaceProjectInZed") {
        this.postRemoteProjectNativeAction(
          "openRemoteWorkspaceProjectInZed",
          remoteGroup,
          originalMessage,
        );
        return;
      }
      this.postRemoteToast("warning", "Remote project open unavailable", {
        description:
          action === "openActiveWorkspaceProjectInFinder"
            ? "GPUI does not open remote project paths in local Finder."
            : "That editor is not supported for GPUI remote project opens.",
      });
      return;
    }
    const projectId = this.activeProjectId;
    if (!projectId || !this.domainProjectById(projectId)) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    this.postNativeProjectPathAction(action, projectId, originalMessage);
  }

  private selectRemoteGroupAttachTarget(
    reference: GpuiRemoteProjectReference,
  ): { machineId: string; projectId: string; sessionId: string } | undefined {
    const presentation = this.remotePresentations.get(reference.machineId);
    const session = (presentation?.sessions ?? [])
      .filter(
        (candidate) =>
          candidate.projectId === reference.projectId &&
          (candidate.kind === "terminal" || candidate.kind === "agent"),
      )
      .sort(compareGpuiRemoteAttachCandidateSessions)[0];
    return session
      ? {
          machineId: reference.machineId,
          projectId: reference.projectId,
          sessionId: session.sessionId,
        }
      : undefined;
  }

  private postRemoteSessionNativeAction(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      "openRemoteSessionTerminal" | "copyRemoteAttachCommand" | "copyRemoteResumeCommand"
    >,
    reference: { machineId: string; projectId: string; sessionId: string },
    originalMessage: SidebarToExtensionMessage,
  ): boolean {
    return this.postNativeProjectPathAction(
      action,
      createGpuiRemotePresentationSessionId(
        reference.machineId,
        reference.projectId,
        reference.sessionId,
      ),
      originalMessage,
    );
  }

  private postRemoteProjectNativeAction(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      | "copyRemoteProjectPath"
      | "copyRemoteProjectOpenFolderCommand"
      | "openRemoteWorkspaceProjectInIde"
      | "openRemoteWorkspaceProjectInVscode"
      | "openRemoteWorkspaceProjectInZed"
      | "openRemoteExistingPullRequestInBrowser"
      | "openRemoteSidebarGitChangedFileInIde"
      | "openRemoteProjectPortsBrowser"
    >,
    reference: GpuiRemoteProjectReference,
    originalMessage: SidebarToExtensionMessage,
    options: { filePath?: string } = {},
  ): boolean {
    return this.postNativeProjectPathAction(
      action,
      createGpuiRemotePresentationProjectId(reference.machineId, reference.projectId),
      originalMessage,
      options,
    );
  }

  private postNativeProjectPathAction(
    action: GpuiSidebarNativeProjectPathAction,
    projectId: string,
    originalMessage: SidebarToExtensionMessage,
    options: { filePath?: string } = {},
  ): boolean {
    const normalizedProjectId = projectId.trim();
    if (!normalizedProjectId) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const bridge = window.ghostexGpui?.postNativeProjectPathAction;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const payload = JSON.stringify({
      action,
      ...(options.filePath ? { filePath: options.filePath } : {}),
      projectId: normalizedProjectId,
      type: GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(originalMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
  }

  private postSidebarCommandAction(
    command: SidebarCommandButton,
    selectionMessage: Extract<SidebarToExtensionMessage, { type: "runSidebarCommand" }>,
  ): boolean {
    const bridge = window.ghostexGpui?.postSidebarCommandAction;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(selectionMessage);
      return false;
    }
    const payload = JSON.stringify({
      actionType: command.actionType,
      commandId: command.commandId,
      name: command.name,
      /*
      CDXC:GPUICommandPane 2026-06-27-07:54:
      `runSidebarCommand` reaches the launch bridge only after GPUI rebuilds it as a selector-shaped object. Forward an own, validated runMode only for terminal Actions so Rust can create the visible debug workspace terminal like macOS while all other launch metadata stays resolved from the trusted HUD command.
      */
      ...(command.actionType === "terminal" &&
      selectionMessage.runMode &&
      isSidebarCommandRunMode(selectionMessage.runMode)
        ? { runMode: selectionMessage.runMode }
        : {}),
      ...(command.actionType === "terminal"
        ? {
            /*
            CDXC:GPUICommandPane 2026-06-27-07:54:
            GPUI command-pane Action launches must match native `runNativeSidebarCommand`: default command-pane runtime forces terminal close-on-exit off even when trusted saved/HUD Action definitions preserve older close-on-exit metadata. Renderer `runSidebarCommand` messages cannot supply this field, and Browser Actions must continue omitting the terminal-only boolean.
            */
            closeTerminalOnExit: false,
            playCompletionSound: command.playCompletionSound,
          }
        : {}),
      ...(command.actionType === "terminal" && command.command ? { command: command.command } : {}),
      ...(command.actionType === "terminal" && command.links && command.links.length > 0
        ? { links: command.links.map((link) => ({ target: link.target, url: link.url })) }
        : {}),
      ...(command.actionType === "browser" && command.url ? { url: command.url } : {}),
      type: GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION,
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(selectionMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(selectionMessage);
      return false;
    }
  }

  private postGhostexHotkeyAction(
    originalMessage: Extract<SidebarToExtensionMessage, { type: "runGhostexHotkeyAction" }>,
  ): boolean {
    const bridge = window.ghostexGpui?.postGhostexHotkeyAction;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    /*
    CDXC:GPUICommandPalette 2026-06-27-08:11:
    Shared SidebarApp and Command Palette hotkey rows emit `runGhostexHotkeyAction` through the reused GPUI runtime, not directly to Rust. Forward only the fixed action-id selector so Open Commands Panel, focused-pane routes, Settings, and modal hotkeys share Rust's native dispatcher without renderer-owned session ids, paths, command text, URLs, or launch metadata.
    */
    if (
      Object.keys(originalMessage).some((key) => key !== "type" && key !== "actionId") ||
      typeof originalMessage.actionId !== "string" ||
      originalMessage.actionId.trim() === ""
    ) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const payload = JSON.stringify({
      actionId: originalMessage.actionId,
      type: "runGhostexHotkeyAction",
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(originalMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
  }

  private postSidebarCommandRunEnd(
    commandId: string,
    originalMessage: SidebarToExtensionMessage,
  ): boolean {
    const bridge = window.ghostexGpui?.postSidebarCommandRunEnd;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const normalizedCommandId = commandId.trim();
    if (!normalizedCommandId) {
      return false;
    }
    const payload = JSON.stringify({
      commandId: normalizedCommandId,
      /*
      CDXC:GPUICommandPane 2026-06-27-05:59:
      `endSidebarCommandRun` is a separate fixed GPUI bridge from Action launch because Rust only needs the selected command id to close the mapped command-pane run. Rebuild the payload here so renderer command text, URLs, close-on-exit flags, cwd/env, paths, logs, output, status-file paths, and run ids never cross the run-end bridge.
      */
      type: GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION,
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(originalMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
  }

  private focusProjectId(projectId: string): void {
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    if (!normalizedProjectId) {
      return;
    }
    this.activeProjectId = normalizedProjectId;
    this.activeGroupId = this.isGpuiPresentationChatProjectId(normalizedProjectId)
      ? GPUI_GXSERVER_CHATS_GROUP_ID
      : createGxserverPresentationProjectGroupId(normalizedProjectId);
    this.refreshSidebarHudFromClient();
  }

  private setLocalPresentationSessionFocus(
    projectId: string,
    sessionId: string,
    targetGroupId?: string,
    exactVisibleSessionIds?: readonly string[],
  ): void {
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedProjectId || !normalizedSessionId) {
      return;
    }
    this.activeProjectId = normalizedProjectId;
    this.activeGroupId =
      targetGroupId ??
      (this.isGpuiPresentationChatProjectId(normalizedProjectId)
        ? GPUI_GXSERVER_CHATS_GROUP_ID
        : createGxserverPresentationProjectGroupId(normalizedProjectId));
    this.refreshSidebarHudFromClient();
    this.focusedSessionId = normalizedSessionId;
    this.visibleSessionIds = exactVisibleSessionIds
      ? new Set(exactVisibleSessionIds)
      : this.nextVisibleSessionIdsForLocalFocus(normalizedProjectId, normalizedSessionId);
    this.postGxserverPresentationFocusState();
  }

  private nextVisibleSessionIdsForLocalFocus(projectId: string, sessionId: string): Set<string> {
    /*
    CDXC:GPUISidebarSessionFocus 2026-06-26-04:42:
    GPUI local session focus should follow the macOS sidebar rule that a click selects the target within the current visible workspace projection instead of replacing all visible ownership with a singleton. Preserve live local visible ids and remote ids, materialize the current project's projected visible row, then add the clicked session so last-activity resorting cannot make a second session steal focus back.
    */
    const liveLocalSessionIds = new Set(
      (this.presentation?.sessions ?? []).map((session) => session.sessionId),
    );
    const nextVisibleSessionIds = new Set(
      [...this.visibleSessionIds].filter(
        (visibleSessionId) =>
          parseGpuiRemotePresentationSessionId(visibleSessionId) ||
          liveLocalSessionIds.has(visibleSessionId),
      ),
    );
    const projectVisibleSessionIds = this.currentVisibleSessionIdsForLocalProject(projectId);
    for (const visibleSessionId of projectVisibleSessionIds) {
      nextVisibleSessionIds.add(visibleSessionId);
    }
    nextVisibleSessionIds.add(sessionId);
    return nextVisibleSessionIds;
  }

  private currentVisibleSessionIdsForLocalProject(projectId: string): string[] {
    const presentation = this.presentation;
    if (!presentation) {
      return [];
    }
    const sessions =
      createGxserverPresentationSessionsByProjectFromGroups({ presentation }).get(projectId) ?? [];
    return sessions.flatMap((session, index) =>
      this.visibleSessionIds.has(session.sessionId) || index === 0 ? [session.sessionId] : [],
    );
  }

  private isGpuiPresentationChatProjectId(projectId: string): boolean {
    return (
      isGpuiPresentationChatDomainProject(this.domainProjectById(projectId)) ||
      isGpuiPresentationChatProjectPath(
        this.presentation?.projects.find((project) => project.projectId === projectId)?.path,
      )
    );
  }

  private setRemotePresentationSessionFocus(reference: {
    machineId: string;
    projectId: string;
    sessionId: string;
  }): void {
    const machineId = normalizeNonEmptyString(reference.machineId);
    const projectId = normalizeNonEmptyString(reference.projectId);
    const sessionId = normalizeNonEmptyString(reference.sessionId);
    if (!machineId || !projectId || !sessionId) {
      return;
    }
    const scopedSessionId = createGpuiRemotePresentationSessionId(machineId, projectId, sessionId);
    const project = this.remotePresentations
      .get(machineId)
      ?.projects.find((candidate) => candidate.projectId === projectId);
    const scopedGroupId = createGpuiRemotePresentationGroupId(
      machineId,
      isGpuiPresentationChatProjectPath(project?.path)
        ? GPUI_GXSERVER_CHATS_GROUP_ID
        : projectId,
    );
    this.activeGroupId = scopedGroupId;
    this.focusedSessionId = scopedSessionId;
    this.visibleSessionIds = new Set([scopedSessionId]);
    this.postGxserverPresentationFocusState();
  }

  private dropLocalPresentationSessionFocus(): void {
    if (this.focusedSessionId && !parseGpuiRemotePresentationSessionId(this.focusedSessionId)) {
      this.focusedSessionId = undefined;
    }
    this.visibleSessionIds = new Set(
      [...this.visibleSessionIds].filter((sessionId) =>
        Boolean(parseGpuiRemotePresentationSessionId(sessionId)),
      ),
    );
  }

  private dropRemotePresentationSessionFocus(machineId: string): void {
    if (
      this.focusedSessionId &&
      parseGpuiRemotePresentationSessionId(this.focusedSessionId)?.machineId === machineId
    ) {
      this.focusedSessionId = undefined;
    }
    this.visibleSessionIds = new Set(
      [...this.visibleSessionIds].filter(
        (sessionId) => parseGpuiRemotePresentationSessionId(sessionId)?.machineId !== machineId,
      ),
    );
  }

  private activeDomainProject(): GxserverProjectDomainState | undefined {
    return this.activeProjectId
      ? this.domainProjectById(this.activeProjectId)
      : this.domainProjects.find(
          (project) =>
            project.isRecentProject !== true && !isGpuiPresentationQuickDomainProject(project),
        );
  }

  private domainProjectById(projectId: string): GxserverProjectDomainState | undefined {
    return this.domainProjects.find((project) => project.projectId === projectId);
  }

  private resolveDomainProjectScope(scope: {
    projectId?: string;
    projectPath?: string;
  }): GxserverProjectDomainState | undefined {
    if (scope.projectId) {
      const byId = this.domainProjectById(scope.projectId);
      if (byId) {
        return byId;
      }
    }
    const normalizedPath = normalizeGpuiProjectPath(scope.projectPath);
    if (!normalizedPath) {
      return undefined;
    }
    return this.domainProjects.find(
      (project) => normalizeGpuiProjectPath(project.path) === normalizedPath,
    );
  }

  private resolveWorktreeFamilyParentProject(
    project: GxserverProjectDomainState,
  ): GxserverProjectDomainState | undefined {
    const parentProjectId = normalizeGpuiWorktreeParentProjectId(project.worktree);
    return parentProjectId ? this.domainProjectById(parentProjectId) : project;
  }

  private isTrustedExistingWorktreePath(
    path: string,
    sourceProject: GxserverProjectDomainState,
    parentProject: GxserverProjectDomainState,
  ): boolean {
    const trusted = this.trustedExistingWorktreeList;
    return Boolean(
      trusted &&
      trusted.sourceProjectId === sourceProject.projectId &&
      trusted.parentProjectId === parentProject.projectId &&
      trusted.paths.has(path),
    );
  }

  private resolveSidebarAgent(agentId: string): SidebarAgentButton | undefined {
    const normalizedAgentId = agentId.trim();
    if (!normalizedAgentId) {
      return undefined;
    }
    const agents = this.sidebarHud
      ? ([...this.sidebarHud.agents] as SidebarAgentButton[])
      : createSidebarAgentButtons([], []);
    return agents.find((agent) => agent.agentId === normalizedAgentId);
  }

  /*
   * CDXC:GlobalActions 2026-08-01:
   * Scope selects the list exclusively rather than falling through from one to
   * the other. A tab strip click names a Global Action, so resolving it against
   * project commands — which a shared id would allow — would run something the
   * user did not click. Global ids are additionally barred from the reserved
   * built-in names at save time, so the two spaces cannot collide there either.
   */
  private resolveSidebarCommand(
    commandId: string,
    scope: SidebarCommandScope = "project",
  ): SidebarCommandButton | undefined {
    const normalizedCommandId = commandId.trim();
    if (!normalizedCommandId) {
      return undefined;
    }
    if (scope === "global") {
      const globalCommands = (this.sidebarHud?.globalCommands ?? []) as SidebarCommandButton[];
      return globalCommands.find((command) => command.commandId === normalizedCommandId);
    }
    const commands = this.sidebarHud
      ? ([...this.sidebarHud.commands] as SidebarCommandButton[])
      : createSidebarCommandButtons([], [], []);
    return commands.find((command) => command.commandId === normalizedCommandId);
  }

  /*
  CDXC:ProjectActions 2026-08-01:
  Project-row Action clicks resolve against the clicked project's own command
  list from the HUD's commandsByProject block, never the active project's list,
  so two projects with different Actions cannot cross-launch. A project id with
  no per-project entry only falls back to the flat active list when it IS the
  active project; otherwise the click is an unsupported no-op.
  */
  private resolveSidebarCommandForProject(
    commandId: string,
    projectId: string | undefined,
  ): SidebarCommandButton | undefined {
    if (!projectId) {
      return this.resolveSidebarCommand(commandId);
    }
    const normalizedCommandId = commandId.trim();
    if (!normalizedCommandId) {
      return undefined;
    }
    const projectCommands = this.sidebarHud?.commandsByProject?.[projectId];
    if (projectCommands) {
      return ([...projectCommands] as SidebarCommandButton[]).find(
        (command) => command.commandId === normalizedCommandId,
      );
    }
    if (projectId !== this.activeProjectId) {
      return undefined;
    }
    return this.resolveSidebarCommand(commandId);
  }

  private createSidebarCommandSelectionMessage(
    commandId: string,
    originalMessage: SidebarToExtensionMessage,
  ): Extract<SidebarToExtensionMessage, { type: "runSidebarCommand" }> | undefined {
    /*
    CDXC:GPUICommandPane 2026-06-27-07:54:
    The GPUI SidebarApp/Command Palette Action launch boundary accepts only selector-shaped `runSidebarCommand` objects: type, command id, and an own optional runMode. Renderer-supplied command text, URLs, cwd/env, paths, output, logs, run ids, and status fields are unsupported instead of being stripped into a launch.
    */
    if (
      Object.keys(originalMessage).some(
        (key) => !GPUI_SIDEBAR_COMMAND_SELECTOR_MESSAGE_KEYS.has(key),
      )
    ) {
      return undefined;
    }
    if (!Object.prototype.hasOwnProperty.call(originalMessage, "runMode")) {
      return {
        commandId,
        type: "runSidebarCommand",
      };
    }
    const runMode = (originalMessage as { runMode?: unknown }).runMode;
    if (!isSidebarCommandRunMode(runMode)) {
      return undefined;
    }
    return {
      commandId,
      runMode,
      type: "runSidebarCommand",
    };
  }

  private runSidebarCommand(
    commandId: string,
    originalMessage: SidebarToExtensionMessage,
    scope: SidebarCommandScope = "project",
  ): void {
    /*
     * CDXC:GPUICommandPane 2026-06-26-05:11:
     * The shared SidebarApp and Command Palette emit `runSidebarCommand` as an
     * Action-selection message: command id plus optional runMode. In GPUI,
     * resolve the selected Action from the live gxserver HUD projection and hand
     * trusted launch metadata to Rust through the fixed command-action bridge so
     * command text, URLs, saved close-on-exit metadata, paths, output, and logs
     * never come from the renderer message.
     *
     * CDXC:GPUICommandPane 2026-06-27-06:37:
     * Match native sidebar dispatch for stale Action selectors: an unknown command id is an unsupported no-op, while an existing but unconfigured Action still opens Settings so the user can supply the missing command or URL.
     *
     * CDXC:GPUICommandPane 2026-06-27-07:54:
     * Treat selector shape as part of the Action contract before looking up the HUD command. Extra launch/run-state fields are unsupported no-ops, not sanitized launches, while valid configured-but-empty selectors still reach Settings like macOS.
     *
     * CDXC:ProjectActions 2026-08-01:
     * Project-row Action buttons pass the row's group id. Resolve the Action
     * from that project's own command list and activate the project through
     * the existing focus flow before dispatching, so the launch bridge payload
     * stays project-blind and the command pane opens in the project the user
     * clicked — the same ordering as clicking the row and then the titlebar
     * action by hand.
     */
    const selectionMessage = this.createSidebarCommandSelectionMessage(commandId, originalMessage);
    if (!selectionMessage) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    /*
     * CDXC:GlobalActions 2026-08-01:
     * A global-scoped selector names an action that belongs to no project, so
     * it resolves against the global list. Project selectors keep the
     * per-project resolution above: the two scopes pick different lists rather
     * than falling through to each other.
     *
     * CDXC:GlobalActions 2026-08-07:
     * Scope and group id answer different questions, so a global selector may
     * carry one: the scope picks the list, the group id picks the project to
     * activate before dispatching. That is what makes a Global Action on a
     * project row run in the row the user clicked instead of whichever project
     * happened to be active. The tab strip still sends no group id — its
     * normalizer rejects the key — so it keeps running in the active project.
     */
    const groupId =
      originalMessage.type !== "runSidebarCommand"
        ? undefined
        : normalizeNonEmptyString(originalMessage.groupId ?? "");
    const targetProjectId = groupId
      ? parseGxserverPresentationProjectGroupId(groupId)
      : undefined;
    const command =
      scope === "global"
        ? this.resolveSidebarCommand(commandId, scope)
        : this.resolveSidebarCommandForProject(commandId, targetProjectId);
    if (!command) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    if (!isSidebarCommandConfigured(command)) {
      this.openAppModal("settings");
      return;
    }
    if (targetProjectId && targetProjectId !== this.activeProjectId) {
      this.focusProjectId(targetProjectId);
      /*
       * Publish a presentation patch rather than posting focus state and
       * active-project context directly: both read `latestGroups`, which only
       * `publishPresentation` refreshes, so posting them alone would leave the
       * sidebar's active-row highlight on the previous project until an
       * unrelated delta arrived. This matches every other project-switching
       * call site in this file.
       */
      this.publishPresentation("patch");
    }
    if (this.postSidebarCommandAction(command, selectionMessage)) {
      return;
    }
    this.handleUnsupportedSidebarMessage(selectionMessage);
  }

  private endSidebarCommandRun(
    commandId: string,
    originalMessage: SidebarToExtensionMessage,
  ): void {
    if (this.postSidebarCommandRunEnd(commandId, originalMessage)) {
      return;
    }
    this.handleUnsupportedSidebarMessage(originalMessage);
  }

  private async mutateSidebarHudSettings(
    params: GxserverSidebarHudSettingsMutationParams,
  ): Promise<GxserverSidebarHudSettingsMutationResult | undefined> {
    const client = this.client;
    if (!client) {
      return undefined;
    }
    /*
     * CDXC:SidebarHudSettingsMutation 2026-06-24-20:54:
     * GPUI SidebarApp forwards Settings agent/action save, delete, and order
     * intents to gxserver instead of normalizing custom project metadata in the
     * renderer. Apply the returned canonical project rows and HUD projection so
     * Settings rows and sidebar buttons refresh from the same daemon contract.
     */
    const response = await client.mutateSidebarHudSettings({
      ...params,
      /*
       * CDXC:ProjectActions 2026-08-01:
       * The mutation result replaces the whole HUD snapshot, so every settings
       * mutation must carry the per-project command block the sidebar rows
       * render from — otherwise an agent or action save would blank them
       * until the next full HUD poll.
       */
      includeAllProjectCommands: true,
    });
    if (this.client !== client) {
      return undefined;
    }
    for (const project of response.projects) {
      this.upsertDomainProject(project);
    }
    this.sidebarHud = response.hud;
    this.publishHudPatch();
    return response;
  }

  private async updateProjectDomainState(
    projectId: string,
    params: Record<string, unknown>,
  ): Promise<GxserverProjectDomainState | undefined> {
    if (!this.client) {
      return undefined;
    }
    const response = await this.client.rpc<{ project: GxserverProjectDomainState }>(
      "/api/updateProject",
      {
        ...params,
        projectId,
      },
    );
    this.upsertDomainProject(response.project);
    this.publishHudPatch();
    this.refreshSidebarHudFromClient();
    return response.project;
  }

  private upsertDomainProject(nextProject: GxserverProjectDomainState): void {
    const existingIndex = this.domainProjects.findIndex(
      (project) => project.projectId === nextProject.projectId,
    );
    this.domainProjects =
      existingIndex >= 0
        ? this.domainProjects.map((project, index) =>
            index === existingIndex ? nextProject : project,
          )
        : [...this.domainProjects, nextProject];
  }

  private async refreshDomainPresentationFromClient(
    kind: GpuiSidebarRuntimeSnapshotKind,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    const [snapshot, domainProjects, recentProjects] = await Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchProjectList(),
      client.fetchRecentProjects().catch(() => this.recentProjects),
    ]);
    if (this.client !== client) {
      return;
    }
    this.domainProjects = [...domainProjects];
    this.recentProjects = [...recentProjects];
    this.applyPresentationSnapshot(snapshot, kind);
  }

  private async refreshDomainPresentationSnapshotFromClient(
    kind: GpuiSidebarRuntimeSnapshotKind,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    const [snapshot, domainProjects] = await Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchProjectList(),
    ]);
    if (this.client !== client) {
      return;
    }
    this.domainProjects = [...domainProjects];
    this.applyPresentationSnapshot(snapshot, kind);
  }

  private postWorktreeToast(
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description, {
          persistent: options.persistent,
          toastId: options.toastId,
        }),
        "AppModals:gpuiWorktreeToast",
      );
    } catch {
      /*
      CDXC:GPUIWorktrees 2026-06-24-18:21:
      Worktree mutations should still run when the toast host is unavailable.
      The missing toast bridge is a presentation problem, while gxserver remains
      the production owner for Git, setup, Beads hook, and agent-session state.
      */
    }
  }

  private saveSidebarSettingsPatch(
    message: Extract<SidebarToExtensionMessage, { type: "updateSettingsPatch" }>,
  ): void {
    /*
    CDXC:SidebarV2 2026-07-29:
    Sidebar-origin settings writes (sidebar version, Group by Project, remote
    machine ordering) are real Settings saves, so they take the same route the
    Settings modal uses: the app-modal host bridge installed on the GPUI sidebar
    surface, where Rust merges the patch onto the stored snapshot and hydrates
    every surface back. Do not persist settings inside this adapter.
    */
    try {
      postAppModalHostMessage(
        { message, type: "sidebarCommand" },
        "GPUISidebarActions:updateSettingsPatch",
      );
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  }

  private openExternalUrl(
    message: Extract<SidebarToExtensionMessage, { type: "openExternalUrl" }>,
  ): void {
    /*
    CDXC:SidebarDiscord 2026-08-07:
    The shared sidebar's external links must enter the same native command
    route as Settings and first-launch links. The GPUI sidebar adapter used to
    drop openExternalUrl as unsupported after the React click had already
    closed the menu, so Join Discord appeared inert. Forward the typed command
    through the existing app-modal host bridge; Rust remains responsible for
    validating and opening the http/https URL.
    */
    try {
      postAppModalHostMessage(
        { message, type: "sidebarCommand" },
        "GPUISidebarActions:openExternalUrl",
      );
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  }

  private openAppModal(modal: "firstLaunchSetup" | "settings" | "watchGhostexVideo"): void {
    /*
    CDXC:GPUISidebarAppModalBridge 2026-06-24-11:40:
    Sidebar-origin Settings, first-launch welcome, and tutorial-video requests in GPUI must use the shared app-modal host bridge installed by the CEF sidebar surface. Do not fork Settings React UI, duplicate modal state, or route these first-party modals through fixture/sidebar-only alternate paths.
    */
    try {
      openAppModal({ modal, type: "open" });
    } catch {
      this.handleUnsupportedSidebarMessage({ type: "openSettings" });
    }
  }

  private async saveScratchPad(content: string): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    this.appUserData = await client.saveScratchPad(content);
    this.publishAppUserDataHydrate();
  }

  private async savePinnedPrompt(
    message: Extract<SidebarToExtensionMessage, { type: "savePinnedPrompt" }>,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    this.appUserData = await client.savePinnedPrompt({
      content: message.content,
      promptId: message.promptId,
      title: message.title,
    });
    this.publishAppUserDataHydrate();
  }

  private publishAppUserDataHydrate(): void {
    if (!this.hasHydrated) {
      return;
    }
    this.messageSource.postMessage(this.createHydrateMessage(this.latestGroups, this.latestHud));
  }

  private patchPresentationSession(
    projectId: string,
    sessionId: string,
    patch: Partial<GxserverPresentationSnapshot["sessions"][number]>,
  ): void {
    const presentation = this.presentation;
    const session = presentation?.sessions.find(
      (candidate) => candidate.projectId === projectId && candidate.sessionId === sessionId,
    );
    if (!presentation || !session) {
      return;
    }
    this.presentation = reduceGxserverPresentationDelta(
      presentation,
      {
        session: {
          ...session,
          ...patch,
        },
        type: "sessionUpdated",
      },
      presentation.revision + 1,
    );
    this.publishPresentation("patch");
  }

  private removePresentationSession(projectId: string, sessionId: string): void {
    this.hideLocalPresentationSession(projectId, sessionId);
    const presentation = this.presentation;
    if (!presentation) {
      return;
    }
    this.presentation = reduceGxserverPresentationDelta(
      presentation,
      {
        projectId: projectId as GxserverProjectId,
        sessionId: sessionId as GxserverSessionId,
        type: "sessionRemoved",
      },
      presentation.revision + 1,
    );
    this.publishPresentation("patch");
  }

  private hideLocalPresentationSession(projectId: string, sessionId: string): void {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:59:
    GPUI native tab close must match macOS local-first sidebar removal. Keep a runtime-only hidden-session overlay so future gxserver hydrates cannot reinsert a locally closed mapped Agents row while the backend transition catches up or fails best-effort. Store only project/session ids.
    */
    this.localFirstHiddenPresentationSessionKeys.add(
      createGxserverPresentationSidebarSessionKey(projectId, sessionId),
    );
  }

  private removeLocalPresentationProject(projectId: string): void {
    const presentation = this.presentation;
    if (!presentation) {
      return;
    }
    /*
    CDXC:GPUIRecentProjects 2026-06-25-18:50:
    Local close-to-recent must immediately mirror macOS by removing the parked project from normal GPUI sidebar groups while using gxserver's `/api/closeProjectToRecent` recent-project response as the only drawer source.
    */
    this.presentation = reduceGxserverPresentationDelta(
      presentation,
      {
        projectId: projectId as GxserverProjectId,
        type: "projectRemoved",
      },
      presentation.revision + 1,
    );
  }

  private handleUnsupportedSidebarMessage(_message: SidebarToExtensionMessage): void {
    /*
    CDXC:GPUISidebarGxserverRuntime 2026-06-24-11:00:
    GPUI command parity is intentionally incremental. Unsupported SidebarApp messages must be explicit no-ops in this adapter instead of mutating fixture state, inventing host behavior, logging user content, or pretending native-only Browser/Git/settings/chrome actions succeeded.
    */
  }
}

type GpuiRendererCommandHandler = (
  command: GxserverRendererCommand,
) => Promise<Record<string, unknown> | void> | Record<string, unknown> | void;

class GpuiGxserverClient {
  constructor(private readonly bootstrap: GpuiValidatedGxserverBootstrap) {}

  async fetchPresentationSnapshot(): Promise<GxserverPresentationSnapshot> {
    const { snapshot } = await this.rpc<{ snapshot: GxserverPresentationSnapshot }>(
      "/api/readPresentationSnapshot",
    );
    return snapshot;
  }

  async fetchProjectList(): Promise<GxserverProjectDomainState[]> {
    const { projects } = await this.rpc<{ projects: GxserverProjectDomainState[] }>(
      "/api/listProjects",
    );
    return projects;
  }

  async fetchRecentProjects(): Promise<GxserverRecentProjectDomainState[]> {
    const { recentProjects } = await this.rpc<{
      recentProjects: GxserverRecentProjectDomainState[];
    }>("/api/listRecentProjects");
    return recentProjects;
  }

  async fetchSidebarHud(activeProjectId: string | undefined): Promise<GxserverSidebarHudResponse> {
    const normalizedActiveProjectId = activeProjectId?.trim();
    /*
     * CDXC:ProjectActions 2026-08-01:
     * The GPUI sidebar renders showOnProjectRow quick actions on every project
     * row, so the HUD read always asks for the per-project command block.
     */
    return this.rpc<GxserverSidebarHudResponse>("/api/readSidebarHud", {
      includeAllProjectCommands: true,
      ...(normalizedActiveProjectId ? { activeProjectId: normalizedActiveProjectId } : {}),
    });
  }

  async mutateSidebarHudSettings(
    params: GxserverSidebarHudSettingsMutationParams,
  ): Promise<GxserverSidebarHudSettingsMutationResult> {
    return this.rpc<GxserverSidebarHudSettingsMutationResult>(
      "/api/mutateSidebarHudSettings",
      params,
    );
  }

  async fetchWorkspaceSessionGroups(): Promise<unknown> {
    const { groups } = await this.rpc<{ groups?: unknown }>("/api/readWorkspaceSessionGroups");
    return groups;
  }

  async updateWorkspaceSessionGroups(state: GpuiWorkspaceSessionGroupsState): Promise<void> {
    await this.rpc("/api/updateWorkspaceSessionGroups", { state });
  }

  async updateSidebarProjectCollections(
    state: GxserverSidebarProjectCollectionsState,
  ): Promise<unknown> {
    const { sidebarProjectCollections } = await this.rpc<{
      sidebarProjectCollections?: unknown;
    }>("/api/updateSidebarProjectCollections", { state });
    return sidebarProjectCollections;
  }

  async fetchAppUserData(): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>("/api/readAppUserData");
  }

  async saveScratchPad(content: string): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>("/api/saveScratchPad", { content });
  }

  async savePinnedPrompt(params: {
    content: string;
    promptId?: string;
    title: string;
  }): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>("/api/savePinnedPrompt", params);
  }

  async rpc<TResult>(
    path: GxserverEndpointPath,
    params: Record<string, unknown> = {},
  ): Promise<TResult> {
    const response = await fetch(`${this.bootstrap.baseUrl}${path}`, {
      body: JSON.stringify({
        params,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      }),
      headers: {
        authorization: `Bearer ${this.bootstrap.authToken}`,
        "content-type": "application/json",
        "x-gxserver-protocol-version": String(GXSERVER_PROTOCOL_VERSION),
      },
      method: "POST",
    });
    const body = await readJson(response);
    if (!response.ok || !isGxserverRpcSuccess<TResult>(body)) {
      const errorMessage = gpuiGxserverRpcErrorMessage(body);
      throw new GpuiGxserverRpcError(
        errorMessage ??
          `gxserver rejected ${path} (${response.status > 0 ? response.status : "no response"}).`,
        gpuiGxserverRpcErrorCode(body),
      );
    }
    if (body.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
      throw new Error("gxserver protocol mismatch.");
    }
    return body.result;
  }

  subscribePresentation({
    clientId,
    lastRevision,
    onClose,
    onDelta,
    onError,
    onGlobalSidebarCommands,
    onRendererCommand,
    onSessionChatEvent,
    onSidebarProjectCollections,
    onSnapshot,
  }: {
    clientId: string;
    lastRevision: number;
    onClose: () => void;
    onDelta: (delta: GxserverPresentationDelta, revision: number) => void;
    onError: () => void;
    onGlobalSidebarCommands?: () => void;
    onRendererCommand?: GpuiRendererCommandHandler;
    onSessionChatEvent?: (event: GxserverSessionChatEvent) => void;
    onSidebarProjectCollections?: (state: GxserverSidebarProjectCollectionsState) => void;
    onSnapshot: (snapshot: GxserverPresentationSnapshot) => void;
  }): GpuiPresentationSubscription {
    const url = new URL(`${this.bootstrap.baseUrl}/api/events`);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("protocolVersion", String(GXSERVER_PROTOCOL_VERSION));
    url.searchParams.set("authToken", this.bootstrap.authToken);

    const socket = new WebSocket(url.toString());
    let closedByClient = false;
    socket.addEventListener("open", () => {
      socket.send(
        JSON.stringify({
          clientId,
          lastRevision,
          ...(onRendererCommand ? { rendererCommands: true } : {}),
          type: "subscribePresentation",
        }),
      );
    });
    socket.addEventListener("message", (event) => {
      const message = parseObject(event.data);
      if (!message) {
        return;
      }
      if (message.type === "presentationSnapshot" && isPresentationSnapshot(message.snapshot)) {
        onSnapshot(message.snapshot);
        return;
      }
      if (
        message.type === "presentationDelta" &&
        typeof message.revision === "number" &&
        isPresentationDelta(message.delta)
      ) {
        onDelta(message.delta, message.revision);
        return;
      }
      if (
        message.type === "rendererCommand" &&
        onRendererCommand &&
        isGpuiRendererCommand(message.command)
      ) {
        void handleGpuiRendererCommand(socket, message.command, onRendererCommand);
        return;
      }
      if (
        message.type === "sidebarProjectCollectionsChanged" &&
        onSidebarProjectCollections &&
        isSidebarProjectCollectionsState(message.sidebarProjectCollections)
      ) {
        onSidebarProjectCollections(message.sidebarProjectCollections);
        return;
      }
      /*
      CDXC:GlobalActions 2026-08-07:
      The Global Actions announcement carries no list — the handler refetches
      the HUD, which is the one projection of it — so there is no payload to
      shape-validate before forwarding it.
      */
      if (message.type === "globalSidebarCommandsChanged" && onGlobalSidebarCommands) {
        onGlobalSidebarCommands();
        return;
      }
      /*
      CDXC:SessionChatCore 2026-07-31:
      Session-chat frames ride the same local /api/events socket as
      presentation. The runtime only forwards shape-validated frames to an
      opted-in handler; the gpui chat CEF surface owns its own subscription,
      so this branch exists for parity with the shared native client switch
      and stays inert unless a handler is provided.
      */
      if (
        typeof message.type === "string" &&
        isSessionChatEventType(message.type) &&
        onSessionChatEvent &&
        isGpuiSessionChatEventMessage(message)
      ) {
        onSessionChatEvent(message);
      }
    });
    socket.addEventListener("error", () => {
      onError();
    });
    socket.addEventListener("close", () => {
      if (!closedByClient) {
        onClose();
      }
    });
    return {
      close: () => {
        closedByClient = true;
        socket.close();
      },
    };
  }
}

type GpuiPresentationSubscription = {
  close: () => void;
};

function validateGpuiGxserverBootstrap(
  bootstrap: GpuiGxserverBootstrap,
): GpuiValidatedGxserverBootstrap | undefined {
  if (
    bootstrap.protocolVersion !== undefined &&
    bootstrap.protocolVersion !== GXSERVER_PROTOCOL_VERSION
  ) {
    return undefined;
  }
  if (typeof bootstrap.baseUrl !== "string" || bootstrap.baseUrl.trim().length === 0) {
    return undefined;
  }
  if (typeof bootstrap.authToken !== "string" || bootstrap.authToken.trim().length === 0) {
    return undefined;
  }
  try {
    const baseUrl = new URL(bootstrap.baseUrl);
    return {
      authToken: bootstrap.authToken,
      baseUrl: baseUrl.toString().replace(/\/$/u, ""),
      clientId: normalizeNonEmptyString(bootstrap.clientId) ?? GPUI_SIDEBAR_DEFAULT_CLIENT_ID,
      focusedSessionId: normalizeNonEmptyString(bootstrap.focusedSessionId),
      initialActiveProjectId: normalizeNonEmptyString(bootstrap.initialActiveProjectId),
      visibleSessionIds: uniqueNonEmptyStrings(bootstrap.visibleSessionIds),
    };
  } catch {
    return undefined;
  }
}

function hasSameGpuiGxserverBootstrapTransport(
  left: GpuiValidatedGxserverBootstrap,
  right: GpuiValidatedGxserverBootstrap,
): boolean {
  return (
    left.authToken === right.authToken &&
    left.baseUrl === right.baseUrl &&
    left.clientId === right.clientId
  );
}

function activeGroupIdForGpuiGxserverBootstrapPresentationState({
  focusedSessionId,
  initialActiveProjectId,
}: Pick<GpuiValidatedGxserverBootstrap, "focusedSessionId" | "initialActiveProjectId">):
  string | undefined {
  const activeTerminal = resolveActiveTerminalSelection({
    activeProjectId: initialActiveProjectId,
    focusedSessionId,
  });
  if (activeTerminal?.remote) {
    return createGpuiRemotePresentationGroupId(
      activeTerminal.machineId,
      activeTerminal.projectId,
    );
  }
  const remoteProject = initialActiveProjectId
    ? parseGpuiRemotePresentationProjectId(initialActiveProjectId)
    : undefined;
  if (remoteProject) {
    return createGpuiRemotePresentationGroupId(remoteProject.machineId, remoteProject.projectId);
  }
  return initialActiveProjectId
    ? createGxserverPresentationProjectGroupId(initialActiveProjectId)
    : undefined;
}

function uniqueNonEmptyStrings(
  values: readonly unknown[] | undefined,
): readonly string[] | undefined {
  if (!Array.isArray(values)) {
    return undefined;
  }
  return [
    ...new Set(
      values.flatMap((value) => {
        const normalized = typeof value === "string" ? normalizeNonEmptyString(value) : undefined;
        return normalized ? [normalized] : [];
      }),
    ),
  ];
}

function sameStringSet(left: ReadonlySet<string>, right: ReadonlySet<string>): boolean {
  if (left.size !== right.size) {
    return false;
  }
  for (const value of left) {
    if (!right.has(value)) {
      return false;
    }
  }
  return true;
}

const GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT = 128;
const GPUI_COMMAND_PANE_SESSION_STRING_MAX_LENGTH = 512;
const GPUI_COMMAND_PANE_TIMER_DEADLINE_MAX_LENGTH = 64;
const GPUI_COMMAND_PANE_TIMER_LABEL_MAX_LENGTH = 32;
const GPUI_COMMAND_PANE_TIMER_REMAINING_MS_MAX = 2_147_483_647;

function normalizeGpuiWorkspaceSessionDelayedSends(
  sessions: readonly GpuiWorkspaceSessionDelayedSendSummary[] | unknown,
): GpuiWorkspaceSessionDelayedSendSummary[] {
  if (!Array.isArray(sessions)) {
    return [];
  }
  return sessions.slice(0, GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT).flatMap((session) => {
    if (!session || typeof session !== "object") {
      return [];
    }
    const record = session as Partial<
      Record<keyof GpuiWorkspaceSessionDelayedSendSummary, unknown>
    >;
    const sessionId = normalizeGpuiCommandPaneSessionString(record.sessionId);
    if (!sessionId || !parseGxserverPresentationProjectSessionId(sessionId)) {
      return [];
    }
    const delayedSendDeadlineAt = normalizeGpuiCommandPaneTimerDeadlineAt(
      record.delayedSendDeadlineAt,
    );
    const delayedSendRemainingLabel = normalizeGpuiWorkspaceDelayedSendRemainingLabel(
      record.delayedSendRemainingLabel,
    );
    const delayedSendRemainingMs = normalizeGpuiCommandPaneTimerRemainingMs(
      record.delayedSendRemainingMs,
    );
    const sendWhenAllProjectSessionsStopActive =
      record.sendWhenAllProjectSessionsStopActive === true;
    const sendWhenAgentStopsActive = record.sendWhenAgentStopsActive === true;
    if (
      !delayedSendDeadlineAt &&
      !delayedSendRemainingLabel &&
      delayedSendRemainingMs === undefined &&
      !sendWhenAllProjectSessionsStopActive &&
      !sendWhenAgentStopsActive
    ) {
      return [];
    }
    return [
      {
        ...(delayedSendDeadlineAt ? { delayedSendDeadlineAt } : {}),
        ...(delayedSendRemainingLabel ? { delayedSendRemainingLabel } : {}),
        ...(delayedSendRemainingMs !== undefined ? { delayedSendRemainingMs } : {}),
        ...(sendWhenAllProjectSessionsStopActive
          ? { sendWhenAllProjectSessionsStopActive: true }
          : {}),
        ...(sendWhenAgentStopsActive ? { sendWhenAgentStopsActive: true } : {}),
        sessionId,
      },
    ];
  });
}
const GPUI_GXSERVER_LOCAL_COMMAND_PANE_SESSION_ID_PATTERN = /^G[0-9][0-9A-Za-z_-]*$/u;

function normalizeGpuiBrowserTabs(
  tabs: readonly GpuiBrowserTabSummary[] | unknown,
): GpuiBrowserTabSummary[] {
  if (!Array.isArray(tabs)) {
    return [];
  }
  return tabs.slice(0, 256).flatMap((tab) => {
    if (!tab || typeof tab !== "object") {
      return [];
    }
    const record = tab as Partial<Record<keyof GpuiBrowserTabSummary, unknown>>;
    const projectId =
      typeof record.projectId === "string" ? normalizeNonEmptyString(record.projectId) : undefined;
    const tabId =
      typeof record.tabId === "string" ? normalizeNonEmptyString(record.tabId) : undefined;
    const title =
      typeof record.title === "string"
        ? normalizeNonEmptyString(record.title)?.slice(0, 512)
        : undefined;
    const url =
      typeof record.url === "string"
        ? record.url.trim().slice(0, GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS)
        : "";
    if (!projectId || !tabId || !title) {
      return [];
    }
    return [
      {
        isActive: record.isActive === true,
        isSleeping: record.isSleeping === true,
        isVisible: record.isVisible === true,
        projectId,
        tabId,
        title,
        url,
      },
    ];
  });
}

function relayoutGpuiSidebarSessions(
  sessions: readonly SidebarSessionItem[],
): SidebarSessionItem[] {
  return sessions.map((session, index) => ({
    ...session,
    column: index % GRID_COLUMN_COUNT,
    row: Math.floor(index / GRID_COLUMN_COUNT),
  }));
}

function gpuiBrowserSidebarSessionId(tab: GpuiBrowserTabSummary): string {
  return `gpui-browser:${encodeURIComponent(tab.projectId)}:${tab.tabId}`;
}

function normalizeGpuiWorkspaceDelayedSendRemainingLabel(value: unknown): string | undefined {
  if (value === "Waiting for agent" || value === "Waiting for agents") {
    return value;
  }
  return normalizeGpuiCommandPaneTimerRemainingLabel(value);
}

function normalizeGpuiCommandPaneSessions(
  sessions: readonly GpuiCommandPaneSessionSummary[] | unknown,
): GpuiCommandPaneSessionSummary[] {
  if (!Array.isArray(sessions)) {
    return [];
  }
  return sessions.slice(0, GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT).flatMap((session) => {
    if (!session || typeof session !== "object") {
      return [];
    }
    const record = session as Partial<Record<keyof GpuiCommandPaneSessionSummary, unknown>>;
    const sessionId = normalizeGpuiCommandPaneSessionString(record.sessionId);
    const status = normalizeGpuiCommandPaneSessionStatus(record.status);
    if (!sessionId || !status || !isGpuiGxserverLocalCommandPaneSessionId(sessionId)) {
      return [];
    }
    const commandId = normalizeGpuiCommandPaneSessionString(record.commandId);
    const title = normalizeGpuiCommandPaneSessionString(record.title);
    const delayedSendDeadlineAt = normalizeGpuiCommandPaneTimerDeadlineAt(
      record.delayedSendDeadlineAt,
    );
    const delayedSendRemainingLabel = normalizeGpuiCommandPaneTimerRemainingLabel(
      record.delayedSendRemainingLabel,
    );
    const delayedSendRemainingMs = normalizeGpuiCommandPaneTimerRemainingMs(
      record.delayedSendRemainingMs,
    );
    const closeAfterDoneDeadlineAt = normalizeGpuiCommandPaneTimerDeadlineAt(
      record.closeAfterDoneDeadlineAt,
    );
    const closeAfterDoneRemainingLabel = normalizeGpuiCommandPaneTimerRemainingLabel(
      record.closeAfterDoneRemainingLabel,
    );
    const closeAfterDoneRemainingMs = normalizeGpuiCommandPaneTimerRemainingMs(
      record.closeAfterDoneRemainingMs,
    );
    return [
      {
        ...(commandId ? { commandId } : {}),
        /*
        CDXC:GPUICommandPaneTimers 2026-06-27-02:05:
        Native Rust emits command-pane timer summaries with only Delayed Send and Close After Done display fields. Keep the TypeScript bridge at the same privacy boundary by normalizing and forwarding just bounded timer strings, non-negative remaining milliseconds, and a true-only Close After Done flag; never pass command text, cwd/env, URLs, paths, output, run ids, status-file paths, tokens, or unknown native fields into the Sidebar HUD.
        */
        ...(record.closeAfterDone === true ? { closeAfterDone: true } : {}),
        ...(closeAfterDoneDeadlineAt ? { closeAfterDoneDeadlineAt } : {}),
        ...(closeAfterDoneRemainingLabel ? { closeAfterDoneRemainingLabel } : {}),
        ...(closeAfterDoneRemainingMs !== undefined ? { closeAfterDoneRemainingMs } : {}),
        ...(delayedSendDeadlineAt ? { delayedSendDeadlineAt } : {}),
        ...(delayedSendRemainingLabel ? { delayedSendRemainingLabel } : {}),
        ...(delayedSendRemainingMs !== undefined ? { delayedSendRemainingMs } : {}),
        ...(record.isActive === true ? { isActive: true } : {}),
        ...(record.isPaneOwner === true ? { isPaneOwner: true } : {}),
        sessionId,
        status,
        ...(title ? { title } : {}),
      },
    ];
  });
}

function normalizeGpuiCommandPaneSessionString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim().replace(/\s+/g, " ");
  if (
    !normalized ||
    normalized.length > GPUI_COMMAND_PANE_SESSION_STRING_MAX_LENGTH ||
    /[\u0000-\u001F\u007F]/.test(normalized)
  ) {
    return undefined;
  }
  return normalized;
}

function normalizeGpuiCommandPaneTimerDeadlineAt(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim();
  if (
    !normalized ||
    normalized.length > GPUI_COMMAND_PANE_TIMER_DEADLINE_MAX_LENGTH ||
    /[\u0000-\u001F\u007F]/.test(normalized) ||
    !/^\d{4}-\d{2}-\d{2}T/u.test(normalized) ||
    Number.isNaN(Date.parse(normalized))
  ) {
    return undefined;
  }
  return normalized;
}

function normalizeGpuiCommandPaneTimerRemainingLabel(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim().replace(/\s+/g, " ");
  if (
    !normalized ||
    normalized.length > GPUI_COMMAND_PANE_TIMER_LABEL_MAX_LENGTH ||
    /[\u0000-\u001F\u007F]/.test(normalized) ||
    !/^[0-9dhms: .+-]+$/iu.test(normalized)
  ) {
    return undefined;
  }
  return normalized;
}

function normalizeGpuiCommandPaneTimerRemainingMs(value: unknown): number | undefined {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    value < 0 ||
    value > GPUI_COMMAND_PANE_TIMER_REMAINING_MS_MAX
  ) {
    return undefined;
  }
  return Math.ceil(value);
}

function normalizeGpuiCommandPaneSessionStatus(
  value: unknown,
): SidebarCommandSessionIndicator["status"] | undefined {
  return isValidGpuiCommandPaneSessionStatus(value) ? value : undefined;
}

function isValidGpuiCommandPaneSessionStatus(
  value: unknown,
): value is SidebarCommandSessionIndicator["status"] {
  return value === "idle" || value === "running" || value === "error";
}

function hasSameGpuiCommandPaneSessions(
  current: readonly GpuiCommandPaneSessionSummary[],
  next: readonly GpuiCommandPaneSessionSummary[],
): boolean {
  if (current.length !== next.length) {
    return false;
  }
  return current.every((session, index) => {
    const candidate = next[index];
    return (
      session.commandId === candidate?.commandId &&
      session.closeAfterDone === candidate?.closeAfterDone &&
      session.closeAfterDoneDeadlineAt === candidate?.closeAfterDoneDeadlineAt &&
      session.closeAfterDoneRemainingLabel === candidate?.closeAfterDoneRemainingLabel &&
      session.closeAfterDoneRemainingMs === candidate?.closeAfterDoneRemainingMs &&
      session.delayedSendDeadlineAt === candidate?.delayedSendDeadlineAt &&
      session.delayedSendRemainingLabel === candidate?.delayedSendRemainingLabel &&
      session.delayedSendRemainingMs === candidate?.delayedSendRemainingMs &&
      session.isActive === candidate?.isActive &&
      session.isPaneOwner === candidate?.isPaneOwner &&
      session.sessionId === candidate?.sessionId &&
      session.status === candidate?.status &&
      session.title === candidate?.title
    );
  });
}

function isGpuiGxserverLocalCommandPaneSessionId(sessionId: unknown): sessionId is string {
  /*
  CDXC:GPUICommandPane 2026-06-27-01:37:
  GPUI command-pane summaries are live local tab state for gxserver-backed native-shaped `G...` command sessions only. Rust shell internals may still carry numeric ids, so drop raw numeric strings, lowercase `g...`, malformed strings, and non-string rows at the bridge boundary before stale native-local command tabs can drive HUD indicators, active-tab state, timer projection, or auto-sleep protection.
  */
  return (
    typeof sessionId === "string" &&
    GPUI_GXSERVER_LOCAL_COMMAND_PANE_SESSION_ID_PATTERN.test(sessionId)
  );
}

type GpuiSidebarCommandSessionIndicatorScope = {
  activeProjectId?: string;
  presentation?: GxserverPresentationSnapshot;
};

function filterGpuiGxserverLocalCommandPaneSessions(
  commandPaneSessions: readonly GpuiCommandPaneSessionSummary[],
  scope: GpuiSidebarCommandSessionIndicatorScope = {},
): GpuiCommandPaneSessionSummary[] {
  /*
  CDXC:GPUICommandPane 2026-06-27-08:32:
  Command-pane ownership consumers require both an external native-shaped local `G...` id and a valid Sidebar HUD status. Reuse this filter for HUD indicators and Auto Sleep owner protection so malformed native rows, including `isPaneOwner:true` rows with invalid status, cannot keep sessions awake.

  CDXC:GPUICommandPane 2026-06-27-08:45:
  Native presentation cleanup removes stale command-panel rows after authoritative gxserver snapshots and explicit removal deltas. When the live HUD is built with an active project and presentation, require the command-pane summary id to still exist in that active project so deleted local `G...` tabs cannot keep Action indicators, timers, or active states visible.
  */
  const presentedSessionIds =
    scope.activeProjectId && scope.presentation
      ? new Set<string>(
          scope.presentation.sessions.flatMap((session) =>
            session.projectId === scope.activeProjectId ? [session.sessionId] : [],
          ),
        )
      : undefined;
  return commandPaneSessions.filter((session) => {
    if (
      !isGpuiGxserverLocalCommandPaneSessionId(session.sessionId) ||
      !isValidGpuiCommandPaneSessionStatus(session.status)
    ) {
      return false;
    }
    return presentedSessionIds ? presentedSessionIds.has(session.sessionId) : true;
  });
}

export function createGpuiSidebarCommandSessionIndicators(
  commands: readonly SidebarCommandButton[],
  commandPaneSessions: readonly GpuiCommandPaneSessionSummary[],
  scope: GpuiSidebarCommandSessionIndicatorScope = {},
): SidebarCommandSessionIndicator[] {
  /*
  CDXC:GPUICommandPane 2026-06-27-06:30:
  Command-session HUD status is owned by Rust's sanitized command-pane summary. The TypeScript bridge may forward only external native-shaped local `G...` command-pane rows whose status is already a Sidebar HUD status; internal Rust numeric shell ids and malformed bridge rows must not match HUD Actions or infer status from renderer activity, command text, paths, URLs, output, logs, titles, status files, or other private fields.

  CDXC:GPUICommandPane 2026-06-27-08:45:
  Keep the exported helper backward-compatible for direct two-argument tests and callers. Live HUD construction passes the optional active-project presentation scope so stale command-pane summaries are pruned against the full current presentation, not against whichever ids happen to appear in a non-removal delta.
  */
  const localCommandPaneSessions = filterGpuiGxserverLocalCommandPaneSessions(
    commandPaneSessions,
    scope,
  );
  return commands.flatMap((command) => {
    if (command.actionType !== "terminal") {
      return [];
    }
    const commandTitleKey = getGpuiSidebarCommandTitleKey(
      getGpuiSidebarCommandSessionTitle(command),
    );
    if (!commandTitleKey) {
      return [];
    }
    const mappedSession = localCommandPaneSessions.find(
      (session) =>
        session.commandId === command.commandId &&
        getGpuiSidebarCommandTitleKey(session.title) === commandTitleKey,
    );
    const session =
      mappedSession ??
      localCommandPaneSessions.find(
        (candidate) => getGpuiSidebarCommandTitleKey(candidate.title) === commandTitleKey,
      );
    if (!session) {
      return [];
    }
    return [
      {
        commandId: command.commandId,
        ...(session.closeAfterDone === true ? { closeAfterDone: true } : {}),
        ...(session.closeAfterDoneDeadlineAt
          ? {
              closeAfterDoneDeadlineAt: session.closeAfterDoneDeadlineAt,
            }
          : {}),
        ...(session.closeAfterDoneRemainingLabel
          ? {
              closeAfterDoneRemainingLabel: session.closeAfterDoneRemainingLabel,
            }
          : {}),
        ...(session.closeAfterDoneRemainingMs !== undefined
          ? {
              closeAfterDoneRemainingMs: session.closeAfterDoneRemainingMs,
            }
          : {}),
        ...(session.delayedSendDeadlineAt
          ? {
              delayedSendDeadlineAt: session.delayedSendDeadlineAt,
            }
          : {}),
        ...(session.delayedSendRemainingLabel
          ? {
              delayedSendRemainingLabel: session.delayedSendRemainingLabel,
            }
          : {}),
        ...(session.delayedSendRemainingMs !== undefined
          ? {
              delayedSendRemainingMs: session.delayedSendRemainingMs,
            }
          : {}),
        isActive: session.isActive === true,
        sessionId: session.sessionId,
        status: session.status,
        ...(session.title ? { title: session.title } : {}),
      },
    ];
  });
}

function getGpuiSidebarCommandSessionTitle(command: SidebarCommandButton): string {
  const normalizedActionName = command.name.trim();
  return normalizedActionName.length > 0
    ? normalizedActionName
    : (command.command ?? "").trim().slice(0, 20);
}

function getGpuiSidebarCommandTitleKey(value: string | undefined): string {
  return normalizeGpuiCommandPaneSessionString(value)?.toLocaleLowerCase() ?? "";
}

function createGpuiSidebarHudState({
  activeProjectId,
  commandPaneSessions = [],
  domainProjects = [],
  focusedSessionId,
  git,
  groups = [],
  presentation,
  recentProjects = [],
  remoteRecentProjectsByMachineId,
  remotePresentationsByMachineId,
  runtimeSettings,
  sidebarHud,
}: {
  activeProjectId?: string;
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  domainProjects?: readonly GxserverProjectDomainState[];
  focusedSessionId?: string;
  git?: SidebarGitState;
  groups?: readonly SidebarSessionGroup[];
  presentation?: GxserverPresentationSnapshot;
  recentProjects?: readonly GxserverRecentProjectDomainState[];
  remoteRecentProjectsByMachineId?: ReadonlyMap<
    string,
    readonly GxserverRecentProjectDomainState[]
  >;
  remotePresentationsByMachineId?: ReadonlyMap<string, GxserverPresentationSnapshot>;
  runtimeSettings?: GpuiSidebarRuntimeSettings;
  sidebarHud?: GxserverSidebarHudResponse;
} = {}): SidebarHudState {
  const settings = createGpuiSidebarSettings(runtimeSettings);
  /*
   * CDXC:SidebarHudContract 2026-06-24-20:34:
   * GPUI SidebarApp uses gxserver's `/api/readSidebarHud` projection for read-side agent/action buttons so live sidebar and app-modal Settings share one production contract. The local shared defaults are only for pre-bootstrap or unavailable gxserver state; project metadata is not re-normalized here.
   */
  const agents = (
    sidebarHud
      ? ([...sidebarHud.agents] as SidebarAgentButton[])
      : createSidebarAgentButtons([], [])
  ).filter((agent) => T3CODE_ENABLED || agent.agentId !== "t3");
  /*
   * CDXC:ProjectActions 2026-08-01:
   * `showOnProjectRow` is optional on the gxserver contract because a daemon
   * older than the app drops fields it does not know, so a legacy response
   * yields `undefined` where SidebarCommandButton promises a boolean. Normalize
   * at the surface boundary instead of casting the gap away, so row rendering
   * and the Settings toggle both see a real boolean.
   */
  const normalizeHudCommands = (
    hudCommands: readonly GxserverSidebarHudResponse["commands"][number][],
  ): ReturnType<typeof createSidebarCommandButtons> =>
    hudCommands.map((command) => ({
      ...command,
      showOnProjectRow: command.showOnProjectRow === true,
    })) as ReturnType<typeof createSidebarCommandButtons>;
  const commands = sidebarHud
    ? normalizeHudCommands(sidebarHud.commands)
    : createSidebarCommandButtons([], [], []);
  /*
   * CDXC:GlobalActions 2026-08-01:
   * `globalCommands` is optional on the gxserver contract because a daemon
   * older than the app drops fields it does not know. Normalize the gap to an
   * empty list here, at the surface boundary, so Settings renders an empty
   * Global Actions section instead of failing on undefined.
   */
  const globalCommands = (
    sidebarHud?.globalCommands ? normalizeHudCommands(sidebarHud.globalCommands) : []
  ) as ReturnType<typeof createSidebarCommandButtons>;
  const commandsByProject = sidebarHud?.commandsByProject
    ? Object.fromEntries(
        Object.entries(sidebarHud.commandsByProject).map(([projectId, projectCommands]) => [
          projectId,
          normalizeHudCommands(projectCommands),
        ]),
      )
    : undefined;
  const focusedSession = groups
    .flatMap((group) => group.sessions)
    .find(
      (session) =>
        parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId ===
          focusedSessionId || session.sessionId === focusedSessionId,
    );
  const visibleSessions = groups.flatMap((group) =>
    group.sessions.filter((session) => session.isVisible),
  );
  return {
    activeSessionsSortMode: "lastActivity",
    agentManagerZoomPercent: settings.agentManagerZoomPercent,
    agents,
    commands,
    ...(commandsByProject ? { commandsByProject } : {}),
    commandSessionIndicators: createGpuiSidebarCommandSessionIndicators(
      commands,
      commandPaneSessions,
      {
        activeProjectId,
        presentation,
      },
    ),
    completionBellEnabled: settings.completionBellEnabled,
    completionSound: settings.completionSound,
    completionSoundLabel: getCompletionSoundLabel(settings.completionSound),
    debuggingMode: settings.debuggingMode,
    focusedSessionTitle:
      focusedSession?.displayTitle ?? focusedSession?.primaryTitle ?? focusedSession?.alias,
    git: git ?? createDefaultSidebarGitState(),
    globalCommands,
    highlightedVisibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    isFocusModeActive: false,
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29:
    Settle/snooze capability is per daemon, and GPUI holds one presentation
    snapshot per daemon: the local gxserver plus `remotePresentations` keyed by
    machine id. Publish them separately so the sidebar can gate a remote
    machine's rows on that machine's own answer. A snapshot with no
    `capabilities` block (an older daemon) projects to `undefined`, which the
    sidebar reads as "no lifecycle" and hides the affordances — never as
    "assume it works".

    CDXC:SidebarV2Git 2026-07-29:
    The per-session git/PR probe rides the SAME block (`sessionGitStatus`) and
    the same two paths, so a remote machine whose daemon predates the probe
    renders plain cards while the local one shows branch/PR lines. The git data
    itself needs no plumbing here: it lives on the presentation session and
    reaches the sidebar through the existing snapshot/delta projection.
    */
    lifecycleCapabilities: gxserverPresentationSidebarLifecycleCapabilities(presentation),
    lifecycleCapabilitiesByMachineId: Object.fromEntries(
      [...(remotePresentationsByMachineId ?? new Map())].flatMap(
        ([machineId, remotePresentation]) => {
          const capabilities =
            gxserverPresentationSidebarLifecycleCapabilities(remotePresentation);
          return capabilities ? [[machineId, capabilities] as const] : [];
        },
      ),
    ),
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29:
    The auto-settle WINDOW travels the same two paths as the capability block
    above, and for the same reason: each daemon runs its own sweep against its
    own `sidebarAutoSettleAfterDays`, so the local user's window is not an
    answer for a remote machine. A daemon that omits the field is left OUT of
    the map entirely rather than defaulted, because "absent" and "null" mean
    different things to the sidebar (fall back to the local setting vs. do not
    inactivity-settle at all).
    */
    autoSettleAfterDays: gxserverPresentationSidebarAutoSettleAfterDays(presentation),
    autoSettleAfterDaysByMachineId: Object.fromEntries(
      [...(remotePresentationsByMachineId ?? new Map())].flatMap(
        ([machineId, remotePresentation]) => {
          const autoSettleAfterDays =
            gxserverPresentationSidebarAutoSettleAfterDays(remotePresentation);
          return autoSettleAfterDays === undefined
            ? []
            : [[machineId, autoSettleAfterDays] as const];
        },
      ),
    ),
    pendingAgentIds: [],
    projectSettingsProjects: createGpuiProjectSettingsProjects(domainProjects, presentation),
    /*
    CDXC:GPUIRecentProjects 2026-06-24-12:27:
    GPUI Recent Projects hydrate from `/api/listRecentProjects`, a
    gxserver-owned parked-project contract. Keep an empty drawer when the
    endpoint has no explicit rows; never derive recent projects from labels,
    inactive sessions, presentation titles, command text, or path guessing.
    */
    recentProjects: [
      ...createGpuiRecentProjects(recentProjects, settings),
      ...createGpuiRemoteRecentProjects(
        remoteRecentProjectsByMachineId,
        remotePresentationsByMachineId,
        settings,
      ),
    ].sort(compareGpuiRecentProjectsByClosedAt),
    settings,
    createSessionOnSidebarDoubleClick: settings.createSessionOnSidebarDoubleClick,
    renameSessionOnDoubleClick: settings.renameSessionOnDoubleClick,
    showCloseButtonOnSessionCards: settings.showCloseButtonOnSessionCards,
    theme: resolveSidebarTheme(settings.sidebarTheme, "dark"),
    viewMode: "grid",
    visibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    visibleSlotLabels: visibleSessions.map((session) => session.shortcutLabel),
  };
}

const GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE = "ghostex.gpui.sidebar.titlebarGitMenuState";
const GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION = 1;
const GPUI_TITLEBAR_GIT_ACTION_MESSAGE_TYPE = "ghostex.gpui.sidebar.titlebarGitAction";
const GPUI_TITLEBAR_GIT_ACTION_MESSAGE_VERSION = 1;
const GPUI_TITLEBAR_GIT_ACTIONS: ReadonlySet<SidebarGitAction> = new Set([
  "commit",
  "push",
  "pr",
  "syncMain",
  "syncRemote",
  "multiRelease",
  "release",
]);

type GpuiWorktreeDeleteBranchMetadata = {
  branch: string | null;
  canDeleteLocalBranch: boolean;
  localBranchName?: string;
  remoteBranchDisabledReason?: string;
  remoteBranchExists: boolean;
  remoteBranchName?: string;
  remoteName: string;
};

function normalizeGpuiWorktreeDeleteBranchName(
  currentBranch: string | null | undefined,
  fallbackBranch: string | null | undefined,
): string | undefined {
  for (const candidate of [currentBranch, fallbackBranch]) {
    const branch = candidate?.trim();
    if (branch && branch !== "HEAD" && branch !== "detached") {
      return branch;
    }
  }
  return undefined;
}

async function resolveGpuiWorktreeDeleteBranchMetadata(
  branchName: string | undefined,
  checkRemoteBranch: (
    remoteName: string,
    remoteBranchName: string,
  ) => Promise<GxserverTypedOperationResult>,
): Promise<GpuiWorktreeDeleteBranchMetadata> {
  const remoteName = "origin";
  if (!branchName) {
    return {
      branch: null,
      canDeleteLocalBranch: false,
      remoteBranchDisabledReason: "No local branch is checked out for this worktree.",
      remoteBranchExists: false,
      remoteName,
    };
  }
  const remoteBranch = await checkRemoteBranch(remoteName, branchName);
  const remoteBranchExists = remoteBranch.exitCode === 0;
  return {
    branch: branchName,
    canDeleteLocalBranch: true,
    localBranchName: branchName,
    remoteBranchDisabledReason: remoteBranchExists
      ? undefined
      : `No ${remoteName}/${branchName} remote branch exists.`,
    remoteBranchExists,
    remoteBranchName: branchName,
    remoteName,
  };
}

function hasGpuiGitShortStatusChanges(stdout: string): boolean {
  return stdout.split("\n").some((line) => {
    const trimmed = line.trim();
    return trimmed.length > 0 && !trimmed.startsWith("##");
  });
}

type GpuiWorktreeModalCommand =
  | Extract<SidebarToExtensionMessage, { type: "requestProjectWorktrees" }>
  | Extract<SidebarToExtensionMessage, { type: "createProjectWorktree" }>
  | Extract<SidebarToExtensionMessage, { type: "confirmDeleteWorktree" }>
  | Extract<SidebarToExtensionMessage, { type: "commitWorktreeBeforeDelete" }>;

type GpuiGitCommitModalCommand =
  | Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>
  | Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>
  | Extract<SidebarToExtensionMessage, { type: "runSidebarGitMultipleCommits" }>
  | Extract<SidebarToExtensionMessage, { type: "openSidebarGitChangedFileDiff" }>
  | Extract<SidebarToExtensionMessage, { type: "cancelSidebarGitCommit" }>;

function parseGpuiGitCommitModalCommand(payload: unknown): GpuiGitCommitModalCommand | undefined {
  if (!payload || typeof payload !== "object") {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  const stringField = (field: string, maxChars: number, allowEmpty = false): string | undefined => {
    const value = record[field];
    return typeof value === "string" && (allowEmpty || value.length > 0) && value.length <= maxChars
      ? value
      : undefined;
  };
  const requestId = stringField("requestId", 120);
  if (!requestId) {
    return undefined;
  }
  const agentId = stringField("agentId", 300);
  switch (record.type) {
    case "confirmSidebarGitCommit":
    case "confirmSidebarGitDirectMerge": {
      const message = stringField("message", 20_000, true);
      if (message === undefined) {
        return undefined;
      }
      const filePaths = Array.isArray(record.filePaths)
        ? record.filePaths.filter(
            (value): value is string =>
              typeof value === "string" && value.length > 0 && value.length <= 1024,
          )
        : undefined;
      return {
        agentId,
        deleteWorktreeAfter: record.deleteWorktreeAfter === true,
        filePaths,
        message,
        requestId,
        type: record.type,
        ...(record.type === "confirmSidebarGitCommit"
          ? { commitOnNewRef: record.commitOnNewRef === true }
          : {}),
      };
    }
    case "runSidebarGitMultipleCommits":
      return { agentId, requestId, type: "runSidebarGitMultipleCommits" };
    case "openSidebarGitChangedFileDiff": {
      const filePath = stringField("filePath", 1024);
      return filePath ? { filePath, requestId, type: "openSidebarGitChangedFileDiff" } : undefined;
    }
    case "cancelSidebarGitCommit":
      return { requestId, type: "cancelSidebarGitCommit" };
    default:
      return undefined;
  }
}

function parseGpuiWorktreeModalCommand(payload: unknown): GpuiWorktreeModalCommand | undefined {
  // Worktree modal commands arrive from the native app-modal host bridge.
  // Rebuild them field-by-field with bounded strings so only the shared modal
  // contract enters the runtime's worktree/git handlers, which then
  // revalidate all project and worktree identity against gxserver state.
  if (!payload || typeof payload !== "object") {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  const stringField = (field: string, maxChars: number): string | undefined => {
    const value = record[field];
    return typeof value === "string" && value.length > 0 && value.length <= maxChars
      ? value
      : undefined;
  };
  switch (record.type) {
    case "requestProjectWorktrees": {
      const requestId = stringField("requestId", 120);
      if (!requestId) {
        return undefined;
      }
      return {
        projectId: stringField("projectId", 300),
        projectPath: stringField("projectPath", 1024),
        remoteMachineId: stringField("remoteMachineId", 300),
        requestId,
        type: "requestProjectWorktrees",
      };
    }
    case "createProjectWorktree":
      return {
        agentId: stringField("agentId", 300),
        baseBranch: stringField("baseBranch", 300),
        existingWorktreeKey: stringField("existingWorktreeKey", 600),
        existingWorktreePath: stringField("existingWorktreePath", 1024),
        mode: record.mode === "openExisting" || record.mode === "create" ? record.mode : undefined,
        projectId: stringField("projectId", 300),
        projectPath: stringField("projectPath", 1024),
        prompt: stringField("prompt", 20_000),
        remoteMachineId: stringField("remoteMachineId", 300),
        type: "createProjectWorktree",
      };
    case "confirmDeleteWorktree": {
      const projectId = stringField("projectId", 300);
      if (!projectId) {
        return undefined;
      }
      return {
        deleteLocalBranch: record.deleteLocalBranch === true,
        deleteRemoteBranch: record.deleteRemoteBranch === true,
        projectId,
        type: "confirmDeleteWorktree",
      };
    }
    case "commitWorktreeBeforeDelete": {
      const groupId = stringField("groupId", 300);
      if (!groupId) {
        return undefined;
      }
      return { groupId, type: "commitWorktreeBeforeDelete" };
    }
    default:
      return undefined;
  }
}

function parseGpuiTitlebarGitAction(payload: unknown): SidebarGitAction | "refresh" | undefined {
  // Native titlebar Git menu selections carry a fixed action selector only;
  // reject everything else so this bridge can never smuggle command text,
  // paths, or ids into the Git pipeline.
  if (!payload || typeof payload !== "object") {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  if (
    record.type !== GPUI_TITLEBAR_GIT_ACTION_MESSAGE_TYPE ||
    record.version !== GPUI_TITLEBAR_GIT_ACTION_MESSAGE_VERSION ||
    typeof record.action !== "string"
  ) {
    return undefined;
  }
  if (record.action === "refresh") {
    return "refresh";
  }
  return GPUI_TITLEBAR_GIT_ACTIONS.has(record.action as SidebarGitAction)
    ? (record.action as SidebarGitAction)
    : undefined;
}

export function createGpuiTitlebarGitMenuStatePayload(state: SidebarGitState): {
  additions: number;
  aheadCount: number;
  behindCount: number;
  branch: string | null;
  deletions: number;
  hasWorkingTreeChanges: boolean;
  isBusy: boolean;
  isRepo: boolean;
  primaryAction: SidebarGitAction;
  rows: {
    action: SidebarGitAction;
    disabled: boolean;
    label: string;
    primary: boolean;
  }[];
  syncRemoteDisabled: boolean;
  type: string;
  version: number;
} {
  // The native titlebar renders this projection verbatim, so the shared menu
  // builders stay the single owner of row order, labels, and disabled gating.
  // The primary row carries the resolved split-primary label macOS shows on
  // its split button, since a native menu cannot express the split control.
  const primary = resolveSidebarGitPrimaryActionState(state);
  return {
    additions: state.additions,
    aheadCount: state.aheadCount,
    behindCount: state.behindCount,
    branch: state.branch,
    deletions: state.deletions,
    hasWorkingTreeChanges: state.hasWorkingTreeChanges,
    isBusy: state.isBusy,
    isRepo: state.isRepo,
    primaryAction: primary.action,
    rows: buildSidebarGitMenuItems(state).map((item) => ({
      action: item.action,
      disabled: item.action === primary.action ? primary.disabled : item.disabled,
      label: item.action === primary.action ? primary.label : item.label,
      primary: item.action === primary.action,
    })),
    syncRemoteDisabled:
      getSidebarGitDisabledReason(state, "syncRemote") !== undefined ||
      !hasSidebarGitRemoteCommitDelta(state),
    type: GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE,
    version: GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION,
  };
}

function createGpuiSidebarSettings(runtimeSettings?: GpuiSidebarRuntimeSettings): ghostexSettings {
  /*
  CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
  GPUI SidebarApp must receive the real saved shared Settings object, normalized through the same TypeScript settings schema as macOS, instead of hardcoded bootstrap defaults. Keep debuggingMode/showBetaFeatures pinned to strict CEF-provided booleans so string-like or numeric truthy values cannot alter the Settings/HUD projection.
  */
  const settings = normalizeghostexSettings(runtimeSettings?.settings);
  return {
    ...settings,
    debuggingMode: runtimeSettings?.debuggingMode === true,
    showBetaFeatures: runtimeSettings?.showBetaFeatures === true,
  };
}

export function createGpuiAutoSleepAgentSessionIds({
  activeProjectId,
  commandPaneSessions = [],
  focusedSessionId,
  groups = [],
  nowMs,
  presentation,
  settings,
}: {
  activeProjectId?: string;
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  focusedSessionId?: string;
  groups?: readonly SidebarSessionGroup[];
  nowMs: number;
  presentation: GxserverPresentationSnapshot;
  settings: Pick<
    ghostexSettings,
    | "autoSleepAgentIdleMinutes"
    | "autoSleepAgentSessionsEnabled"
    | "autoSleepFavoriteAgentSessions"
    | "autoSleepRequireAgentResumeCommand"
  >;
}): string[] {
  /*
  CDXC:GPUISidebarAutoSleep 2026-06-27-01:24:
  GPUI Agent Auto Sleep must choose only local gxserver presentation agent terminals after protecting selected/visible sidebar owners, focused sessions, active command-pane owners, and popped-out rows. Return bounded project/session routing ids for the existing setSessionSleeping path; do not inspect Browser/project-editor surfaces, titles, paths, commands, terminal output, URLs, tokens, or remote-machine rows.
  */
  if (!settings.autoSleepAgentSessionsEnabled) {
    return [];
  }
  const protectedProjectSessionKeys = collectGpuiAutoSleepProtectedProjectSessionKeys({
    activeProjectId,
    commandPaneSessions,
    focusedSessionId,
    groups,
    presentation,
  });
  return presentation.sessions.flatMap((session) =>
    shouldAutoSleepGpuiPresentationAgentSession({
      nowMs,
      protectedProjectSessionKeys,
      session,
      settings,
    })
      ? [createGxserverPresentationProjectSessionId(session.projectId, session.sessionId)]
      : [],
  );
}

export function collectGpuiAutoSleepProtectedProjectSessionKeys({
  activeProjectId,
  commandPaneSessions = [],
  focusedSessionId,
  groups = [],
  presentation,
}: {
  activeProjectId?: string;
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  focusedSessionId?: string;
  groups?: readonly SidebarSessionGroup[];
  presentation: GxserverPresentationSnapshot;
}): Set<string> {
  const protectedProjectSessionKeys = new Set<string>();
  for (const group of groups) {
    if (group.remoteMachineContext) {
      continue;
    }
    let hasProjectedOwner = false;
    for (const session of group.sessions) {
      if (session.isFocused === true || session.isVisible === true) {
        addGpuiAutoSleepProtectedSessionId(
          protectedProjectSessionKeys,
          presentation,
          session.sessionId,
          group.projectContext?.editor.projectId,
        );
        hasProjectedOwner = true;
      }
      if (session.isPoppedOut === true) {
        addGpuiAutoSleepProtectedSessionId(
          protectedProjectSessionKeys,
          presentation,
          session.sessionId,
          group.projectContext?.editor.projectId,
        );
      }
    }
    if (!hasProjectedOwner && group.sessions[0]) {
      addGpuiAutoSleepProtectedSessionId(
        protectedProjectSessionKeys,
        presentation,
        group.sessions[0].sessionId,
        group.projectContext?.editor.projectId,
      );
    }
  }
  addGpuiAutoSleepProtectedSessionId(protectedProjectSessionKeys, presentation, focusedSessionId);
  /*
  CDXC:GPUISidebarAutoSleep 2026-06-27-06:54:
  Native Auto Sleep protects the active owner of every visible command-panel split leaf from the command-pane layout, not the HUD-focused tab. GPUI Rust sends that split ownership as sanitized `isPaneOwner:true` on external native-shaped `G...` ids; TypeScript protects only that field after the same local id and valid-status filtering used by command indicators, so internal numeric Rust ids, stale legacy rows, collapsed HUD focus, and malformed statuses cannot keep sessions awake.

  CDXC:GPUISidebarAutoSleep 2026-06-27-07:28:
  Native command-panel layout is scoped to the active project, so a GPUI command-pane owner summary must protect only the active project's matching external `G...` session. Do not treat a bare command-pane id as globally owned across projects because that can keep unrelated same-id agent sessions awake.
  */
  const localCommandPaneSessions = filterGpuiGxserverLocalCommandPaneSessions(commandPaneSessions);
  for (const commandPaneSession of localCommandPaneSessions) {
    if (commandPaneSession.isPaneOwner === true) {
      addGpuiAutoSleepProtectedSessionId(
        protectedProjectSessionKeys,
        presentation,
        commandPaneSession.sessionId,
        activeProjectId,
      );
    }
  }
  return protectedProjectSessionKeys;
}

function shouldAutoSleepGpuiPresentationAgentSession({
  nowMs,
  protectedProjectSessionKeys,
  session,
  settings,
}: {
  nowMs: number;
  protectedProjectSessionKeys: ReadonlySet<string>;
  session: GxserverPresentationSession;
  settings: Pick<
    ghostexSettings,
    | "autoSleepAgentIdleMinutes"
    | "autoSleepAgentSessionsEnabled"
    | "autoSleepFavoriteAgentSessions"
    | "autoSleepRequireAgentResumeCommand"
  >;
}): boolean {
  if (session.lifecycleState !== "running" || session.activity !== "idle") {
    return false;
  }
  if (session.actions.sleep !== true || !isGpuiAutoSleepAgentTerminalSession(session)) {
    return false;
  }
  if (
    protectedProjectSessionKeys.has(
      gpuiAutoSleepProjectSessionKey(session.projectId, session.sessionId),
    )
  ) {
    return false;
  }
  if (session.isFavorite === true && settings.autoSleepFavoriteAgentSessions !== true) {
    return false;
  }
  if (
    settings.autoSleepRequireAgentResumeCommand &&
    !gpuiAutoSleepSessionHasAgentResumeReference(session)
  ) {
    return false;
  }
  const lastActivityMs = gpuiAutoSleepLastActivityMs(session);
  if (lastActivityMs === undefined) {
    return false;
  }
  return nowMs - lastActivityMs >= settings.autoSleepAgentIdleMinutes * GPUI_AUTO_SLEEP_MINUTE_MS;
}

function gpuiAutoSleepSessionHasAgentResumeReference(
  session: GxserverPresentationSession,
): boolean {
  /*
  gxserver sleep kills the zmx provider and wake relaunches from the daemon's
  stored agent resume state, so a session without any published resume
  reference wakes degraded. macOS validates per-agent reference formats against
  its local agents catalog (canRestoreNativeTerminalSession); GPUI evaluates
  the same restorability contract against the daemon-published resume fields,
  which gxserver already normalizes.
  */
  return Boolean(
    normalizeNonEmptyString(session.agentSessionId) ||
    normalizeNonEmptyString(session.agentSessionPath) ||
    normalizeNonEmptyString(session.trustedResumeTitle),
  );
}

function isGpuiAutoSleepAgentTerminalSession(session: GxserverPresentationSession): boolean {
  if (session.kind === "t3") {
    return false;
  }
  if (session.surface !== "workspace" && session.surface !== "commands") {
    return false;
  }
  if (session.kind === "agent") {
    return true;
  }
  return Boolean(
    normalizeNonEmptyString(session.agentId) ||
    normalizeNonEmptyString(session.agentName) ||
    normalizeNonEmptyString(session.agentSessionId) ||
    normalizeNonEmptyString(session.agentSessionPath),
  );
}

function gpuiAutoSleepLastActivityMs(session: GxserverPresentationSession): number | undefined {
  const timestamp = session.lastActiveAt ?? session.updatedAt;
  const timestampMs = Date.parse(timestamp);
  return Number.isFinite(timestampMs) ? timestampMs : undefined;
}

function addGpuiAutoSleepProtectedSessionId(
  protectedProjectSessionKeys: Set<string>,
  presentation: GxserverPresentationSnapshot,
  sessionId: string | undefined,
  projectIdHint?: string,
): void {
  const normalizedSessionId = normalizeNonEmptyString(sessionId)?.trim();
  if (!normalizedSessionId || parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
    return;
  }
  const scopedReference = parseGxserverPresentationProjectSessionId(normalizedSessionId);
  if (scopedReference) {
    protectedProjectSessionKeys.add(
      gpuiAutoSleepProjectSessionKey(scopedReference.projectId, scopedReference.sessionId),
    );
    return;
  }
  const matchingSessions = presentation.sessions.filter(
    (session) =>
      session.sessionId === normalizedSessionId &&
      (!projectIdHint || session.projectId === projectIdHint),
  );
  for (const session of matchingSessions) {
    protectedProjectSessionKeys.add(
      gpuiAutoSleepProjectSessionKey(session.projectId, session.sessionId),
    );
  }
}

function gpuiAutoSleepProjectSessionKey(projectId: string, sessionId: string): string {
  return `${projectId}\u0000${sessionId}`;
}

export function createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups(
  groups: readonly SidebarSessionGroup[],
): GpuiSessionStatusIndicatorCandidate[] {
  /*
  CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
  GPUI derives status/pet candidates from the live gxserver SidebarApp groups because the GPUI sidebar entry mounts SidebarApp directly and never runs native-sidebar.tsx. Preserve the same project/session order semantics as macOS by reusing shared display layout, but keep the bridge payload bounded and route with ids only rather than paths, commands, terminal text, external URLs, or daemon bodies. Project icon parity may carry only an already-normalized image data URL for notification attachments.
  */
  const candidates: GpuiSessionStatusIndicatorCandidate[] = [];
  let order = 0;
  for (const group of groups) {
    if (candidates.length >= GPUI_STATUS_INDICATOR_MAX_CANDIDATES) {
      break;
    }
    const groupProjectId = group.projectContext?.editor.projectId;
    const groupIconDataUrl = normalizeWorkspaceProjectIconDataUrl(
      group.projectContext?.iconDataUrl,
    );
    const sessionsById = Object.fromEntries(
      group.sessions.map((session) => [session.sessionId, session]),
    );
    const manualSessionIds = group.sessions.map((session) => session.sessionId);
    const displayLayout = createDisplaySessionLayout({
      sessionIdsByGroup: { [group.groupId]: manualSessionIds },
      sessionsById,
      sortMode: "lastActivity",
      workspaceGroupIds: [group.groupId],
    });
    const visualSessionIds = displayLayout.sessionIdsByGroup[group.groupId] ?? manualSessionIds;
    for (const sessionId of visualSessionIds) {
      if (candidates.length >= GPUI_STATUS_INDICATOR_MAX_CANDIDATES) {
        break;
      }
      const session = sessionsById[sessionId];
      if (!session) {
        continue;
      }
      const combinedReference = parseGxserverPresentationProjectSessionId(session.sessionId);
      const candidateProjectId = groupProjectId ?? combinedReference?.projectId;
      if (!candidateProjectId) {
        continue;
      }
      candidates.push({
        hasRunningZmxBacking: hasRunningZmxBackingForGpuiIdleIndicator(session),
        ...(groupIconDataUrl ? { iconDataUrl: groupIconDataUrl } : {}),
        lastInteractionAt: session.lastInteractionAt,
        order,
        projectId: candidateProjectId,
        projectTitle: boundedGpuiStatusIndicatorTitle(
          group.title || candidateProjectId,
          candidateProjectId,
        ),
        sessionId: session.sessionId,
        status: getGpuiSessionStatusIndicatorStatus(session),
        title: getGpuiPetOverlaySessionTitle(session),
      });
      order += 1;
    }
  }
  return candidates;
}

export function createGpuiSessionStatusIndicatorsPayload(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
  settings: ghostexSettings,
): GpuiSessionStatusIndicatorsPayload {
  const counts = countGpuiSessionStatusIndicatorCandidates(candidates);
  return {
    attentionCount: counts.attention,
    availableCount: counts.available,
    hideMenuBarIndicators: settings.hideMenuBarSessionStatusIndicators,
    projects: createGpuiSessionStatusIndicatorProjects(candidates),
    type: GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE,
    version: GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION,
    workingCount: counts.working,
  };
}

export function createGpuiPetOverlayStatePayload(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
  settings: ghostexSettings,
): GpuiPetOverlayStatePayload {
  const actionableActivityCandidates = candidates.filter(
    (candidate) => candidate.status === "attention" || candidate.status === "working",
  );
  const shownActivityCandidates =
    actionableActivityCandidates.length > 0
      ? [...actionableActivityCandidates].sort(compareGpuiPetOverlayActivityCandidates).slice(0, 3)
      : [...candidates].sort(compareGpuiSessionStatusIndicatorCandidates).slice(0, 2);
  return {
    activities: shownActivityCandidates.map((candidate) => ({
      id: candidate.sessionId,
      projectId: candidate.projectId,
      state: candidate.status,
      title: candidate.title,
    })),
    enabled: settings.petOverlayEnabled,
    selectedPetId: boundedGpuiStatusIndicatorTitle(settings.selectedPetId, "cat"),
    statusItems: createGpuiPetOverlayStatusItems(candidates),
    type: GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE,
    version: GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION,
  };
}

function createGpuiSessionStatusIndicatorProjects(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
): GpuiSessionStatusIndicatorProject[] {
  const projects: GpuiSessionStatusIndicatorProject[] = [];
  const projectsById = new Map<string, GpuiSessionStatusIndicatorProject>();
  for (const candidate of candidates) {
    if (!shouldCountGpuiSessionStatusIndicatorCandidate(candidate)) {
      continue;
    }
    let project = projectsById.get(candidate.projectId);
    if (!project) {
      if (projects.length >= GPUI_STATUS_INDICATOR_MAX_PROJECTS) {
        continue;
      }
      project = {
        ...(candidate.iconDataUrl ? { iconDataUrl: candidate.iconDataUrl } : {}),
        projectId: candidate.projectId,
        sessions: [],
        title: candidate.projectTitle,
      };
      projectsById.set(candidate.projectId, project);
      projects.push(project);
    }
    if (project.sessions.length >= GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT) {
      continue;
    }
    project.sessions.push({
      lastActiveAt: candidate.lastInteractionAt,
      sessionId: candidate.sessionId,
      sidebarOrder: candidate.order,
      status: candidate.status,
      title: candidate.title,
    });
  }
  return projects;
}

function countGpuiSessionStatusIndicatorCandidates(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
): Record<GpuiSessionStatusIndicatorStatus, number> {
  const counts = {
    attention: 0,
    available: 0,
    working: 0,
  };
  for (const candidate of candidates) {
    if (shouldCountGpuiSessionStatusIndicatorCandidate(candidate)) {
      counts[candidate.status] += 1;
    }
  }
  return counts;
}

function createGpuiPetOverlayStatusItems(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
): Array<{ count: number; status: GpuiSessionStatusIndicatorStatus }> {
  const counts = countGpuiSessionStatusIndicatorCandidates(candidates);
  if (counts.attention > 0 || counts.working > 0) {
    const items: Array<{ count: number; status: GpuiSessionStatusIndicatorStatus }> = [];
    if (counts.attention > 0) {
      items.push({ count: counts.attention, status: "attention" });
    }
    if (counts.working > 0) {
      items.push({ count: counts.working, status: "working" });
    }
    return items;
  }
  return counts.available > 0 ? [{ count: counts.available, status: "available" }] : [];
}

function getGpuiSessionStatusIndicatorStatus(
  session: SidebarSessionItem,
): GpuiSessionStatusIndicatorStatus {
  if (session.activity === "attention") {
    return "attention";
  }
  if (session.activity === "working") {
    return "working";
  }
  return "available";
}

function hasRunningZmxBackingForGpuiIdleIndicator(session: SidebarSessionItem): boolean {
  if (session.sessionKind !== "terminal") {
    return false;
  }
  if (
    session.sessionPersistenceProvider !== "zmx" ||
    !normalizeNonEmptyString(session.sessionPersistenceName)
  ) {
    return false;
  }
  return (
    session.providerSessionState === "exists" ||
    session.nativePaneState === "mounted" ||
    session.nativePaneState === "mounting" ||
    session.isLive === true
  );
}

function shouldCountGpuiSessionStatusIndicatorCandidate(
  candidate: GpuiSessionStatusIndicatorCandidate,
): boolean {
  return candidate.status !== "available" || candidate.hasRunningZmxBacking;
}

function compareGpuiSessionStatusIndicatorCandidates(
  left: GpuiSessionStatusIndicatorCandidate,
  right: GpuiSessionStatusIndicatorCandidate,
): number {
  const timeDelta =
    getGpuiIndicatorTimestamp(right.lastInteractionAt) -
    getGpuiIndicatorTimestamp(left.lastInteractionAt);
  if (timeDelta !== 0) {
    return timeDelta;
  }
  return left.order - right.order;
}

function compareGpuiPetOverlayActivityCandidates(
  left: GpuiSessionStatusIndicatorCandidate,
  right: GpuiSessionStatusIndicatorCandidate,
): number {
  const statusDelta =
    getGpuiPetOverlayActivityStatusPriority(right.status) -
    getGpuiPetOverlayActivityStatusPriority(left.status);
  if (statusDelta !== 0) {
    return statusDelta;
  }
  return left.order - right.order;
}

function getGpuiPetOverlayActivityStatusPriority(status: GpuiSessionStatusIndicatorStatus): number {
  switch (status) {
    case "attention":
      return 2;
    case "working":
      return 1;
    case "available":
      return 0;
  }
}

function getGpuiIndicatorTimestamp(value: string | undefined): number {
  if (!value) {
    return 0;
  }
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? timestamp : 0;
}

function getGpuiPetOverlaySessionTitle(session: SidebarSessionItem): string {
  const title =
    session.displayTitle?.trim() ||
    session.primaryTitle?.trim() ||
    session.terminalTitle?.trim() ||
    session.alias.trim() ||
    session.sessionNumber?.trim();
  return boundedGpuiStatusIndicatorTitle(title, "Untitled session");
}

function boundedGpuiStatusIndicatorTitle(value: string | undefined, fallback: string): string {
  const normalized = normalizeNonEmptyString(value) ?? fallback;
  return normalized.length > GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS
    ? normalized.slice(0, GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS)
    : normalized;
}

function boundedGpuiActiveWorkspaceTabSessionTitle(value: string): string {
  const normalized = normalizeNonEmptyString(value) ?? DEFAULT_TERMINAL_SESSION_TITLE;
  return normalized.length > GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS
    ? normalized.slice(0, GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS)
    : normalized;
}

function gpuiAgentGuiTitle(value: string | undefined): string | undefined {
  const normalized = value?.trim().toLocaleLowerCase();
  return normalized === "agent gui" || normalized === "t3 code" || normalized === "t3 code (alpha)"
    ? "Chat"
    : value;
}

function normalizeGpuiStatusPetActivation(
  value: unknown,
): GpuiStatusPetActivationPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !["sessionId", "type", "version"].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (!sessionId || !gpuiStatusPetActivationSessionIdAllowed(sessionId)) {
    return undefined;
  }
  return { sessionId };
}

function normalizeGpuiMenuBarProjectActivation(
  value: unknown,
): GpuiMenuBarProjectActivationPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !["projectId", "type", "version"].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  if (!projectId || !gpuiStatusPetActivationSessionIdAllowed(projectId)) {
    return undefined;
  }
  return { projectId };
}

function normalizeGpuiMenuBarSessionActivation(
  value: unknown,
): GpuiMenuBarSessionActivationPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !["projectId", "sessionId", "type", "version"].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiStatusPetActivationSessionIdAllowed(projectId) ||
    !gpuiStatusPetActivationSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

type GpuiCreatedProjectAgentSessionRecord = {
  agentSessionId?: string;
  agentSessionPath?: string;
  projectId: string;
  sessionId: string;
  zmxName?: string;
};

type GpuiProjectBoardConversationRequest = {
  action:
    | "appendDebugLog"
    | "associateFocusedSession"
    | "getState"
    | "jumpToConversation"
    | "showToast"
    | "startWork"
    | "unlinkConversation";
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

const GPUI_PROJECT_BOARD_CONVERSATION_ACTIONS = new Set<string>([
  "appendDebugLog",
  "associateFocusedSession",
  "getState",
  "jumpToConversation",
  "showToast",
  "startWork",
  "unlinkConversation",
]);

function boundedGpuiProjectBoardRequestString(
  value: unknown,
  maxChars: number,
): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > maxChars) {
    return undefined;
  }
  return trimmed;
}

function normalizeGpuiProjectBoardConversationRequest(
  payload: unknown,
): GpuiProjectBoardConversationRequest | undefined {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  if (
    record.type !== GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION ||
    !record.request ||
    typeof record.request !== "object" ||
    Array.isArray(record.request)
  ) {
    return undefined;
  }
  const request = record.request as Record<string, unknown>;
  const requestId = boundedGpuiProjectBoardRequestString(request.requestId, 256);
  const action = typeof request.action === "string" ? request.action : "";
  if (!requestId || !GPUI_PROJECT_BOARD_CONVERSATION_ACTIONS.has(action)) {
    return undefined;
  }
  return {
    action: action as GpuiProjectBoardConversationRequest["action"],
    agentId: boundedGpuiProjectBoardRequestString(request.agentId, 256),
    beadDisplayId: boundedGpuiProjectBoardRequestString(request.beadDisplayId, 256),
    beadId: boundedGpuiProjectBoardRequestString(request.beadId, 512),
    projectId: boundedGpuiProjectBoardRequestString(request.projectId, 512),
    projectPath: boundedGpuiProjectBoardRequestString(request.projectPath, 4096),
    prompt: boundedGpuiProjectBoardRequestString(request.prompt, 60_000),
    requestId,
    sessionId: boundedGpuiProjectBoardRequestString(request.sessionId, 512),
    startLocation: boundedGpuiProjectBoardRequestString(request.startLocation, 32),
    toastDescription: boundedGpuiProjectBoardRequestString(request.toastDescription, 2_000),
    toastLevel: boundedGpuiProjectBoardRequestString(request.toastLevel, 16),
    toastTitle: boundedGpuiProjectBoardRequestString(request.toastTitle, 300),
  };
}

function normalizeGpuiProjectBoardToastLevel(level: string | undefined): AppToastLevel {
  switch (level) {
    case "error":
    case "info":
    case "success":
    case "warning":
      return level;
    default:
      return "error";
  }
}

function normalizeGpuiCommandPaletteSessionFocus(value: unknown): string | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !["sessionId", "type", "version"].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (!sessionId) {
    return undefined;
  }
  // Palette rows only ever carry projected sidebar ids: combined local
  // project-session ids or remote presentation ids. Raw daemon ids are not
  // routable from the palette and are rejected.
  if (
    !parseGpuiRemotePresentationSessionId(sessionId) &&
    !parseGxserverPresentationProjectSessionId(sessionId)
  ) {
    return undefined;
  }
  return sessionId;
}

/*
 * CDXC:GlobalActions 2026-08-01:
 * The tab strip runs Global Actions and the Command Palette runs Project
 * Actions, and both send only an id. Without a scope on the selector the two
 * id spaces are indistinguishable, so a Global Action whose id also exists as a
 * project action would launch the project one. Scope is optional and absent
 * means project, which keeps every existing palette sender unchanged.
 */
function normalizeGpuiCommandPaletteRunSidebarCommand(
  value: unknown,
):
  | {
      message: Extract<SidebarToExtensionMessage, { type: "runSidebarCommand" }>;
      scope: SidebarCommandScope;
    }
  | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) => !["commandId", "runMode", "scope", "type", "version"].includes(key),
    )
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const commandId = normalizeNonEmptyString(record.commandId)?.trim();
  if (!commandId) {
    return undefined;
  }
  let scope: SidebarCommandScope = "project";
  if (Object.prototype.hasOwnProperty.call(record, "scope")) {
    if (record.scope !== "global" && record.scope !== "project") {
      return undefined;
    }
    scope = record.scope;
  }
  if (!Object.prototype.hasOwnProperty.call(record, "runMode")) {
    return {
      message: {
        commandId,
        type: "runSidebarCommand",
      },
      scope,
    };
  }
  if (!isSidebarCommandRunMode(record.runMode)) {
    return undefined;
  }
  return {
    message: {
      commandId,
      runMode: record.runMode,
      type: "runSidebarCommand",
    },
    scope,
  };
}

function normalizeGpuiWorkspaceTabSessionSelection(
  value: unknown,
): GpuiWorkspaceTabSessionSelectionPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) =>
        ![
          "localRuntimeMissing",
          "localWasSleeping",
          "projectId",
          "sessionId",
          "type",
          "version",
          "visibleSessionIds",
        ].includes(key),
    )
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiStatusPetActivationSessionIdAllowed(projectId) ||
    !gpuiStatusPetActivationSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if (record.localWasSleeping !== undefined && record.localWasSleeping !== true) {
    return undefined;
  }
  if (record.localRuntimeMissing !== undefined && record.localRuntimeMissing !== true) {
    return undefined;
  }
  const visibleSessionIds = Array.isArray(record.visibleSessionIds)
    ? uniqueNonEmptyStrings(record.visibleSessionIds)?.filter((visibleSessionId) =>
        gpuiStatusPetActivationSessionIdAllowed(visibleSessionId),
      )
    : undefined;
  if (
    record.visibleSessionIds !== undefined &&
    (!Array.isArray(record.visibleSessionIds) ||
      visibleSessionIds?.length !== record.visibleSessionIds.length ||
      visibleSessionIds.length > 64)
  ) {
    return undefined;
  }
  return {
    ...(record.localRuntimeMissing === true ? { localRuntimeMissing: true } : {}),
    ...(record.localWasSleeping === true ? { localWasSleeping: true } : {}),
    projectId,
    sessionId,
    ...(visibleSessionIds ? { visibleSessionIds } : {}),
  };
}

type GpuiWorkspaceTerminalBellPayload = {
  projectId: string;
  sessionId: string;
};

type GpuiWorkspaceTerminalEscapePressedPayload = {
  projectId: string;
  sessionId: string;
};

type GpuiWorkspaceFirstPromptTitleGenerationCancelPayload = {
  projectId: string;
  sessionId: string;
};

type GpuiWorkspaceSessionAttentionAcknowledgePayload = {
  projectId: string;
  sessionId: string;
};

type GpuiSessionAttentionAcknowledgeReason = "native-focus" | "sidebar-focus" | "terminal-escape";

type GpuiSessionAttentionTarget =
  | {
      kind: "local";
      projectId: string;
      sessionId: string;
    }
  | {
      kind: "remote";
      machineId: string;
      projectId: string;
      sessionId: string;
    };

type GpuiWorkspaceTerminalRuntimeActionPayload =
  | {
      action: "forkSession" | "fullReloadSession";
      projectId: string;
      sessionId: string;
    }
  | { action: "sleepAllDaemonSessions" }
  | { action: "sleepInactiveSessions" };

function normalizeGpuiWorkspaceTerminalRuntimeAction(
  value: unknown,
): GpuiWorkspaceTerminalRuntimeActionPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) => !["action", "projectId", "sessionId", "type", "version"].includes(key),
    )
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  if (record.action === "sleepInactiveSessions" || record.action === "sleepAllDaemonSessions") {
    if (record.projectId !== undefined || record.sessionId !== undefined) {
      return undefined;
    }
    return { action: record.action };
  }
  const action =
    record.action === "forkSession" || record.action === "fullReloadSession"
      ? record.action
      : undefined;
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !action ||
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { action, projectId, sessionId };
}

function normalizeGpuiWorkspaceTerminalBell(
  value: unknown,
): GpuiWorkspaceTerminalBellPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !["projectId", "sessionId", "type", "version"].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

function normalizeGpuiWorkspaceTerminalEscapePressed(
  value: unknown,
): GpuiWorkspaceTerminalEscapePressedPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !["projectId", "sessionId", "type", "version"].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

function normalizeGpuiWorkspaceFirstPromptTitleGenerationCancel(
  value: unknown,
): GpuiWorkspaceFirstPromptTitleGenerationCancelPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !["projectId", "sessionId", "type", "version"].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

function normalizeGpuiWorkspaceSessionAttentionAcknowledge(
  value: unknown,
): GpuiWorkspaceSessionAttentionAcknowledgePayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !["projectId", "sessionId", "type", "version"].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

function normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest(
  value: unknown,
): GpuiWorkspaceTerminalLifecycleRequest | undefined {
  /*
  CDXC:GPUIWorkspaceLifecycle 2026-06-26-05:23:
  Lifecycle retries may contain either the raw fixed bridge payload queued before React started or the runtime's already-normalized id-only request queued while the CEF result bridge was missing. Accept only those two bounded shapes so retries do not reintroduce paths, commands, terminal text, URLs, tokens, or generic IPC fields.
  */
  return (
    normalizeGpuiWorkspaceTerminalLifecycleRequest(value) ??
    normalizeGpuiWorkspaceTerminalLifecycleQueuedRequest(value)
  );
}

function normalizeGpuiWorkspaceTerminalLifecycleQueuedRequest(
  value: unknown,
): GpuiWorkspaceTerminalLifecycleRequest | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) =>
        ![
          "action",
          "projectId",
          "replacementProjectId",
          "replacementSessionId",
          "requestId",
          "sessionId",
          "skipReplacementFallback",
        ].includes(key),
    )
  ) {
    return undefined;
  }
  if (
    typeof record.requestId !== "number" ||
    !Number.isSafeInteger(record.requestId) ||
    record.requestId <= 0
  ) {
    return undefined;
  }
  const action =
    record.action === "close" || record.action === "sleep" || record.action === "wake"
      ? record.action
      : undefined;
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  const replacementProjectId = normalizeNonEmptyString(record.replacementProjectId)?.trim();
  const replacementSessionId = normalizeNonEmptyString(record.replacementSessionId)?.trim();
  if (
    !action ||
    !projectId ||
    !sessionId ||
    (record.skipReplacementFallback !== true && record.skipReplacementFallback !== false) ||
    !gpuiWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if (
    (replacementProjectId && !replacementSessionId) ||
    (!replacementProjectId && replacementSessionId)
  ) {
    return undefined;
  }
  if (record.skipReplacementFallback === true && replacementProjectId && replacementSessionId) {
    return undefined;
  }
  if (
    replacementProjectId &&
    replacementSessionId &&
    (!gpuiWorkspaceLifecycleProjectIdAllowed(replacementProjectId) ||
      !gpuiLocalWorkspaceLifecycleSessionIdAllowed(replacementSessionId))
  ) {
    return undefined;
  }
  return {
    action,
    projectId,
    ...(replacementProjectId && replacementSessionId
      ? { replacementProjectId, replacementSessionId }
      : {}),
    requestId: record.requestId,
    sessionId,
    skipReplacementFallback: record.skipReplacementFallback,
  };
}

function normalizeGpuiWorkspaceTerminalLifecycleRequest(
  value: unknown,
): GpuiWorkspaceTerminalLifecycleRequest | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) =>
        ![
          "action",
          "projectId",
          "replacementProjectId",
          "replacementSessionId",
          "requestId",
          "sessionId",
          "skipReplacementFallback",
          "type",
          "version",
        ].includes(key),
    )
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION ||
    typeof record.requestId !== "number" ||
    !Number.isSafeInteger(record.requestId) ||
    record.requestId <= 0
  ) {
    return undefined;
  }
  const action =
    record.action === "close" || record.action === "sleep" || record.action === "wake"
      ? record.action
      : undefined;
  if (!action) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  const replacementProjectId = normalizeNonEmptyString(record.replacementProjectId)?.trim();
  const replacementSessionId = normalizeNonEmptyString(record.replacementSessionId)?.trim();
  const skipReplacementFallback =
    record.skipReplacementFallback === undefined ? false : record.skipReplacementFallback === true;
  if (
    !projectId ||
    !sessionId ||
    !gpuiWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if (record.skipReplacementFallback !== undefined && record.skipReplacementFallback !== true) {
    return undefined;
  }
  if (
    (replacementProjectId && !replacementSessionId) ||
    (!replacementProjectId && replacementSessionId)
  ) {
    return undefined;
  }
  if (skipReplacementFallback && replacementProjectId && replacementSessionId) {
    return undefined;
  }
  if (
    replacementProjectId &&
    replacementSessionId &&
    (!gpuiWorkspaceLifecycleProjectIdAllowed(replacementProjectId) ||
      !gpuiLocalWorkspaceLifecycleSessionIdAllowed(replacementSessionId))
  ) {
    return undefined;
  }
  return {
    action,
    projectId,
    ...(replacementProjectId && replacementSessionId
      ? { replacementProjectId, replacementSessionId }
      : {}),
    requestId: record.requestId,
    sessionId,
    skipReplacementFallback,
  };
}

function didGpuiGxserverProviderTransitionCommit(result: GxserverSessionTransitionResult): boolean {
  /*
  CDXC:GPUIWorkspaceLifecycle 2026-06-26-08:01:
  GPUI sleep must match macOS gxserver lifecycle ownership: `/api/transitionSession` resolving is not proof that zmx stopped. Only publish local sleep state after the returned session lifecycle matches the action, provider lifecycle is `missing`, and the optional kill result did not explicitly fail.
  */
  if (!isObjectRecord(result) || !isObjectRecord(result.session)) {
    return false;
  }
  const providerState = result.session.providerState;
  if (!isObjectRecord(providerState)) {
    return false;
  }
  const expectedLifecycleState = result.action === "sleep" ? "sleeping" : "stopped";
  const killSucceeded = readGpuiTransitionKillSucceeded(
    isObjectRecord(result.transition) ? result.transition : undefined,
  );
  return (
    result.session.lifecycleState === expectedLifecycleState &&
    providerState.lifecycleState === "missing" &&
    killSucceeded !== false
  );
}

function shouldApplyGpuiLocalWorkspaceTransition(
  result: GxserverSessionTransitionResult,
  action: "close" | "sleep",
): boolean {
  /*
  CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:44:
  macOS close and sleep intentionally diverge after gxserver handles a provider transition. Close removes the local pane/sidebar row once `/api/transitionSession` returns a valid close result, even when provider kill did not commit; sleep must stay strict so GPUI does not show a cold sleeping placeholder while the zmx runtime is still live.
  */
  if (!isObjectRecord(result) || result.action !== action || !isObjectRecord(result.session)) {
    return false;
  }
  return action === "close" || didGpuiGxserverProviderTransitionCommit(result);
}

function readGpuiTransitionKillSucceeded(
  transition: Record<string, unknown> | undefined,
): boolean | undefined {
  const kill = transition?.kill;
  if (!isObjectRecord(kill)) {
    return undefined;
  }
  return typeof kill.killed === "boolean" ? kill.killed : undefined;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function gpuiWorkspaceLifecycleProjectIdAllowed(value: string): boolean {
  return (
    gpuiLocalWorkspaceLifecycleProjectIdAllowed(value) ||
    parseGpuiRemotePresentationProjectId(value) !== undefined
  );
}

function gpuiLocalWorkspaceLifecycleProjectIdAllowed(value: string): boolean {
  return /^P[0-9][a-z0-9]{0,30}$/u.test(value);
}

function gpuiLocalWorkspaceLifecycleSessionIdAllowed(value: string): boolean {
  return (
    gpuiStatusPetActivationSessionIdAllowed(value) &&
    !value.includes(":") &&
    !parseGpuiRemotePresentationSessionId(value) &&
    !parseGxserverPresentationProjectSessionId(value)
  );
}

function gpuiMenuBarStatusSessionFocusRoutingId(projectId: string, sessionId: string): string {
  if (
    parseGpuiRemotePresentationSessionId(sessionId) ||
    parseGxserverPresentationProjectSessionId(sessionId)
  ) {
    return sessionId;
  }
  const remoteProject = parseGpuiRemotePresentationProjectId(projectId);
  if (remoteProject) {
    return createGpuiRemotePresentationSessionId(
      remoteProject.machineId,
      remoteProject.projectId,
      sessionId,
    );
  }
  return createGxserverPresentationProjectSessionId(projectId, sessionId);
}

function gpuiStatusPetActivationSessionIdAllowed(value: string): boolean {
  return (
    value.length <= GPUI_STATUS_INDICATOR_ID_MAX_CHARS &&
    !value.includes("/") &&
    !value.includes("\\") &&
    !/[\u0000-\u001f\u007f]/u.test(value)
  );
}

function createGpuiRecentProjects(
  recentProjects: readonly GxserverRecentProjectDomainState[],
  settings: ghostexSettings,
): SidebarRecentProject[] {
  return recentProjects
    .flatMap((project) => {
      const projectId = typeof project.projectId === "string" ? project.projectId.trim() : "";
      const title = typeof project.title === "string" ? project.title.trim() : "";
      const path = normalizeGpuiProjectPath(project.path);
      if (!projectId || !title || !path) {
        return [];
      }
      const icon = normalizeWorkspaceProjectIcon(project.icon);
      const iconDataUrl = normalizeWorkspaceProjectIconDataUrl(project.iconDataUrl);
      const theme =
        normalizeGpuiSidebarTheme(project.theme) ??
        resolveSidebarTheme(settings.sidebarTheme, "dark");
      const themeColor = normalizeWorkspaceThemeColor(project.themeColor);
      const recentClosedAt =
        typeof project.recentClosedAt === "string" && project.recentClosedAt.trim().length > 0
          ? project.recentClosedAt.trim()
          : undefined;
      return [
        {
          ...(icon ? { icon } : {}),
          ...(iconDataUrl ? { iconDataUrl } : {}),
          ...(recentClosedAt ? { recentClosedAt } : {}),
          ...(themeColor ? { themeColor } : {}),
          path,
          projectId,
          sessionCount: Number.isFinite(project.sessionCount)
            ? Math.max(0, Math.floor(project.sessionCount))
            : 0,
          theme,
          title,
        },
      ];
    })
    .sort(compareGpuiRecentProjectsByClosedAt);
}

function createGpuiRemoteRecentProjects(
  recentProjectsByMachineId:
    ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]> | undefined,
  presentationsByMachineId: ReadonlyMap<string, GxserverPresentationSnapshot> | undefined,
  settings: ghostexSettings,
): SidebarRecentProject[] {
  /*
  CDXC:GPUIRemoteProjects 2026-06-27-19:37:
  Remote Recent Projects are GPUI-client-local parking rows. Keep ids
  machine-scoped and reconcile display fields from a live remote presentation
  when connected, but do not call the remote daemon's recent endpoints or share
  the parked state with the macOS app.
  */
  if (!recentProjectsByMachineId) {
    return [];
  }
  const remoteMachinesById = new Map(
    settings.remoteMachines.map((machine) => [machine.id, machine]),
  );
  return [...recentProjectsByMachineId.entries()].flatMap(([machineId, recentProjects]) => {
    const machine = remoteMachinesById.get(machineId);
    if (!machine) {
      return [];
    }
    const presentation = presentationsByMachineId?.get(machineId);
    return recentProjects.flatMap((project) => {
      const projectId = typeof project.projectId === "string" ? project.projectId.trim() : "";
      const presentationProject = presentation?.projects.find(
        (candidate) => candidate.projectId === projectId,
      );
      if (presentation && !presentationProject) {
        return [];
      }
      const title =
        presentationProject?.title.trim() ||
        (typeof project.title === "string" ? project.title.trim() : "");
      const path = normalizeGpuiProjectPath(presentationProject?.path ?? project.path);
      if (!projectId || !title || !path) {
        return [];
      }
      const icon = normalizeWorkspaceProjectIcon(project.icon);
      const iconDataUrl = normalizeWorkspaceProjectIconDataUrl(project.iconDataUrl);
      const theme =
        normalizeGpuiSidebarTheme(project.theme) ??
        resolveSidebarTheme(settings.sidebarTheme, "dark");
      const themeColor = normalizeWorkspaceThemeColor(project.themeColor);
      const recentClosedAt =
        typeof project.recentClosedAt === "string" && project.recentClosedAt.trim().length > 0
          ? project.recentClosedAt.trim()
          : undefined;
      return [
        {
          ...(icon ? { icon } : {}),
          ...(iconDataUrl ? { iconDataUrl } : {}),
          ...(recentClosedAt ? { recentClosedAt } : {}),
          ...(themeColor ? { themeColor } : {}),
          path,
          projectId: createGpuiRemotePresentationProjectId(machineId, projectId),
          remoteMachineId: machineId,
          remoteMachineName: machine.name || "Remote",
          sessionCount: presentation
            ? countGpuiRemotePresentationProjectSessions(presentation, projectId)
            : Number.isFinite(project.sessionCount)
              ? Math.max(0, Math.floor(project.sessionCount))
              : 0,
          theme,
          title,
        },
      ];
    });
  });
}

/*
CDXC:RemoteGroupReorder 2026-07-12:
Per-machine remote project group order is app-client presentation state, like
the remote recent-projects list: the remote gxserver keeps publishing its own
group order and this map only reorders the projection locally. Persist only
machine ids and remote project ids.
*/
function readStoredGpuiRemoteGroupOrder(): Map<string, string[]> {
  try {
    const raw: unknown = JSON.parse(
      localStorage.getItem(GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY) ?? "{}",
    );
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
      return new Map();
    }
    const next = new Map<string, string[]>();
    for (const [machineId, order] of Object.entries(raw)) {
      if (!machineId.trim() || !Array.isArray(order)) {
        continue;
      }
      const projectIds = order.filter(
        (projectId): projectId is string =>
          typeof projectId === "string" && projectId.trim().length > 0,
      );
      if (projectIds.length > 0) {
        next.set(machineId, projectIds);
      }
    }
    return next;
  } catch {
    return new Map();
  }
}

function writeStoredGpuiRemoteGroupOrder(
  orderByMachineId: ReadonlyMap<string, readonly string[]>,
): void {
  try {
    localStorage.setItem(
      GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY,
      JSON.stringify(Object.fromEntries(orderByMachineId)),
    );
  } catch {
    // CEF storage may be unavailable in tests or early bootstrap; the in-memory order still drives this session.
  }
}

function readStoredGpuiRemoteLastSeenPresentations(): Map<string, GxserverPresentationSnapshot> {
  try {
    const raw: unknown = JSON.parse(
      localStorage.getItem(GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY) ?? "{}",
    );
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
      return new Map();
    }
    const next = new Map<string, GxserverPresentationSnapshot>();
    for (const [machineId, snapshot] of Object.entries(raw)) {
      if (!machineId.trim() || !isPresentationSnapshot(snapshot)) {
        continue;
      }
      next.set(machineId, snapshot);
    }
    return next;
  } catch {
    return new Map();
  }
}

function writeStoredGpuiRemoteLastSeenPresentations(
  presentationsByMachineId: ReadonlyMap<string, GxserverPresentationSnapshot>,
): void {
  /*
  CDXC:GPUIRemoteLastSeen 2026-07-12:
  Last-seen remote presentations are the same sanitized snapshots the sidebar
  already renders (project titles/paths, session titles, states). Persisting
  them app-client-locally lets disconnected machines keep their faded project
  view across restarts; no tokens, SSH details, or daemon internals exist in
  these snapshots.
  */
  try {
    localStorage.setItem(
      GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY,
      JSON.stringify(Object.fromEntries(presentationsByMachineId)),
    );
  } catch {
    // CEF storage may be unavailable in tests or early bootstrap; the in-memory copy still drives this session.
  }
}

function readStoredGpuiRemoteRecentProjects(): Map<string, GxserverRecentProjectDomainState[]> {
  try {
    return groupGpuiRemoteRecentProjectsByMachine(
      normalizeStoredGpuiRemoteRecentProjects(
        JSON.parse(localStorage.getItem(GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY) ?? "[]"),
      ),
    );
  } catch {
    return new Map();
  }
}

function writeStoredGpuiRemoteRecentProjects(
  projectsByMachineId: ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]>,
): void {
  try {
    const rows = [...projectsByMachineId.entries()].flatMap(([machineId, projects]) =>
      projects.flatMap((project) => {
        const projectId = typeof project.projectId === "string" ? project.projectId.trim() : "";
        const title = typeof project.title === "string" ? project.title.trim() : "";
        const path = typeof project.path === "string" ? project.path.trim() : "";
        if (!machineId.trim() || !projectId || !title) {
          return [];
        }
        return [
          {
            machineId: machineId.trim(),
            path,
            projectId,
            recentClosedAt:
              typeof project.recentClosedAt === "string" ? project.recentClosedAt : undefined,
            sessionCount: Number.isFinite(project.sessionCount)
              ? Math.max(0, Math.floor(project.sessionCount))
              : 0,
            title,
          },
        ];
      }),
    );
    /*
    CDXC:GPUIRemoteProjects 2026-06-27-19:37:
    GPUI remote recent rows are app-client state. Persist only machine id,
    remote project id, title/path needed for the disconnected drawer, timestamp,
    and count; do not persist tokens, SSH hosts, usernames, command text,
    terminal output, or local gxserver project rows.
    */
    localStorage.setItem(GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY, JSON.stringify(rows));
  } catch {
    // CEF storage may be unavailable in tests or early bootstrap; the in-memory rows still drive this session.
  }
}

function normalizeStoredGpuiRemoteRecentProjects(
  value: unknown,
): Array<{ machineId: string; project: GxserverRecentProjectDomainState }> {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.flatMap((candidate) => {
    if (!candidate || typeof candidate !== "object") {
      return [];
    }
    const record = candidate as Record<string, unknown>;
    const machineId = normalizeNonEmptyString(record.machineId);
    const projectId = normalizeNonEmptyString(record.projectId);
    const title = normalizeNonEmptyString(record.title);
    if (!machineId || !projectId || !title) {
      return [];
    }
    const path = typeof record.path === "string" ? record.path.trim() : "";
    const recentClosedAt =
      typeof record.recentClosedAt === "string" &&
      record.recentClosedAt.trim().length > 0 &&
      Number.isFinite(Date.parse(record.recentClosedAt))
        ? record.recentClosedAt.trim()
        : undefined;
    const sessionCount = Number(record.sessionCount);
    return [
      {
        machineId,
        project: {
          path,
          projectId: projectId as GxserverProjectId,
          ...(recentClosedAt ? { recentClosedAt } : {}),
          sessionCount:
            Number.isFinite(sessionCount) && sessionCount > 0 ? Math.floor(sessionCount) : 0,
          title,
        },
      },
    ];
  });
}

/*
CDXC:GPUIRemoteProjects 2026-06-27-21:59:
The GPUI start build runs through Vite/Rolldown, whose transformer accepts readonly array shorthand and ReadonlyArray<T> but rejects `readonly Array<T>`. Keep this helper input in ReadonlyArray<T> form so Remote Recent Projects packaging does not break local GPUI startup.
*/
function groupGpuiRemoteRecentProjectsByMachine(
  rows: ReadonlyArray<{ machineId: string; project: GxserverRecentProjectDomainState }>,
): Map<string, GxserverRecentProjectDomainState[]> {
  const projectsByMachineId = new Map<string, GxserverRecentProjectDomainState[]>();
  for (const row of rows) {
    projectsByMachineId.set(
      row.machineId,
      orderGpuiRecentProjects([
        row.project,
        ...(projectsByMachineId.get(row.machineId) ?? []).filter(
          (project) => project.projectId !== row.project.projectId,
        ),
      ]),
    );
  }
  return projectsByMachineId;
}

function orderGpuiRecentProjects(
  projects: readonly GxserverRecentProjectDomainState[],
): GxserverRecentProjectDomainState[] {
  return [...projects].sort(
    (left, right) => Date.parse(right.recentClosedAt ?? "") - Date.parse(left.recentClosedAt ?? ""),
  );
}

function countGpuiRemotePresentationProjectSessions(
  presentation: GxserverPresentationSnapshot,
  projectId: string,
): number {
  return presentation.sessions.filter(
    (session) =>
      session.projectId === projectId &&
      session.visibleInSidebarByDefault === true &&
      session.surface !== "commands",
  ).length;
}

function normalizeGpuiSidebarTheme(value: unknown): SidebarTheme | undefined {
  if (value === "plain-dark") {
    return "dark-2";
  }
  return GPUI_SIDEBAR_THEME_VALUES.has(value as SidebarTheme) ? (value as SidebarTheme) : undefined;
}

const GPUI_SIDEBAR_THEME_VALUES = new Set<SidebarTheme>([
  "dark-1",
  "dark-2",
  "plain-dark",
  "plain-light",
  "dark-green",
  "dark-blue",
  "dark-red",
  "dark-pink",
  "dark-orange",
  "light-blue",
  "light-green",
  "light-pink",
  "light-orange",
]);

function compareGpuiRecentProjectsByClosedAt(
  left: SidebarRecentProject,
  right: SidebarRecentProject,
): number {
  /*
  CDXC:GPUIRecentProjects 2026-06-25-19:22:
  Native `compareRecentProjectsByClosedAt` only sorts parsed close time descending. The Recent Projects drawer contract does not include gxserver `updatedAt`, so GPUI must not invent title or id tie-breaks; stable sort preserves producer order for equal timestamps.
  */
  return gpuiRecentProjectClosedAtMillis(right) - gpuiRecentProjectClosedAtMillis(left);
}

function gpuiRecentProjectClosedAtMillis(project: SidebarRecentProject): number {
  const millis = Date.parse(project.recentClosedAt ?? "");
  return Number.isFinite(millis) ? millis : 0;
}

type GpuiPresentationProjectProjectionMetadata = {
  chatProjectIds: ReadonlySet<string>;
  hiddenProjectIds: ReadonlySet<string>;
  projectOverlays: readonly GxserverPresentationSidebarProjectOverlay[];
};

function createGpuiPresentationProjectProjectionMetadata({
  domainProjects,
  presentation,
  projectOrder,
  recentProjects,
}: {
  domainProjects: readonly GxserverProjectDomainState[];
  presentation: GxserverPresentationSnapshot;
  projectOrder?: readonly string[];
  recentProjects?: readonly GxserverRecentProjectDomainState[];
}): GpuiPresentationProjectProjectionMetadata {
  const chatProjectIds = new Set<string>();
  /*
  CDXC:GPUIRecentProjects 2026-06-27-19:37:
  GPUI must match the macOS sidebar split: parked Recent Projects belong only in the React Recent Projects drawer, never in the main Projects list. Hide ids from both the domain project flag and the authoritative `/api/listRecentProjects` endpoint so presentation snapshots cannot briefly resurrect parked projects as normal groups.
  */
  const hiddenProjectIds = new Set(
    (recentProjects ?? [])
      .map((project) => (typeof project.projectId === "string" ? project.projectId.trim() : ""))
      .filter((projectId) => projectId.length > 0),
  );
  const projectOverlaysById = new Map<string, GxserverPresentationSidebarProjectOverlay>();
  const domainProjectIds = new Set(domainProjects.map((project) => project.projectId));
  const orderIndexByProjectId = new Map(
    (projectOrder ?? []).map((projectId, index) => [projectId, index]),
  );
  const worktreeParentCandidates = createGpuiProjectWorktreeParentCandidates({
    domainProjects,
    presentation,
  });

  for (const project of domainProjects) {
    const isChatProject = isGpuiPresentationChatDomainProject(project);
    const isQuickProject = isGpuiPresentationQuickDomainProject(project);
    const iconDataUrl = gpuiPresentationProjectIconDataUrl(project);
    const icon = gpuiPresentationProjectIcon(project);
    const worktree = resolveGpuiProjectWorktreeParentMetadata(
      normalizeGpuiSidebarWorktreeMetadata(project.worktree),
      worktreeParentCandidates,
    );
    if (project.isRecentProject === true) {
      hiddenProjectIds.add(project.projectId);
    }
    if (isChatProject || isQuickProject) {
      chatProjectIds.add(project.projectId);
    }
    mergeGpuiPresentationProjectOverlay(projectOverlaysById, project.projectId, {
      ...(icon ? { icon } : {}),
      ...(iconDataUrl ? { iconDataUrl } : {}),
      ...(isChatProject ? { isChatProject } : {}),
      ...(isQuickProject ? { isQuickProject } : {}),
      ...optionalNumberField("orderIndex", orderIndexByProjectId.get(project.projectId)),
      ...(worktree ? { worktree } : {}),
    });
  }

  for (const project of presentation.projects) {
    const orderIndex = orderIndexByProjectId.get(project.projectId);
    const worktree = resolveGpuiProjectWorktreeParentMetadata(
      normalizeGpuiSidebarWorktreeMetadata(project.worktree),
      worktreeParentCandidates,
    );
    if (orderIndex !== undefined || worktree) {
      mergeGpuiPresentationProjectOverlay(projectOverlaysById, project.projectId, {
        ...optionalNumberField("orderIndex", orderIndex),
        ...(worktree ? { worktree } : {}),
      });
    }
    if (
      domainProjectIds.has(project.projectId) ||
      !isGpuiPresentationChatProjectPath(project.path)
    ) {
      continue;
    }
    chatProjectIds.add(project.projectId);
    mergeGpuiPresentationProjectOverlay(projectOverlaysById, project.projectId, {
      isChatProject: true,
      isQuickProject: true,
    });
  }

  return {
    chatProjectIds,
    hiddenProjectIds,
    projectOverlays: [...projectOverlaysById.values()],
  };
}

function mergeGpuiPresentationProjectOverlay(
  overlaysById: Map<string, GxserverPresentationSidebarProjectOverlay>,
  projectId: string,
  patch: Partial<Omit<GxserverPresentationSidebarProjectOverlay, "projectId">>,
): void {
  if (!overlaysById.has(projectId) && Object.values(patch).every((value) => value === undefined)) {
    return;
  }
  overlaysById.set(projectId, {
    ...overlaysById.get(projectId),
    ...patch,
    projectId,
  });
}

function createGpuiProjectWorktreeParentCandidates({
  domainProjects,
  presentation,
}: {
  domainProjects: readonly GxserverProjectDomainState[];
  presentation: GxserverPresentationSnapshot;
}): GpuiProjectWorktreeParentCandidate[] {
  return [
    ...presentation.projects.map((project) => ({
      name: project.title,
      path: project.path,
      projectId: project.projectId,
      worktree: project.worktree,
    })),
    ...domainProjects.map((project) => ({
      name: project.name,
      path: project.path,
      projectId: project.projectId,
      worktree: project.worktree,
    })),
  ];
}

function resolveGpuiProjectWorktreeParentMetadata(
  worktree: SidebarProjectWorktreeMetadata | undefined,
  candidates: readonly GpuiProjectWorktreeParentCandidate[],
): SidebarProjectWorktreeMetadata | undefined {
  if (!worktree) {
    return undefined;
  }
  const parentPath = normalizeGpuiPathForProjectComparison(worktree.parentProjectPath);
  const canonicalParent = candidates.find((candidate) => {
    if (candidate.projectId === worktree.parentProjectId || !candidate.path) {
      return false;
    }
    if (normalizeGpuiPathForProjectComparison(candidate.path) !== parentPath) {
      return false;
    }
    return !normalizeGpuiWorktreeParentProjectId(candidate.worktree);
  });
  if (!canonicalParent) {
    return worktree;
  }
  const canonicalParentPath = canonicalParent.path?.trim();
  return {
    ...worktree,
    parentProjectId: canonicalParent.projectId,
    parentProjectName: canonicalParent.name?.trim() || worktree.parentProjectName,
    parentProjectPath: canonicalParentPath || worktree.parentProjectPath,
  };
}

/*
CDXC:SidebarV2ProjectIcons 2026-07-29:
The TYPED project icon, from the same gxserver identity metadata as the image
data URL above it. Most Ghostex projects carry a Tabler glyph plus a color
rather than an uploaded image, so a sidebar that only receives `iconDataUrl`
shows almost every project a generic folder. Same sourcing rules apply: identity
metadata only, never inferred from paths, titles, sessions, or renderer state.
*/
function gpuiPresentationProjectIcon(
  project: GxserverProjectDomainState,
): WorkspaceProjectIcon | undefined {
  return normalizeWorkspaceProjectIcon(project.identityIcon?.icon);
}

function gpuiPresentationProjectIconDataUrl(
  project: GxserverProjectDomainState,
): string | undefined {
  /*
  CDXC:GPUISettingsNotifications 2026-06-26-07:22:
  Session-attention icon parity must source images only from gxserver project identity metadata already normalized for workspace project appearance. Do not infer icons from project paths, URLs, titles, sessions, browser favicons, logs, command output, or renderer-local state.
  */
  const identityIcon = project.identityIcon;
  if (!identityIcon) {
    return undefined;
  }
  const icon = normalizeWorkspaceProjectIcon(identityIcon.icon);
  if (icon?.kind === "image") {
    return icon.dataUrl;
  }
  return normalizeWorkspaceProjectIconDataUrl(identityIcon.iconDataUrl);
}

function isGpuiPresentationChatDomainProject(
  project: GxserverProjectDomainState | undefined,
): boolean {
  return (
    booleanFromRecord(project as Record<string, unknown> | undefined, "isChat") === true ||
    booleanFromRecord(project?.launchSettings, "isChat") === true ||
    isGpuiPresentationChatProjectPath(project?.path)
  );
}

function isGpuiPresentationQuickDomainProject(
  project: GxserverProjectDomainState | undefined,
): boolean {
  return (
    booleanFromRecord(project as Record<string, unknown> | undefined, "isQuick") === true ||
    booleanFromRecord(project?.launchSettings, "isQuick") === true ||
    isGpuiPresentationChatDomainProject(project)
  );
}

function isGpuiPresentationChatProjectPath(value: unknown): boolean {
  const path = normalizeGpuiProjectPath(value)?.replace(/\\/gu, "/").replace(/\/+$/u, "");
  if (!path) {
    return false;
  }
  /*
  CDXC:GPUISidebarProjectClassification 2026-06-24-22:51:
  Match macOS chat-project detection by storage root instead of display title. `~/ghostex/chats`, `~/.ghostex[-variant]/chats`, and host-provided Ghostex homes such as repo-local `.active/chats` are projectless Chats containers; arbitrary projects named "Chat ..." are not.
  */
  return (
    /(?:^|\/)(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path) ||
    /^~\/(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path)
  );
}

function createGpuiProjectSettingsProjects(
  domainProjects: readonly GxserverProjectDomainState[],
  presentation: GxserverPresentationSnapshot | undefined,
): SidebarProjectSettingsItem[] {
  if (domainProjects.length > 0) {
    return domainProjects.flatMap((project) => {
      const path = normalizeGpuiProjectPath(project.path);
      if (
        !path ||
        project.isRecentProject === true ||
        isGpuiPresentationQuickDomainProject(project)
      ) {
        return [];
      }
      return [
        {
          ...optionalGpuiProjectSettingsString(
            "beadsDirectory",
            stringFromRecord(project.projectBoardConfig, "beadsDirectory"),
          ),
          ...optionalGpuiProjectSettingsString(
            "beadsDisplayKey",
            stringFromRecord(project.projectBoardConfig, "beadsDisplayKey") ??
              stringFromRecord(project.gitConfig, "beadsDisplayKey"),
          ),
          ...optionalGpuiProjectSettingsString(
            "docsDirectory",
            stringFromRecord(project.projectBoardConfig, "docsDirectory"),
          ),
          name: project.name,
          path,
          projectId: project.projectId,
          ...optionalGpuiProjectSettingsString(
            "worktreeCommand",
            stringFromRecord(project.gitConfig, "worktreeCommand"),
          ),
          ...optionalGpuiProjectSettingsString(
            "worktreeParentProjectId",
            normalizeGpuiWorktreeParentProjectId(project.worktree),
          ),
        },
      ];
    });
  }
  return (presentation?.projects ?? []).flatMap((project) => {
    const path = normalizeGpuiProjectPath(project.path);
    if (!path || isGpuiPresentationChatProjectPath(path)) {
      return [];
    }
    return [
      {
        name: project.title,
        path,
        projectId: project.projectId,
        ...optionalGpuiProjectSettingsString(
          "worktreeParentProjectId",
          normalizeGpuiWorktreeParentProjectId(project.worktree),
        ),
      },
    ];
  });
}

function optionalGpuiProjectSettingsString<TKey extends keyof SidebarProjectSettingsItem>(
  key: TKey,
  value: string | undefined,
): Partial<Pick<SidebarProjectSettingsItem, TKey>> {
  return value ? ({ [key]: value } as Partial<Pick<SidebarProjectSettingsItem, TKey>>) : {};
}

function normalizeGpuiWorktreeParentProjectId(
  worktree: Record<string, unknown> | undefined,
): string | undefined {
  return stringFromRecord(worktree, "parentProjectId");
}

function normalizeGpuiSidebarWorktreeMetadata(
  worktree: Record<string, unknown> | undefined,
): SidebarProjectWorktreeMetadata | undefined {
  const branch = stringFromRecord(worktree, "branch");
  const name = stringFromRecord(worktree, "name");
  const parentProjectId = normalizeGpuiWorktreeParentProjectId(worktree);
  const parentProjectName = stringFromRecord(worktree, "parentProjectName");
  const parentProjectPath = stringFromRecord(worktree, "parentProjectPath");
  if (!branch || !name || !parentProjectId || !parentProjectName || !parentProjectPath) {
    return undefined;
  }
  const createdAt = stringFromRecord(worktree, "createdAt");
  return {
    branch,
    ...(createdAt && !Number.isNaN(Date.parse(createdAt)) ? { createdAt } : {}),
    name,
    parentProjectId,
    parentProjectName,
    parentProjectPath,
  };
}

function normalizeGpuiWorktreeMetadata(
  worktree: Record<string, unknown> | undefined,
): GpuiWorktreeMetadata | undefined {
  const parentProjectId = normalizeGpuiWorktreeParentProjectId(worktree);
  if (!parentProjectId) {
    return undefined;
  }
  return {
    ...optionalStringField("branch", stringFromRecord(worktree, "branch")),
    ...optionalStringField("name", stringFromRecord(worktree, "name")),
    ...optionalStringField("parentProjectName", stringFromRecord(worktree, "parentProjectName")),
    parentProjectId,
  };
}

function stringFromRecord(
  record: Record<string, unknown> | undefined,
  key: string,
): string | undefined {
  const value = record?.[key];
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function booleanFromRecord(
  record: Record<string, unknown> | undefined,
  key: string,
): boolean | undefined {
  const value = record?.[key];
  return typeof value === "boolean" ? value : undefined;
}

function optionalStringField<TKey extends string>(
  key: TKey,
  value: string | undefined,
): Partial<Record<TKey, string>> {
  return value ? ({ [key]: value } as Partial<Record<TKey, string>>) : {};
}

function optionalNumberField<TKey extends string>(
  key: TKey,
  value: number | undefined,
): Partial<Record<TKey, number>> {
  return value !== undefined ? ({ [key]: value } as Partial<Record<TKey, number>>) : {};
}

function normalizeGpuiPathForProjectComparison(path: string): string {
  return path.trim().replace(/\/+$/u, "") || path.trim();
}

function parseGpuiGitNumstatFiles(stdout: string): SidebarGitChangedFile[] {
  return stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .flatMap((line) => {
      const [additions, deletions, ...pathParts] = line.split(/\s+/);
      const path = normalizeGpuiRelativeGitFilePath(pathParts.join(" "));
      if (!path) {
        return [];
      }
      return [
        {
          additions: normalizeGpuiGitNumstatNumber(additions),
          deletions: normalizeGpuiGitNumstatNumber(deletions),
          path,
        },
      ];
    });
}

function parseGpuiGitStatusPorcelainFiles(stdout: string): SidebarGitChangedFile[] {
  return stdout
    .split(/\r?\n/)
    .filter((line) => line.length >= 4)
    .flatMap((line) => {
      const rawPath = line.slice(3).trim();
      const path = normalizeGpuiRelativeGitFilePath(
        rawPath.includes(" -> ") ? (rawPath.split(" -> ").at(-1) ?? "") : rawPath,
      );
      return path ? [{ additions: 0, deletions: 0, path }] : [];
    });
}

function mergeGpuiGitChangedFiles(
  files: readonly SidebarGitChangedFile[],
): SidebarGitChangedFile[] {
  const mergedFiles = new Map<string, SidebarGitChangedFile>();
  for (const file of files) {
    const existing = mergedFiles.get(file.path);
    mergedFiles.set(file.path, {
      additions: Math.max(existing?.additions ?? 0, file.additions),
      deletions: Math.max(existing?.deletions ?? 0, file.deletions),
      path: file.path,
    });
  }
  return [...mergedFiles.values()];
}

function normalizeGpuiGitNumstatNumber(value: string | undefined): number {
  if (!value || value === "-") {
    return 0;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function summarizeGpuiGitChangedFiles(files: readonly SidebarGitChangedFile[]): {
  additions: number;
  deletions: number;
} {
  return files.reduce(
    (stats, file) => ({
      additions: stats.additions + file.additions,
      deletions: stats.deletions + file.deletions,
    }),
    { additions: 0, deletions: 0 },
  );
}

function parseGpuiGitHubPullRequest(stdout: string, success: boolean): SidebarGitState["pr"] {
  if (!success || !stdout.trim()) {
    return null;
  }
  try {
    const candidate = JSON.parse(stdout) as Partial<NonNullable<SidebarGitState["pr"]>>;
    const state = String(candidate.state || "").toLowerCase();
    if (!candidate.url || !candidate.title || !["open", "closed", "merged"].includes(state)) {
      return null;
    }
    return {
      number: typeof candidate.number === "number" ? candidate.number : undefined,
      state: state as NonNullable<SidebarGitState["pr"]>["state"],
      title: candidate.title,
      url: candidate.url,
    };
  } catch {
    return null;
  }
}

function isGpuiConfirmedOpenPullRequest(result: GxserverCreatePullRequestResult): boolean {
  return (
    result.ok === true &&
    result.pr?.state === "open" &&
    typeof result.pr.url === "string" &&
    /^https:\/\/github\.com\/[^/\s]+\/[^/\s]+\/pull\/\d+$/u.test(result.pr.url)
  );
}

function isGpuiConfirmedOpenRemotePullRequest(result: GpuiRemoteCreatePullRequestResult): boolean {
  return result.ok === true && result.pr?.state === "open";
}

function normalizeGpuiGitHubRemoteUrl(remoteUrl: string): string | undefined {
  const trimmed =
    remoteUrl
      .trim()
      .split(/\s+/)[0]
      ?.replace(/\.git$/u, "") ?? "";
  if (!trimmed) {
    return undefined;
  }
  const sshMatch = /^git@github\.com:(?<path>[^#?]+)$/u.exec(trimmed);
  const sshPath = sshMatch?.groups?.path;
  if (sshPath) {
    return `https://github.com/${sshPath.replace(/^\/+/u, "").replace(/\.git$/u, "")}`;
  }
  try {
    const parsed = new URL(trimmed);
    if (parsed.hostname !== "github.com") {
      return undefined;
    }
    const repoPath = parsed.pathname.replace(/^\/+/u, "").replace(/\.git$/u, "");
    return repoPath ? `https://github.com/${repoPath}` : undefined;
  } catch {
    return undefined;
  }
}

function parseGpuiSidebarGitCommitMessage(message: string): {
  body: string;
  subject: string;
} {
  const trimmedMessage = message.trim();
  if (!trimmedMessage) {
    return { body: "", subject: "" };
  }
  const [firstLine = "", ...restLines] = trimmedMessage.split(/\r?\n/);
  return {
    body: restLines.join("\n").trim(),
    subject: firstLine.trim(),
  };
}

/*
CDXC:GPUISidebarGit 2026-06-24-16:11:
Blank commit-message generation in GPUI mirrors the native background prompt
support set. Built-in agents that do not expose a safe headless prompt mode
must fail explicitly, while configured non-default custom agents may use their
stored command through the local gxserver generation endpoint.
*/
function supportsGpuiBackgroundCommitMessageGeneration(agent: SidebarAgentButton): boolean {
  return (
    GPUI_BACKGROUND_COMMIT_MESSAGE_DEFAULT_AGENT_IDS.has(agent.agentId) ||
    !isDefaultSidebarAgentId(agent.agentId)
  );
}

function gpuiUserVisibleGitErrorMessage(error: unknown, fallback: string): string {
  /*
  CDXC:GPUISidebarGit 2026-07-11-05:08:
  The gxserver client already converts daemon failures into bounded,
  user-facing Error messages. Preserve those messages at the Git mutation
  boundary so stale reviews, unavailable agents, and generation failures do
  not collapse into an unactionable generic toast. Generation runs inside the
  mutation's keyed progress toast, so it must not create a second unkeyed info
  toast that survives after the mutation fails.
  */
  if (!(error instanceof Error)) {
    return fallback;
  }
  const message = error.message
    .replace(/[\u0000-\u001f\u007f-\u009f]+/gu, " ")
    .replace(/\s+/gu, " ")
    .trim()
    .slice(0, 500);
  return message || fallback;
}

function sanitizeGpuiSidebarGitBranchName(subject: string): string {
  return (
    subject
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^\w\s-]/gu, "")
      .trim()
      .replace(/[\s_]+/gu, "-")
      .replace(/-+/gu, "-")
      .replace(/^-|-$/gu, "")
      .slice(0, 48) || `change-${Date.now().toString(36)}`
  );
}

function normalizeGpuiRelativeGitFilePath(filePath: string): string | undefined {
  const normalizedFilePath = filePath.replaceAll("\\", "/").replace(/^\/+/, "").trim();
  if (!normalizedFilePath || normalizedFilePath.includes("\0")) {
    return undefined;
  }
  const segments = normalizedFilePath.split("/");
  if (segments.some((segment) => !segment || segment === "." || segment === "..")) {
    return undefined;
  }
  return normalizedFilePath;
}

function isMissingGpuiBeadsDatabaseError(message: string): boolean {
  return /no beads database found|run ['"]?bd init['"]?|not initialized|no storage/iu.test(message);
}

function resolveGpuiSidebarGitConfirmLabel(
  action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
  hasCommit: boolean,
): string {
  if (action === "commit") {
    return "Commit";
  }
  if (action === "push") {
    return hasCommit ? "Commit & Push" : "Push";
  }
  return hasCommit ? "Commit, Push & PR" : "Push & Create PR";
}

function resolveGpuiSidebarGitPromptDescription(
  action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
): string {
  if (action === "commit") {
    return "Review and commit changes.";
  }
  if (action === "push") {
    return "Push the current branch.";
  }
  return "Create or open a pull request.";
}

function resolveGpuiSidebarGitStartedTitle(
  action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
  hasCommit: boolean,
): string {
  if (action === "pr") {
    return hasCommit ? "Committing, pushing, and creating PR" : "Pushing and creating PR";
  }
  if (action === "push") {
    return hasCommit ? "Committing and pushing" : "Pushing";
  }
  return "Committing";
}

function resolveGpuiSidebarGitFinishedTitle(
  action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
): string {
  if (action === "pr") {
    return "Pull request ready";
  }
  return action === "push" ? "Push complete" : "Commit complete";
}

function formatGpuiGitAgentWorkflowTitle(title: string): string {
  const normalizedTitle = title.trim();
  return normalizedTitle.startsWith("Git:") ? normalizedTitle : `Git: ${normalizedTitle}`;
}

function buildGpuiGitSyncWithMainPrompt(): string {
  return [
    "Please sync the latest main branch changes into this worktree so it can be merged back to main afterward.",
    "",
    "Use the current repository and branch in this terminal. Inspect Git state directly before changing anything.",
    "",
    "Requirements:",
    "- Fetch the latest remote refs before syncing.",
    "- Bring main into this worktree branch using the safest normal project workflow for this repository, such as merge or rebase only if that is clearly the repo convention.",
    "- Preserve work from both main and this worktree. If conflicts happen, resolve them without dropping code, behavior, or UX from either side.",
    "- After resolving conflicts, run the relevant checks you can run locally.",
    "- Leave the worktree branch ready for the user to merge back into main.",
    "- Stop and explain clearly if the repository state is unsafe or if a decision is needed.",
  ]
    .filter(Boolean)
    .join("\n");
}

function buildGpuiGitPullRequestAgentPrompt(input: {
  filePaths?: readonly string[];
  hasExplicitFileSelection: boolean;
  hasCommit: boolean;
  message: string;
  selectedFiles: readonly string[];
}): string {
  const selectedFiles = input.selectedFiles.filter((filePath) => filePath.trim().length > 0);
  return [
    "Please complete the Git pull request flow in this terminal.",
    "",
    "Use the current repository checkout in this terminal. Inspect branch, remote, and PR state directly before changing anything.",
    "",
    "Do these steps visibly:",
    input.hasCommit
      ? input.hasExplicitFileSelection
        ? "- Stage and commit only the selected files listed below. Do not stage excluded files."
        : "- Stage and commit all new/modified files."
      : "- There were no working tree changes when the modal opened, so skip committing unless you find new user changes.",
    input.message
      ? "- Use the requested commit message below unless it is clearly invalid for the actual diff."
      : "- Write a concise commit message that matches the staged diff.",
    "- If you encounter conflicts, rebases, merge state, or divergent local/remote changes, make sure not to lose changes from either side.",
    "- Push the current branch to origin, setting upstream if needed.",
    "- Create a GitHub pull request with `gh pr create --fill`, or open/show the existing PR if one already exists.",
    "- Stop and explain clearly if a command fails, authentication is missing, or a merge/rebase/conflict situation needs the user's decision.",
    "",
    input.hasExplicitFileSelection && selectedFiles.length > 0
      ? ["Selected files:", ...selectedFiles.map((filePath) => `- ${filePath}`)].join("\n")
      : "Selected files: all new/modified files.",
    input.message ? `\nRequested commit message:\n${input.message}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function buildGpuiMergeConflictPrompt(input: {
  branch: string;
  mergeOutput: string;
  parentProject: GxserverProjectDomainState;
  worktree: GpuiWorktreeMetadata;
  worktreeProject: GxserverProjectDomainState;
}): string {
  const output = input.mergeOutput.trim();
  const worktreeName = input.worktree.name ?? input.worktreeProject.name ?? "this worktree";
  const parentName =
    input.parentProject.name || input.worktree.parentProjectName || "the main project";
  return [
    "Please handle the current Git merge conflicts on the main branch.",
    "",
    `Target project: ${parentName}`,
    "Target branch: main",
    `Merged worktree branch: ${input.branch}`,
    `Worktree: ${worktreeName}`,
    "",
    "Resolve the conflicts without losing any code, behavior, or UX from either side.",
    "Inspect the conflict markers, preserve the important intent from main and the worktree branch, run the relevant checks you can run locally, stage the resolved files, and leave the final state ready for review.",
    output ? `\nMerge output:\n${output}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function hasGpuiGxserverShortStatusChanges(stdout: string): boolean {
  return stdout.split("\n").some((line) => {
    const trimmed = line.trim();
    return trimmed.length > 0 && !trimmed.startsWith("##");
  });
}

function normalizeGpuiProjectPath(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value.trim().replace(/\/+$/u, "")
    : undefined;
}

function normalizeGpuiWorktreeBaseBranches(
  branches: GxserverTypedOperationResult["branches"],
): Array<{ current: boolean; name: string; remote: boolean }> {
  const seenBranches = new Set<string>();
  return (branches ?? []).flatMap((branch) => {
    const name = branch.name?.trim();
    if (!name || seenBranches.has(name)) {
      return [];
    }
    seenBranches.add(name);
    return [
      {
        current: branch.current === true,
        name,
        remote: branch.remote === true,
      },
    ];
  });
}

function normalizeGpuiExistingWorktreeOptions(
  worktrees: GxserverProjectWorktreeListResult["worktrees"] | unknown,
): Array<{
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
  worktreeKey: string;
}> {
  if (!Array.isArray(worktrees)) {
    return [];
  }
  return worktrees.flatMap((entry) => {
    if (!entry || typeof entry !== "object") {
      return [];
    }
    const worktree = entry as Record<string, unknown>;
    const path = normalizeGpuiProjectPath(worktree.path);
    const name =
      stringFromRecord(worktree, "name") ?? (path ? gpuiProjectNameFromPath(path) : undefined);
    const worktreeKey = stringFromRecord(worktree, "worktreeKey");
    if (!path || !name || !worktreeKey) {
      return [];
    }
    return [
      {
        branch: stringFromRecord(worktree, "branch") ?? "",
        isCurrentProject: booleanFromRecord(worktree, "isCurrentProject") === true,
        isRegistered: booleanFromRecord(worktree, "isRegistered") === true,
        name,
        path,
        worktreeKey,
      },
    ];
  });
}

function createGpuiExistingWorktreeOptions(
  worktrees: GxserverTypedOperationResult["worktrees"],
  parentProject: GxserverProjectDomainState,
  sourceProject: GxserverProjectDomainState,
  domainProjects: readonly GxserverProjectDomainState[],
): Array<{
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
}> {
  const entries = worktrees ?? [];
  const mainEntry = entries.find((entry) => entry.bare !== true);
  const mainPath =
    normalizeGpuiProjectPath(mainEntry?.path) ?? normalizeGpuiProjectPath(parentProject.path);
  const sourcePath = normalizeGpuiProjectPath(sourceProject.path);
  const registeredPaths = new Set(
    domainProjects
      .map((project) => normalizeGpuiProjectPath(project.path))
      .filter((path): path is string => Boolean(path)),
  );
  return entries.flatMap((entry) => {
    if (entry.bare === true) {
      return [];
    }
    const path = normalizeGpuiProjectPath(entry.path);
    if (!path || path === mainPath) {
      return [];
    }
    return [
      {
        branch: entry.branch?.trim() ?? "",
        isCurrentProject: path === sourcePath,
        isRegistered: registeredPaths.has(path),
        name: gpuiProjectNameFromPath(path),
        path,
      },
    ];
  });
}

function gpuiProjectNameFromPath(path: string): string {
  return path.split("/").filter(Boolean).at(-1) ?? "Project";
}

function gpuiWithAtuinIgnoredShellHistoryPrefix(text: string): string {
  return text.startsWith(" ") ? text : ` ${text}`;
}

function gpuiDirname(path: string): string {
  const parts = path.replace(/\/+$/u, "").split("/").filter(Boolean);
  if (parts.length <= 1) {
    return "/";
  }
  return `/${parts.slice(0, -1).join("/")}`;
}

function gpuiWorktreeSlugFromPrompt(prompt: string): string {
  const firstWords = prompt
    .trim()
    .toLowerCase()
    .replace(/[`'"]/gu, "")
    .replace(/[^a-z0-9]+/gu, "-")
    .replace(/^-+|-+$/gu, "")
    .split("-")
    .filter(Boolean)
    .slice(0, 6)
    .join("-");
  return (firstWords || "worktree").slice(0, 48).replace(/-+$/u, "") || "worktree";
}

function createGpuiWorktreeToastId(): string {
  return `toast-gpui-worktree-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function createGpuiGitToastId(): string {
  return `toast-gpui-git-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function gpuiWorktreeUserVisibleErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message.trim() : "";
  if (
    message &&
    !message.includes("/") &&
    !message.includes("\\") &&
    !message.includes("\n") &&
    message.length <= 160
  ) {
    return message;
  }
  return "The gxserver worktree operation failed.";
}

function createGpuiGxserverUnavailableSidebarGroups(): SidebarSessionGroup[] {
  return [
    {
      groupId: GPUI_GXSERVER_CHATS_GROUP_ID,
      isActive: false,
      isChatCollection: true,
      isFocusModeActive: false,
      kind: "workspace",
      layoutVisibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
      sessions: [],
      title: "Chats",
      viewMode: "grid",
      visibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    },
    {
      groupId: GPUI_GXSERVER_UNAVAILABLE_GROUP_ID,
      isActive: true,
      isFocusModeActive: false,
      kind: "workspace",
      layoutVisibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
      sessions: [],
      title: "",
      viewMode: "grid",
      visibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    },
  ];
}

function createGpuiRemotePresentationSidebarGroups({
  activeGroupId,
  focusedSessionId,
  presentationsByMachineId,
  remoteGroupOrderByMachineId,
  remoteRecentProjectsByMachineId,
  resolveAgentIcon,
  resolveCloseAfterDone,
  settings,
  visibleSessionIds,
}: {
  activeGroupId?: string;
  focusedSessionId?: string;
  presentationsByMachineId: ReadonlyMap<string, GxserverPresentationSnapshot>;
  remoteGroupOrderByMachineId?: ReadonlyMap<string, readonly string[]>;
  remoteRecentProjectsByMachineId?: ReadonlyMap<
    string,
    readonly GxserverRecentProjectDomainState[]
  >;
  resolveAgentIcon: (agentName: string | undefined) => SidebarAgentButton["icon"];
  resolveCloseAfterDone?: (
    machineId: string,
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationCloseAfterDoneProjection | undefined;
  settings: ghostexSettings;
  visibleSessionIds?: ReadonlySet<string>;
}): SidebarSessionGroup[] {
  /*
  CDXC:GPUIRemoteMachines 2026-06-24-16:48:
  GPUI remote machine sections must render only saved machines with Rust-delivered gxserver presentation snapshots. Prefix every project/session id with the machine id so reused SidebarApp rows cannot collide with local gxserver rows or another remote machine, while tokens, SSH hosts, usernames, key paths, and remote URLs stay outside renderer state.
  */
  return settings.remoteMachines.flatMap((machine) => {
    const presentation = presentationsByMachineId.get(machine.id);
    if (!presentation) {
      return [];
    }
    const projectsById = new Map(
      presentation.projects.map((project) => [project.projectId, project]),
    );
    const orderedGroups = orderGpuiRemotePresentationGroups(
      presentation.groups,
      remoteGroupOrderByMachineId?.get(machine.id),
    );
    const orderIndexByProjectId = new Map(
      orderedGroups.map((group, index) => [group.projectId, index]),
    );
    const hiddenProjectIds = new Set(
      presentation.projects.flatMap((project) =>
        isGpuiRemoteProjectClosedToRecent(
          machine.id,
          project.projectId,
          remoteRecentProjectsByMachineId,
        )
          ? [project.projectId]
          : [],
      ),
    );
    const chatProjectIds = new Set(
      presentation.projects.flatMap((project) =>
        isGpuiPresentationChatProjectPath(project.path) ? [project.projectId] : [],
      ),
    );
    const activeRemoteGroup = activeGroupId
      ? parseGpuiRemotePresentationGroupId(activeGroupId)
      : undefined;
    const focusedRemoteSession = focusedSessionId
      ? parseGpuiRemotePresentationSessionId(focusedSessionId)
      : undefined;
    const activeProjectId =
      activeRemoteGroup?.machineId === machine.id &&
      activeRemoteGroup.projectId !== GPUI_GXSERVER_CHATS_GROUP_ID
        ? activeRemoteGroup.projectId
        : focusedRemoteSession?.machineId === machine.id
          ? focusedRemoteSession.projectId
          : undefined;
    const focusedRawSessionId =
      focusedRemoteSession?.machineId === machine.id
        ? focusedRemoteSession.sessionId
        : undefined;
    const visibleRawSessionIds = new Set(
      [...(visibleSessionIds ?? [])].flatMap((sessionId) => {
        const reference = parseGpuiRemotePresentationSessionId(sessionId);
        return reference?.machineId === machine.id ? [reference.sessionId] : [];
      }),
    );
    const groups = createGxserverPresentationSidebarGroups({
      activeProjectId,
      chatProjectIds,
      chatsGroupId: createGpuiRemotePresentationGroupId(
        machine.id,
        GPUI_GXSERVER_CHATS_GROUP_ID,
      ),
      createProjectGroupId: (projectId) =>
        createGpuiRemotePresentationGroupId(machine.id, projectId),
      createProjectSessionId: (projectId, sessionId) =>
        createGpuiRemotePresentationSessionId(machine.id, projectId, sessionId),
      focusedSessionId: focusedRawSessionId,
      hiddenProjectIds,
      presentation,
      projectOverlays: presentation.projects.map((project) => {
        const worktree = normalizeGpuiSidebarWorktreeMetadata(project.worktree);
        return {
          editor: {
            diffStats: createDefaultSidebarProjectDiffStats(),
            isOpen: false,
            isSleeping: false,
            projectId: createGpuiRemotePresentationProjectId(machine.id, project.projectId),
            status: "idle" as const,
          },
          orderIndex: orderIndexByProjectId.get(project.projectId),
          path: project.path ?? "",
          projectId: project.projectId,
          theme: resolveSidebarTheme(settings.sidebarTheme, "dark"),
          ...(worktree ? { worktree } : {}),
        };
      }),
      resolveAgentIcon,
      resolveCloseAfterDone: resolveCloseAfterDone
        ? (projectId, sessionId) =>
            resolveCloseAfterDone(machine.id, projectId, sessionId)
        : undefined,
      resolveSessionRoutingId: (projectId, sessionId) =>
        createGpuiRemotePresentationSessionRoutingId(machine.id, projectId, sessionId),
      visibleSessionIds: visibleRawSessionIds,
    });
    return groups.map((group) => {
      const reference = parseGpuiRemotePresentationGroupId(group.groupId);
      const project =
        reference && reference.projectId !== GPUI_GXSERVER_CHATS_GROUP_ID
          ? projectsById.get(reference.projectId)
          : undefined;
      return {
        ...group,
        canCreateSessionGroup: project !== undefined,
        projectContext: group.projectContext
          ? {
              ...group.projectContext,
              canRemoveProject: true,
              path: project?.path ?? group.projectContext.path,
            }
          : undefined,
        remoteMachineContext: {
          machineId: machine.id,
          machineName: machine.name,
          ...(project ? { projectId: project.projectId } : {}),
        },
        sessions: group.sessions.map((session) => ({
          ...session,
          canPopOutPane:
            session.sessionKind === "terminal" &&
            Boolean(session.agentIcon) &&
            session.isSleeping !== true &&
            session.lifecycleState !== "sleeping",
          canScheduleDelayedSend: session.sessionKind === "terminal",
          canToggleCloseAfterDone: session.sessionKind === "terminal",
        })),
      };
    });
  });
}

function orderGpuiRemotePresentationGroups<Group extends { projectId: string }>(
  groups: readonly Group[],
  storedProjectIdOrder: readonly string[] | undefined,
): Group[] {
  /*
  CDXC:RemoteGroupReorder 2026-07-12:
  Apply the app-local per-machine order overlay as a stable sort: known project
  ids render in the stored order, and projects the overlay has never seen keep
  their remote presentation position after them.
  */
  if (!storedProjectIdOrder || storedProjectIdOrder.length === 0) {
    return [...groups];
  }
  const orderIndexByProjectId = new Map(
    storedProjectIdOrder.map((projectId, index) => [projectId, index]),
  );
  const ordered = groups.filter((group) => orderIndexByProjectId.has(group.projectId));
  const unordered = groups.filter((group) => !orderIndexByProjectId.has(group.projectId));
  ordered.sort(
    (left, right) =>
      orderIndexByProjectId.get(left.projectId)! - orderIndexByProjectId.get(right.projectId)!,
  );
  return [...ordered, ...unordered];
}

function isGpuiRemoteProjectClosedToRecent(
  machineId: string,
  projectId: string,
  recentProjectsByMachineId:
    ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]> | undefined,
): boolean {
  /*
  CDXC:GPUIRemoteProjects 2026-06-27-19:37:
  Connected remote presentation projects render under their saved-machine sections, while client-parked remote projects render only as machine-scoped rows in Recent Projects. Filter the remote machine projection with GPUI's app-local recent list instead of mutating the remote gxserver project state.
  */
  return (recentProjectsByMachineId?.get(machineId) ?? []).some(
    (project) => project.projectId === projectId,
  );
}

function compareGpuiRemoteAttachCandidateSessions(
  left: GxserverPresentationSession,
  right: GxserverPresentationSession,
): number {
  const score = (session: GxserverPresentationSession): number => {
    let value = 0;
    if (session.lifecycleState === "running") {
      value += 100;
    }
    if (session.activity === "attention") {
      value += 40;
    } else if (session.activity === "working") {
      value += 30;
    }
    if (session.isPinned) {
      value += 10;
    }
    if (session.isFavorite) {
      value += 5;
    }
    return value;
  };
  const scoreDelta = score(right) - score(left);
  if (scoreDelta !== 0) {
    return scoreDelta;
  }
  const rightTime = Date.parse(right.lastActiveAt ?? right.updatedAt ?? right.createdAt);
  const leftTime = Date.parse(left.lastActiveAt ?? left.updatedAt ?? left.createdAt);
  return (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
}

function createGpuiRemotePresentationGroupId(machineId: string, projectId: string): string {
  return `remote:${machineId}:group:${projectId}`;
}

function parseGpuiRemotePresentationGroupId(
  groupId: string,
): { machineId: string; projectId: string } | undefined {
  const match = /^remote:([^:]+):group:(.+)$/u.exec(groupId);
  if (!match) {
    return undefined;
  }
  return { machineId: match[1]!, projectId: match[2]! };
}

function createGpuiRemotePresentationProjectId(machineId: string, projectId: string): string {
  return createRemoteProjectId({ machineId, projectId });
}

function parseGpuiRemotePresentationProjectId(
  projectId: string,
): { machineId: string; projectId: string } | undefined {
  return parseRemoteProjectId(projectId);
}

function createGpuiRemotePresentationSessionId(
  machineId: string,
  projectId: string,
  sessionId: string,
): string {
  return createRemoteTerminalSessionId({ machineId, projectId, sessionId });
}

function parseGpuiRemotePresentationSessionId(
  sessionId: string,
): { machineId: string; projectId: string; sessionId: string } | undefined {
  return parseRemoteTerminalSessionId(sessionId);
}

function createGpuiRemotePresentationSessionRoutingId(
  machineId: string,
  projectId: string,
  sessionId: string,
): string {
  return `${machineId}:${projectId}:${sessionId}`;
}

function createGpuiSidebarGroupsPatch(
  previousGroups: readonly SidebarSessionGroup[],
  nextGroups: SidebarSessionGroup[],
): GpuiSidebarGroupsPatch {
  const previousGroupIds = new Set(previousGroups.map((group) => group.groupId));
  const nextGroupIds = new Set(nextGroups.map((group) => group.groupId));
  const previousSessionIds = new Set(
    previousGroups.flatMap((group) => group.sessions.map((session) => session.sessionId)),
  );
  const nextSessionIds = new Set(
    nextGroups.flatMap((group) => group.sessions.map((session) => session.sessionId)),
  );
  return {
    groupOrder: nextGroups.map((group) => group.groupId),
    groups: nextGroups,
    removedGroupIds: [...previousGroupIds].filter((groupId) => !nextGroupIds.has(groupId)),
    removedSessionIds: [...previousSessionIds].filter(
      (sessionId) => !nextSessionIds.has(sessionId),
    ),
  };
}

function gxserverSearchResultToPreviousSessionItem(
  result: GxserverPresentationSearchResult,
  options: { historyIdPrefix?: string; projectNamePrefix?: string } = {},
): SidebarPreviousSessionItem {
  const title = result.displayTitle || result.primaryTitle || result.title || "Previous Session";
  const closedAt = result.closedAt ?? result.updatedAt ?? result.createdAt;
  const agentName = result.agentName ?? result.agentId;
  const sessionPersistenceProvider = result.sessionPersistenceProvider ?? "zmx";
  const sessionPersistenceName = result.sessionPersistenceName ?? result.zmxName;
  return {
    activity: "idle",
    agentIcon: resolveGpuiSidebarAgentIcon(result.agentIcon ?? agentName),
    agentSessionId: result.agentSessionId,
    alias: title,
    closedAt,
    column: 0,
    displayTitle: result.displayTitle,
    displayTitleTooltip: result.displayTitleTooltip,
    historyId: `${options.historyIdPrefix ?? "gxserver"}:${result.projectId}:${result.sessionId}`,
    isFavorite: result.isFavorite,
    isFocused: false,
    isGeneratedName: false,
    isPinned: result.isPinned,
    isPrimaryTitleTerminalTitle: result.isPrimaryTitleTerminalTitle,
    isRestorable: true,
    isRunning: false,
    isVisible: false,
    lastInteractionAt: result.lastActiveAt,
    lifecycleState: "done",
    primaryTitle: result.primaryTitle ?? title,
    projectId: result.projectId,
    projectName: options.projectNamePrefix
      ? `${options.projectNamePrefix} / ${result.projectTitle}`
      : result.projectTitle,
    row: 0,
    sessionId: result.sessionId,
    sessionKind: "terminal",
    sessionPersistenceName,
    sessionPersistenceProvider,
    sessionTag: result.sessionTag,
    shortcutLabel: "",
    terminalTitle: result.terminalTitle,
  };
}

function comparePreviousSessionItemsByClosedTime(
  left: SidebarPreviousSessionItem,
  right: SidebarPreviousSessionItem,
): number {
  return previousSessionClosedTime(right) - previousSessionClosedTime(left);
}

function previousSessionClosedTime(session: SidebarPreviousSessionItem): number {
  const time = Date.parse(session.closedAt);
  return Number.isFinite(time) ? time : 0;
}

function parseGpuiGxserverPreviousSessionHistoryId(
  historyId: string,
): { projectId: string; sessionId: string } | undefined {
  const match = /^gxserver:([^:]+):([^:]+)$/u.exec(historyId);
  if (!match) {
    return undefined;
  }
  return { projectId: match[1]!, sessionId: match[2]! };
}

function parseGpuiRemotePreviousSessionHistoryId(
  historyId: string,
): { machineId: string; projectId: string; sessionId: string } | undefined {
  const match = /^remote-gxserver:([^:]+):([^:]+):([^:]+)$/u.exec(historyId);
  if (!match) {
    return undefined;
  }
  return { machineId: match[1]!, projectId: match[2]!, sessionId: match[3]! };
}

function previousSessionTitle(previousSession: SidebarPreviousSessionItem | undefined): string {
  return (
    previousSession?.primaryTitle ||
    previousSession?.terminalTitle ||
    previousSession?.alias ||
    DEFAULT_TERMINAL_SESSION_TITLE
  );
}

function gpuiProjectBoardPreviousSessionRowTitle(row: GxserverPresentationSearchResult): string {
  return row.displayTitle || row.primaryTitle || row.title || DEFAULT_TERMINAL_SESSION_TITLE;
}

function resolveGpuiSidebarAgentIcon(agentName: string | undefined): SidebarAgentButton["icon"] {
  const directIcon = getSidebarAgentIconById(agentName);
  if (directIcon) {
    return directIcon;
  }

  const normalizedAgentName = agentName?.trim().toLowerCase();
  if (!normalizedAgentName) {
    return undefined;
  }
  return DEFAULT_SIDEBAR_AGENTS.find(
    (agent) =>
      agent.agentId === normalizedAgentName ||
      agent.name.trim().toLowerCase() === normalizedAgentName ||
      agent.icon === normalizedAgentName,
  )?.icon;
}

function createGpuiSidebarSessionRoutingId(projectId: string, sessionId: string): string {
  return `${projectId}:${sessionId}`;
}

function currentGpuiRuntimeSettings(): GpuiSidebarRuntimeSettings | undefined {
  return window.ghostexGpui?.runtimeSettings;
}

function hasSameGpuiRuntimeSettings(
  previous: GpuiSidebarRuntimeSettings | undefined,
  next: GpuiSidebarRuntimeSettingsSnapshot,
): boolean {
  return (
    previous?.debuggingMode === next.debuggingMode &&
    previous?.showBetaFeatures === next.showBetaFeatures &&
    previous?.settings === next.settings
  );
}

function normalizeGpuiNativeAppShotPromptResult(
  value: unknown,
): { ok: boolean; sessionId: string } | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    record.type !== GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION ||
    typeof record.ok !== "boolean"
  ) {
    return undefined;
  }
  const sessionId = normalizeNonEmptyString(record.sessionId);
  return sessionId ? { ok: record.ok, sessionId } : undefined;
}

function nativeAppShotPromptSessionIdForSidebarSession(
  session: SidebarSessionItem | undefined,
): string | undefined {
  if (!session) {
    return undefined;
  }
  const remoteSession = parseGpuiRemotePresentationSessionId(session.sessionId);
  if (remoteSession) {
    return createGpuiRemotePresentationSessionId(
      remoteSession.machineId,
      remoteSession.projectId,
      remoteSession.sessionId,
    );
  }
  return localGxserverSessionIdForSidebarSession(session);
}

function localGxserverSessionIdForSidebarSession(
  session: SidebarSessionItem | undefined,
): string | undefined {
  if (!session || parseGpuiRemotePresentationSessionId(session.sessionId)) {
    return undefined;
  }
  return (
    parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId ??
    normalizeNonEmptyString(session.sessionId)
  );
}

function localGxserverProjectIdForSidebarSession(
  session: SidebarSessionItem,
  presentation: GxserverPresentationSnapshot | undefined,
): string | undefined {
  const scopedSession = parseGxserverPresentationProjectSessionId(session.sessionId);
  if (scopedSession?.projectId) {
    return scopedSession.projectId;
  }
  const sessionId = localGxserverSessionIdForSidebarSession(session);
  return sessionId
    ? presentation?.sessions.find((candidate) => candidate.sessionId === sessionId)?.projectId
    : undefined;
}

function isNativeAppShotAgentSession(
  session: SidebarSessionItem | undefined,
): session is SidebarSessionItem {
  if (!session) {
    return false;
  }
  if (session.sessionKind !== "terminal" || session.isSleeping === true) {
    return false;
  }
  if (session.lifecycleState === "sleeping" || session.isLive !== true) {
    return false;
  }
  return Boolean(session.agentIcon);
}

function normalizeGpuiNativeAppShotCapture(value: unknown): GpuiNativeAppShotCapture | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    record.type !== GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const appName = normalizeGpuiNativeAppShotString(record.appName, 256);
  const imagePath = normalizeGpuiNativeAppShotImagePath(record.imagePath);
  if (!appName || !imagePath) {
    return undefined;
  }
  const bundleIdentifier = normalizeGpuiNativeAppShotString(record.bundleIdentifier, 256);
  const windowTitle = normalizeGpuiNativeAppShotString(record.windowTitle, 512);
  const windowWidth = normalizeGpuiNativeAppShotDimension(record.windowWidth);
  const windowHeight = normalizeGpuiNativeAppShotDimension(record.windowHeight);
  const trigger = normalizeGpuiNativeAppShotTrigger(record.trigger);
  const appShot: GpuiNativeAppShotCapture = {
    appName,
    imagePath,
  };
  if (bundleIdentifier) {
    appShot.bundleIdentifier = bundleIdentifier;
  }
  if (windowTitle) {
    appShot.windowTitle = windowTitle;
  }
  if (windowWidth) {
    appShot.windowWidth = windowWidth;
  }
  if (windowHeight) {
    appShot.windowHeight = windowHeight;
  }
  if (trigger) {
    appShot.trigger = trigger;
  }
  return appShot;
}

function normalizeGpuiNativeAppShotImagePath(value: unknown): string | undefined {
  const path = normalizeGpuiNativeAppShotString(value, 4096);
  if (!path || (!path.startsWith("~/") && !path.startsWith("/"))) {
    return undefined;
  }
  return path;
}

function normalizeGpuiNativeAppShotString(value: unknown, maxLength: number): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const text = value.trim();
  if (!text || text.length > maxLength || /[\u0000-\u001f\u007f]/u.test(text)) {
    return undefined;
  }
  return text;
}

function normalizeGpuiNativeAppShotDimension(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0 || value > 100_000) {
    return undefined;
  }
  return value;
}

function normalizeGpuiNativeAppShotTrigger(value: unknown): string | undefined {
  const trigger = normalizeGpuiNativeAppShotString(value, 80);
  return trigger === "both-command" ||
    trigger === "both-shift" ||
    trigger === "both-option" ||
    trigger === "double-left-shift" ||
    trigger === "double-left-option"
    ? trigger
    : undefined;
}

function formatGpuiNativeAppShotPrompt(
  appShot: GpuiNativeAppShotCapture,
  includeMetadata: boolean,
): string {
  const metadataLines = [`App: ${appShot.appName}`];
  if (appShot.bundleIdentifier) {
    metadataLines.push(`Bundle ID: ${appShot.bundleIdentifier}`);
  }
  if (appShot.windowTitle) {
    metadataLines.push(`Window title: ${appShot.windowTitle}`);
  }
  if (appShot.windowWidth && appShot.windowHeight) {
    metadataLines.push(`Window size: ${appShot.windowWidth} x ${appShot.windowHeight} px`);
  }
  /*
  CDXC:GPUIAppShots 2026-06-25-23:07:
  GPUI formats App Shot prompts using only native-supplied app/window metadata and the resolved Ghostex image-directory display path. The prompt must not include OCR, Accessibility text, DOM text, terminal content, stdout/stderr, commands, URLs, or renderer-supplied file paths.

  CDXC:GPUIAppShots 2026-06-29-01:29:
  Superseded by 2026-06-29-02:59.

  CDXC:GPUIAppShots 2026-06-29-02:59:
  App Shot prompt text should paste only the image link by default, with no intro sentence, no closing instruction, no blank spacer lines, and one newline of padding before and after. Add WindowServer metadata only when the Settings App Shots metadata toggle is enabled.
  */
  const promptLines = [`[Image #1](${appShot.imagePath})`];
  if (includeMetadata) {
    promptLines.push("Metadata:", ...metadataLines);
  }
  return `\n${promptLines.join("\n")}\n`;
}

function normalizeNonEmptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value : undefined;
}

function delayGpuiAgentPromptStep(delayMs: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, delayMs);
  });
}

const GPUI_MIN_ATTENTION_VISIBLE_MS = 1_500;
const GPUI_ESCAPE_DONE_SUPPRESSION_MS = 5_000;
const GPUI_ATTENTION_COMPLETION_SOUND_EVENT_CACHE_LIMIT = 2_048;
const GPUI_LOCALLY_ACKNOWLEDGED_ATTENTION_EVENT_CACHE_LIMIT = 2_048;

function gpuiSessionAttentionTargetKey(target: GpuiSessionAttentionTarget): string {
  return target.kind === "remote"
    ? createGpuiRemotePresentationSessionId(target.machineId, target.projectId, target.sessionId)
    : createGxserverPresentationProjectSessionId(target.projectId, target.sessionId);
}

function getGpuiSessionAttentionEventKey(
  sessionKey: string,
  attentionEventId: string | undefined,
): string | undefined {
  const normalizedSessionKey = normalizeNonEmptyString(sessionKey)?.trim();
  const normalizedAttentionEventId = normalizeNonEmptyString(attentionEventId)?.trim();
  if (!normalizedSessionKey || !normalizedAttentionEventId) {
    return undefined;
  }
  return `${normalizedSessionKey}\u001f${normalizedAttentionEventId}`;
}

function getGpuiPresentationAttentionEventId(
  session: Pick<GxserverPresentationSession, "activity" | "attention">,
): string | undefined {
  /*
  Presentation attention rows carry eventId for sound dedupe; enteredAt stays
  a compatibility key for older daemon payloads, matching macOS.
  */
  if (session.activity !== "attention") {
    return undefined;
  }
  const eventId = session.attention?.eventId?.trim();
  if (eventId) {
    return eventId;
  }
  const enteredAt = session.attention?.enteredAt?.trim();
  return enteredAt ? enteredAt : undefined;
}

const GPUI_CLOSE_AFTER_DONE_DELAY_MS = 3 * 60_000;
const GPUI_CLOSE_AFTER_DONE_STORAGE_KEY = "ghostex-gpui-close-after-done-session-ids";

type GpuiCloseAfterDoneTimer = {
  deadlineAtMs?: number;
  doneSinceAtMs?: number;
  timeoutId?: number;
};

function isGpuiInactiveProjectPresentationSession(session: GxserverPresentationSession): boolean {
  return (
    session.lifecycleState !== "sleeping" &&
    session.activity !== "working" &&
    session.activity !== "attention"
  );
}

function isGpuiCloseAfterDonePresentationSessionDone(
  session: GxserverPresentationSession,
): boolean {
  if (session.activity === "attention") {
    return true;
  }
  return session.activity !== "working" && hasGpuiCloseAfterDoneAgentIdentity(session);
}

function hasGpuiCloseAfterDoneAgentIdentity(session: GxserverPresentationSession): boolean {
  return Boolean(
    session.agentSessionId?.trim() ||
    session.agentSessionPath?.trim() ||
    session.agentName?.trim() ||
    session.agentId?.trim() ||
    session.agentIcon?.trim(),
  );
}

function formatGpuiCloseAfterDoneCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.ceil(remainingMs / 1_000));
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  const paddedMinutes = String(minutes).padStart(2, "0");
  const paddedSeconds = String(seconds).padStart(2, "0");
  if (hours > 0) {
    return `${String(hours).padStart(2, "0")}:${paddedMinutes}:${paddedSeconds}`;
  }
  return `${paddedMinutes}:${paddedSeconds}`;
}

function readStoredGpuiCloseAfterDoneSessionIds(): string[] {
  try {
    const raw = window.localStorage.getItem(GPUI_CLOSE_AFTER_DONE_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter(
      (value): value is string => typeof value === "string" && value.trim().length > 0,
    );
  } catch {
    return [];
  }
}

function writeStoredGpuiCloseAfterDoneSessionIds(sessionIds: readonly string[]): void {
  try {
    if (sessionIds.length === 0) {
      window.localStorage.removeItem(GPUI_CLOSE_AFTER_DONE_STORAGE_KEY);
      return;
    }
    window.localStorage.setItem(GPUI_CLOSE_AFTER_DONE_STORAGE_KEY, JSON.stringify([...sessionIds]));
  } catch {
    // Storage availability must never gate close-after-done behavior.
  }
}

function normalizeGpuiWorkspaceFolderPick(
  payload: unknown,
): { name?: string; path: string } | undefined {
  if (typeof payload !== "object" || payload === null) {
    return undefined;
  }
  const record = payload as { name?: unknown; path?: unknown; type?: unknown };
  if (record.type !== "workspaceFolderPicked") {
    return undefined;
  }
  const path = normalizeNonEmptyString(record.path);
  if (!path) {
    return undefined;
  }
  return { name: normalizeNonEmptyString(record.name), path };
}

function normalizeGpuiReplacementProjectFolderPick(
  payload: unknown,
): { path: string; projectId: string } | undefined {
  if (typeof payload !== "object" || payload === null) {
    return undefined;
  }
  const record = payload as { path?: unknown; projectId?: unknown; type?: unknown };
  if (record.type !== "replacementProjectFolderPicked") {
    return undefined;
  }
  const path = normalizeNonEmptyString(record.path);
  const projectId = normalizeNonEmptyString(record.projectId);
  if (!path || !projectId) {
    return undefined;
  }
  return { path, projectId };
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  return text.trim() ? (JSON.parse(text) as unknown) : undefined;
}

function isGxserverRpcSuccess<TResult>(value: unknown): value is GpuiGxserverRpcSuccess<TResult> {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    (value as Partial<GpuiGxserverRpcSuccess<TResult>>).ok === true &&
    (value as Partial<GpuiGxserverRpcSuccess<TResult>>).product === "gxserver" &&
    "result" in value
  );
}

function gpuiGxserverRpcErrorMessage(value: unknown): string | undefined {
  /*
  CDXC:GPUISidebarGxserverErrors 2026-07-11-05:56:
  gxserver domain endpoints return an intentionally user-facing `message` in
  their bounded RPC error envelope. The GPUI-local client must preserve that
  field just like the shared native client does; replacing it with a generic
  transport error hides actionable Git/generation failures and forces blind
  retries. Accept only a bounded plain string from an explicit failed envelope.
  */
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (record.ok !== false || typeof record.message !== "string") {
    return undefined;
  }
  const message = record.message
    .replace(/[\u0000-\u001f\u007f-\u009f]+/gu, " ")
    .replace(/\s+/gu, " ")
    .trim()
    .slice(0, 500);
  return message || undefined;
}

function gpuiGxserverRpcErrorCode(value: unknown): GxserverRpcErrorCode | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  return typeof record.error === "string" ? (record.error as GxserverRpcErrorCode) : undefined;
}

function parseObject(value: unknown): Record<string, unknown> | undefined {
  try {
    const parsed = typeof value === "string" ? (JSON.parse(value) as unknown) : value;
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : undefined;
  } catch {
    return undefined;
  }
}

async function handleGpuiRendererCommand(
  socket: WebSocket,
  command: GxserverRendererCommand,
  handler: GpuiRendererCommandHandler,
): Promise<void> {
  try {
    const result = await handler(command);
    socket.send(
      JSON.stringify({
        commandId: command.commandId,
        ok: true,
        result: isObjectRecord(result) ? result : { ok: true },
        type: "rendererCommandResult",
      }),
    );
  } catch (error) {
    socket.send(
      JSON.stringify({
        commandId: command.commandId,
        error: safeGpuiRendererCommandErrorMessage(error),
        ok: false,
        type: "rendererCommandResult",
      }),
    );
  }
}

function isGpuiRendererCommand(value: unknown): value is GxserverRendererCommand {
  if (!isObjectRecord(value)) {
    return false;
  }
  return (
    typeof value.action === "string" &&
    typeof value.commandId === "string" &&
    isObjectRecord(value.payload)
  );
}

function safeGpuiRendererCommandErrorMessage(error: unknown): string {
  if (!(error instanceof Error)) {
    return "Renderer command failed.";
  }
  if (
    error.message === "Invalid renderer command title." ||
    error.message === "No matching session was found." ||
    error.message === "Renderer command bridge unavailable." ||
    error.message === "Unsupported renderer command."
  ) {
    return error.message;
  }
  return "Renderer command failed.";
}

function normalizeGpuiRendererCommandRenameTitle(
  payload: Record<string, unknown>,
): string | undefined {
  const rawTitle = readGpuiRecordString(payload, "title");
  if (rawTitle === undefined || GPUI_RENDERER_COMMAND_RENAME_TITLE_CONTROL_PATTERN.test(rawTitle)) {
    return undefined;
  }
  const title = rawTitle.trim();
  if (!title || title.length > GPUI_RENDERER_COMMAND_RENAME_TITLE_MAX_CHARS) {
    return undefined;
  }
  return title;
}

function readGpuiRendererCommandSessionTarget(
  payload: Record<string, unknown>,
): Record<string, unknown> | undefined {
  const target = payload.sessionTarget;
  return isObjectRecord(target) && !Array.isArray(target) ? target : undefined;
}

function readGpuiRecordString(
  record: Record<string, unknown> | undefined,
  key: string,
): string | undefined {
  const value = record?.[key];
  return typeof value === "string" ? value : undefined;
}

function parseGpuiRendererCommandGlobalSessionRef(
  globalRef: string | undefined,
): { projectId: string; sessionId: string } | undefined {
  const parts = globalRef?.trim().split(":");
  if (parts?.length !== 3 || !parts[1] || !parts[2]) {
    return undefined;
  }
  return {
    projectId: parts[1],
    sessionId: parts[2],
  };
}

function isPresentationSnapshot(value: unknown): value is GxserverPresentationSnapshot {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    Array.isArray((value as GxserverPresentationSnapshot).groups) &&
    Array.isArray((value as GxserverPresentationSnapshot).projects) &&
    Array.isArray((value as GxserverPresentationSnapshot).sessions) &&
    typeof (value as GxserverPresentationSnapshot).revision === "number"
  );
}

function isPresentationDelta(value: unknown): value is GxserverPresentationDelta {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    typeof (value as { type?: unknown }).type === "string"
  );
}

function isSidebarProjectCollectionsState(
  value: unknown,
): value is GxserverSidebarProjectCollectionsState {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    typeof (value as GxserverSidebarProjectCollectionsState).collections === "object" &&
    !Array.isArray((value as GxserverSidebarProjectCollectionsState).collections) &&
    Array.isArray((value as GxserverSidebarProjectCollectionsState).order) &&
    typeof (value as GxserverSidebarProjectCollectionsState).nextCollectionNumber === "number"
  );
}

function isGpuiSessionChatEventMessage(
  value: Record<string, unknown>,
): value is Record<string, unknown> & GxserverSessionChatEvent {
  /*
  CDXC:SessionChatCore 2026-07-31:
  Shape validator for the four sessionChat* event frames, matching the
  presentation-frame validator pattern: identity + epoch/seq cursors must be
  present before a handler sees the frame. Message-array payloads are trusted
  from the authenticated local socket like presentation snapshots are.
  */
  if (
    typeof value.projectId !== "string" ||
    value.projectId.length === 0 ||
    typeof value.sessionId !== "string" ||
    value.sessionId.length === 0 ||
    typeof value.epoch !== "number" ||
    typeof value.seq !== "number"
  ) {
    return false;
  }
  if (
    (value.type === "sessionChatSnapshot" ||
      value.type === "sessionChatAppended" ||
      value.type === "sessionChatReplaced") &&
    !Array.isArray(value.messages)
  ) {
    return false;
  }
  if (value.type === "sessionChatState" && typeof value.status !== "string") {
    return false;
  }
  return true;
}

function normalizeGpuiSidebarRemoteEvent(value: unknown): GpuiSidebarRemoteEvent | undefined {
  const event = parseObject(value);
  if (!event || typeof event.type !== "string") {
    return undefined;
  }
  if (event.type === "remoteMachineStatus") {
    const machineId = normalizeNonEmptyString(event.machineId);
    const state = event.state;
    if (!machineId || !GPUI_REMOTE_MACHINE_STATUS_STATES.has(state as string)) {
      return undefined;
    }
    const message = normalizeNonEmptyString(event.message)?.slice(
      0,
      GPUI_REMOTE_MACHINE_STATUS_MESSAGE_MAX_CHARS,
    );
    return {
      machineId,
      ...(message ? { message } : {}),
      state: state as SidebarRemoteMachineStatusMessage["state"],
      type: "remoteMachineStatus",
    };
  }
  if (event.type === "remoteGxserverResponse") {
    const remoteMachineId = normalizeNonEmptyString(event.remoteMachineId);
    const requestId = normalizeNonEmptyString(event.requestId);
    if (!remoteMachineId || !requestId || typeof event.ok !== "boolean") {
      return undefined;
    }
    return {
      error: normalizeNonEmptyString(event.error),
      ok: event.ok,
      remoteMachineId,
      requestId,
      result: event.result,
      type: "remoteGxserverResponse",
    };
  }
  if (event.type !== "remoteGxserverPresentation") {
    return undefined;
  }
  const remoteMachineId = normalizeNonEmptyString(event.remoteMachineId);
  const payload = parseObject(event.payload);
  if (!remoteMachineId || !payload || typeof payload.type !== "string") {
    return undefined;
  }
  if (payload.type === "presentationSnapshot" && isPresentationSnapshot(payload.snapshot)) {
    return {
      payload: {
        snapshot: payload.snapshot,
        type: "presentationSnapshot",
      },
      remoteMachineId,
      type: "remoteGxserverPresentation",
    };
  }
  if (
    payload.type === "presentationDelta" &&
    isPresentationDelta(payload.delta) &&
    typeof payload.revision === "number"
  ) {
    return {
      payload: {
        delta: payload.delta,
        revision: payload.revision,
        type: "presentationDelta",
      },
      remoteMachineId,
      type: "remoteGxserverPresentation",
    };
  }
  return undefined;
}

const GPUI_REMOTE_MACHINE_STATUS_MESSAGE_MAX_CHARS = 300;
const GPUI_REMOTE_MACHINE_STARTUP_RECONNECT_DELAY_MS = 10_000;
const GPUI_REMOTE_MACHINE_STARTUP_MAX_RETRIES = 3;

const GPUI_REMOTE_MACHINE_STARTUP_RETRY_STATES = new Set<
  SidebarRemoteMachineStatusMessage["state"]
>([
  "disconnected",
  "failed",
  "keychainFailed",
  "presentationSubscribeFailed",
  "sshFailed",
  "tokenUnavailable",
  "tunnelFailed",
]);

const GPUI_REMOTE_MACHINE_STATUS_STATES = new Set([
  "connecting",
  "connected",
  "disconnected",
  "downloadingRemoteServerPackage",
  "installFailed",
  "installApprovalRequired",
  "installing",
  "invalid",
  "keychainFailed",
  "presentationStreamFailed",
  "presentationSubscribeFailed",
  "sshFailed",
  "tokenUnavailable",
  "tunnelFailed",
  "unsupported",
  "unsupportedRemotePlatform",
  "failed",
]);

const GPUI_REMOTE_MACHINE_PRESENTATION_CLEAR_STATES = new Set([
  "disconnected",
  "failed",
  "installApprovalRequired",
  "installFailed",
  "invalid",
  "keychainFailed",
  "presentationSubscribeFailed",
  "sshFailed",
  "tokenUnavailable",
  "tunnelFailed",
  "unsupported",
  "unsupportedRemotePlatform",
]);
