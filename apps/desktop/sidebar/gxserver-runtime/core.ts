/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiWorkspaceSessionGroupsState } from '../workspace-session-groups';
import {
  createEmptyGpuiWorkspaceSessionGroupsState,
  parseGpuiWorkspaceSessionSubgroupId,
  readStoredGpuiWorkspaceSessionGroupsState,
} from '../workspace-session-groups';
import type { GpuiSidebarRuntimeAppShotAndMiscMethods } from './app-shot-and-misc';
import { gpuiSidebarRuntimeAppShotAndMiscMethods } from './app-shot-and-misc';
import type { GpuiSidebarRuntimeAttentionMethods } from './attention-tracking';
import { gpuiSidebarRuntimeAttentionMethods } from './attention-tracking';
import type { GpuiSidebarRuntimeAutoSleepMethods } from './auto-sleep';
import { gpuiSidebarRuntimeAutoSleepMethods } from './auto-sleep';
import { GpuiGxserverClient } from './client';
import type { GpuiSidebarRuntimeCloseAfterDoneMethods } from './close-after-done';
import { gpuiSidebarRuntimeCloseAfterDoneMethods } from './close-after-done';
import {
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_REMOTE_MACHINE_PRESENTATION_CLEAR_STATES,
  GPUI_REMOTE_MACHINE_RECONNECT_PROGRESS_STATES,
  GPUI_REMOTE_MACHINE_RECONNECT_STOP_STATES,
  GPUI_REMOTE_MACHINE_RETRY_STATES,
  GPUI_SIDEBAR_NAVIGATION_HISTORY_COMMAND_EVENT_NAME,
  GPUI_SIDEBAR_REMOTE_EVENT_NAME,
} from './constants';
import type { GpuiSidebarRuntimeExportTranscriptMethods } from './export-transcript';
import { gpuiSidebarRuntimeExportTranscriptMethods } from './export-transcript';
import type { GpuiSidebarRuntimeGitMethods } from './git';
import { gpuiSidebarRuntimeGitMethods } from './git';
import {
  createEmptyGpuiAppUserData,
  currentGpuiRuntimeSettings,
  hasSameGpuiRuntimeSettings,
} from './helpers/bootstrap';
import {
  gpuiBrowserSidebarSessionId,
  normalizeGpuiBrowserTabRevealRequest,
  normalizeGpuiBrowserTabs,
  normalizeGpuiDisplayedWorkspaceSessionIds,
} from './helpers/browser-tabs';
import { readStoredGpuiCloseAfterDoneSessionIds } from './helpers/close-after-done';
import {
  createGpuiSidebarHudState,
  hasSameGpuiCommandPaneSessions,
  normalizeGpuiCommandPaneSessions,
  normalizeGpuiWorkspaceSessionDelayedSends,
} from './helpers/command-pane';
import {
  readStoredGpuiRemoteGroupOrder,
  readStoredGpuiRemoteLastSeenPresentations,
  readStoredGpuiRemoteRecentProjects,
} from './helpers/recent-projects';
import { normalizeNonEmptyString } from './helpers/records';
import {
  normalizeGpuiSidebarRemoteEvent,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import type { GpuiSidebarRuntimePresentationStreamMethods } from './presentation-stream';
import { gpuiSidebarRuntimePresentationStreamMethods } from './presentation-stream';
import type { GpuiSidebarRuntimePreviousSessionMethods } from './previous-sessions';
import { gpuiSidebarRuntimePreviousSessionMethods } from './previous-sessions';
import type { GpuiSidebarRuntimeProjectBoardMethods } from './project-board';
import { gpuiSidebarRuntimeProjectBoardMethods } from './project-board';
import type { GpuiSidebarRuntimeProjectAndCommandMethods } from './projects-and-commands';
import { gpuiSidebarRuntimeProjectAndCommandMethods } from './projects-and-commands';
import type { GpuiSidebarRuntimeRemoteMachineMethods } from './remote-machines';
import { gpuiSidebarRuntimeRemoteMachineMethods } from './remote-machines';
import type { GpuiSidebarRuntimeConversationJumpMethods } from './session-conversation-jump';
import { gpuiSidebarRuntimeConversationJumpMethods } from './session-conversation-jump';
import type { GpuiSidebarRuntimeSessionCreateMethods } from './session-create';
import { gpuiSidebarRuntimeSessionCreateMethods } from './session-create';
import type { GpuiSidebarRuntimeSessionFocusMethods } from './sessions-and-focus';
import { gpuiSidebarRuntimeSessionFocusMethods } from './sessions-and-focus';
import type { GpuiSidebarRuntimeSidebarGroupMethods } from './sidebar-groups';
import { gpuiSidebarRuntimeSidebarGroupMethods } from './sidebar-groups';
import type { GpuiSidebarRuntimeStashedPromptJumpMethods } from './stashed-prompt-jump';
import { gpuiSidebarRuntimeStashedPromptJumpMethods } from './stashed-prompt-jump';
import type { GpuiSidebarRuntimeTerminalLifecycleMethods } from './terminal-lifecycle-queue';
import { gpuiSidebarRuntimeTerminalLifecycleMethods } from './terminal-lifecycle-queue';
import type {
  GpuiBrowserTabSummary,
  GpuiCloseAfterDoneTimer,
  GpuiCommandPaneSessionSummary,
  GpuiExportTranscriptRequestContext,
  GpuiExportedTranscriptResult,
  GpuiPendingGitCommitRequest,
  GpuiPendingNativeAppShotPromptInsertion,
  GpuiPendingRemoteGxserverRequest,
  GpuiPresentationSubscription,
  GpuiProjectWorktreesResultMessage,
  GpuiSidebarGitHubState,
  GpuiSidebarRuntimeSettings,
  GpuiTrustedExistingWorktreeList,
  GpuiValidatedGxserverBootstrap,
  GpuiWorkspaceSessionDelayedSendSummary,
  GpuiWorkspaceTerminalTitleChangedPayload,
} from './types-and-protocol';
import type { GpuiSidebarRuntimeWorkspaceGroupMethods } from './workspace-groups-sync';
import { gpuiSidebarRuntimeWorkspaceGroupMethods } from './workspace-groups-sync';
import type { GpuiSidebarRuntimeWorktreeMethods } from './worktrees';
import { gpuiSidebarRuntimeWorktreeMethods } from './worktrees';
import type { WebviewApi } from '@/packages/core-ui/webview-api';
import { reduceGxserverPresentationDelta } from '@/packages/shared/gxserver-presentation-cache';
import type {
  GxserverAppUserData,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverSidebarHudResponse,
  GxserverSidebarProjectCollectionsState,
} from '@/packages/shared/gxserver-protocol';
import { NAVIGATION_HISTORY_SCOPE_GPUI } from '@/packages/shared/navigation-history/navigation-history-contract';
import { NavigationHistoryController } from '@/packages/shared/navigation-history/navigation-history-controller';
import type { SidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import type {
  ExtensionToSidebarMessage,
  SidebarGroupsChangedMessage,
  SidebarHudChangedMessage,
  SidebarHudState,
  SidebarHydrateMessage,
  SidebarOrderSyncResultMessage,
  SidebarPreviousSessionItem,
  SidebarPreviousSessionsResultMessage,
  SidebarSessionGroup,
  SidebarToExtensionMessage,
} from '@/packages/shared/session-grid-contract';
import { isSidebarCommandScope } from '@/packages/shared/sidebar-commands';
import type { SidebarGitState } from '@/packages/shared/sidebar-git';
import { createDefaultSidebarGitState } from '@/packages/shared/sidebar-git';
import {
  SIDEBAR_GIT_HUB_MEMO_TTL_MS,
  SIDEBAR_GIT_STATE_MEMO_TTL_MS,
  SidebarGitTtlMemo,
} from '@/packages/shared/sidebar-git-state-memo';

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

CDXC:GPUIRemoteNativeActions 2026-08-14:
Remote Recent Projects opens a real project-scoped terminal through the fixed `openRemoteProjectTerminal` selector. React sends only the machine-scoped project id; Rust restores the parked project, creates the remote gxserver terminal, and owns all SSH attach metadata and terminal launch payloads.

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
    options?: AddEventListenerOptions | boolean
  ): void {
    this.eventTarget.addEventListener(type, listener, options);
  }

  removeEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: EventListenerOptions | boolean
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
      | GpuiProjectWorktreesResultMessage
  ): void {
    this.eventTarget.dispatchEvent(
      new MessageEvent('message', {
        data: message,
      })
    );
  }
}

export class GpuiSidebarRuntime {
  readonly messageSource = new GpuiSidebarLocalMessageSource();
  readonly vscode: WebviewApi = {
    postMessage: (message) => {
      void this.handleSidebarMessage(message);
    },
  };

  startLocalGxserver(): void {
    window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
      type: 'startGxserverFromTitlebar',
    });
  }

  notifyNativeGxserverPresentationReady(): void {
    window.requestAnimationFrame(() => {
      if (!this.presentation) {
        return;
      }
      window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
        type: 'gxserverPresentationReady',
      });
    });
  }

  activeProjectContextRetryId: number | undefined;
  titlebarGitMenuStateRetryId: number | undefined;
  lastTitlebarGitMenuStatePayload: string | undefined;
  gitPollingCycleTimeoutId: number | undefined;
  gitPollingTimeoutIds = new Set<number>();
  pendingProjectDiffRefreshProjectIds = new Set<string>();
  projectDiffStatsByProjectId = new Map<string, SidebarProjectDiffStats>();
  /*
  CDXC:SidebarDiffStatsChurn 2026-08-16:
  Projects (plain local ids, machine-scoped remote ids) whose cwd answered
  `isInsideWorkTree` with true. Repo-ness effectively never changes at runtime,
  so steady-state polling skips that probe and goes straight to `diffNumstat`;
  a failed numstat drops the entry so the next cycle re-probes from scratch.
  */
  gitRepoProjectIds = new Set<string>();
  activeGroupId: string | undefined;
  activeProjectId: string | undefined;
  lastNavigationHistoryStatePayload: string | undefined;
  readonly navigationHistory = new NavigationHistoryController({
    activate: (entry) => this.activateNavigationHistoryEntry(entry),
    onStateChange: (state) => this.postNavigationHistoryState(state),
    resolveRpc: () => this.navigationHistoryRpc(),
    scopeId: NAVIGATION_HISTORY_SCOPE_GPUI,
  });
  appUserData: GxserverAppUserData = createEmptyGpuiAppUserData();
  attentionAcknowledgementTimeoutsBySessionKey = new Map<string, number>();
  attentionCompletionSoundEventKeys = new Set<string>();
  attentionCompletionSoundEventKeyOrder: string[] = [];
  attentionCompletionSoundSuppressedUntilBySessionKey = new Map<string, number>();
  attentionEnteredAtBySessionKey = new Map<string, number>();
  attentionEventIdBySessionKey = new Map<string, string>();
  autoSleepMonitorIntervalId: number | undefined;
  autoSleepMonitorRunning = false;
  bootstrapPollTimeoutId: number | undefined;
  browserTabs: GpuiBrowserTabSummary[] = [];
  client: GpuiGxserverClient | undefined;
  closeAfterDoneCountdownTickerId: number | undefined;
  closeAfterDoneTimersBySessionId = new Map<string, GpuiCloseAfterDoneTimer>();
  commandPaneSessions: GpuiCommandPaneSessionSummary[] = [];
  displayedWorkspaceSessionIds: string[] = [];
  workspaceSessionDelayedSends = new Map<string, GpuiWorkspaceSessionDelayedSendSummary>();
  workspaceTerminalTitleObservations = new Map<string, GpuiWorkspaceTerminalTitleChangedPayload>();
  workspaceTerminalTitleSettleTimeouts = new Map<string, number>();
  domainProjects: GxserverProjectDomainState[] = [];
  focusedSessionId: string | undefined;
  gxserverBootstrap: GpuiValidatedGxserverBootstrap | undefined;
  gitState: SidebarGitState = createDefaultSidebarGitState();
  hasHydrated = false;
  latestGroups: SidebarSessionGroup[] = [];
  latestHud: SidebarHudState = createGpuiSidebarHudState();
  localFirstHiddenPresentationSessionKeys = new Set<string>();
  lastAppShotTargetAt = 0;
  lastAppShotTargetSessionId: string | undefined;
  /**
   * Which project the active Git HUD slot currently reflects. This is a
   * *presentation* marker, not a cache: it stops every republish of the same
   * project from re-entering the refresh path. Cross-project freshness lives in
   * `gitStateMemoByProjectId` below.
   */
  lastGitRefreshProjectId: string | undefined;
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
  gitStateMemoByProjectId = new SidebarGitTtlMemo<SidebarGitState>({
    ttlMs: SIDEBAR_GIT_STATE_MEMO_TTL_MS,
  });
  /*
  CDXC:SidebarGitMemo 2026-07-29:
  GitHub CLI results get their own, much longer lease because `gh pr view` is a
  network round trip and pull-request state changes on a human timescale. Kept
  separate from the Git-state memo so a deferred probe landing later can be
  overlaid onto an already-published (or already-memoized) local Git state.
  */
  gitHubStateMemoByProjectId = new SidebarGitTtlMemo<GpuiSidebarGitHubState>({
    ttlMs: SIDEBAR_GIT_HUB_MEMO_TTL_MS,
  });
  pendingGitHubProbeProjectIds = new Set<string>();
  gitHubProbeTimeoutIds = new Set<number>();
  locallyAcknowledgedAttentionEventKeys = new Set<string>();
  locallyAcknowledgedAttentionEventKeyOrder: string[] = [];
  pendingNativeAppShotPromptInsertions: GpuiPendingNativeAppShotPromptInsertion[] = [];
  pendingGitCommitRequests = new Map<string, GpuiPendingGitCommitRequest>();
  pendingRemoteGxserverRequests = new Map<string, GpuiPendingRemoteGxserverRequest>();
  /*
  CDXC:ExportTranscript 2026-08-20:
  What the open Export Transcript result dialog is describing. The dialog is a
  separate child window with no gxserver client, so it sends back only the agent
  the user picked; the exported path and the project the export came from stay
  here, where "Start new conversation" can create the follow-up session in the
  same project without trusting a path posted back by a page.
  */
  pendingExportedTranscript: GpuiExportedTranscriptResult | undefined;
  /*
  CDXC:ExportTranscriptOptions 2026-08-24:
  Which session the open Export Transcript dialog is about while the user is
  still on the include-toggle stage; the dialog's export request carries only
  the toggles back.
  */
  pendingExportTranscriptRequest: GpuiExportTranscriptRequestContext | undefined;
  presentation: GxserverPresentationSnapshot | undefined;
  previousSessionsByHistoryId = new Map<string, SidebarPreviousSessionItem>();
  projectBoardRestorableLinkChecks = new Map<
    string,
    { checkedAt: number; restorable: boolean; resumable: boolean; title?: string }
  >();
  quickAutomationsOverviewOpen = false;
  previousSessionsResult:
    | {
        cursor?: string;
        previousSessions: SidebarPreviousSessionItem[];
        query?: string;
        requestId: string;
      }
    | undefined;
  recentProjects: GxserverRecentProjectDomainState[] = [];
  remoteGxserverRequestSequence = 0;
  remotePresentations = new Map<string, GxserverPresentationSnapshot>();
  remoteLastSeenPresentations = new Map<string, GxserverPresentationSnapshot>();
  remoteLastSeenPersistTimeoutId: number | undefined;
  remoteReconnectAttempts = new Map<string, number>();
  remoteReconnectInFlight = new Set<string>();
  remoteReconnectTimeouts = new Map<string, number>();
  remoteRecentProjectsByMachineId = new Map<string, GxserverRecentProjectDomainState[]>();
  remoteGroupOrderByMachineId = new Map<string, string[]>();
  revision = 0;
  runtimeSettings: GpuiSidebarRuntimeSettings | undefined;
  sidebarHudState: GxserverSidebarHudResponse | undefined;
  postedGlobalActionsPayload: string | undefined;

  /*
   * CDXC:GlobalActions 2026-08-01:
   * The gpui tab strip renders Global Actions natively and cannot read this
   * runtime's state, so every HUD change has to push the list across the
   * bridge. Routing all HUD writes through this accessor is what guarantees a
   * new assignment site cannot forget the push and leave the strip stale.
   */
  get sidebarHud(): GxserverSidebarHudResponse | undefined {
    return this.sidebarHudState;
  }

  set sidebarHud(hud: GxserverSidebarHudResponse | undefined) {
    this.sidebarHudState = hud;
    this.postGpuiGlobalActions();
  }
  sleepingLocalSidebarSessionIds = new Set<string>();
  subscription: GpuiPresentationSubscription | undefined;
  trustedExistingWorktreeList: GpuiTrustedExistingWorktreeList | undefined;
  visibleSessionIds = new Set<string>();
  didAutoMaterializeStartupSession = false;
  didConnectSavedRemoteMachinesOnStartup = false;
  workspaceGroups: GpuiWorkspaceSessionGroupsState = createEmptyGpuiWorkspaceSessionGroupsState();
  workspaceGroupsServerSyncTimeoutId: number | undefined;
  workspaceGroupsServerSyncPending = false;
  latestSidebarProjectCollectionsUpdate: GxserverSidebarProjectCollectionsState | undefined;
  sidebarProjectCollectionsServerSyncTimeoutId: number | undefined;
  sidebarProjectCollectionsServerSyncPending = false;
  lastForwardedSidebarProjectCollectionsJson: string | undefined;
  lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId = new Map<string, string>();
  workspaceTerminalLifecycleBridgeRetryId: number | undefined;

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
    window.addEventListener(
      GPUI_SIDEBAR_NAVIGATION_HISTORY_COMMAND_EVENT_NAME,
      this.handleGpuiSidebarNavigationHistoryCommand
    );
    this.publishUnavailable('bootstrap-pending');
    this.tryStartFromInstalledBootstrap(0);
    this.startGpuiAutoSleepMonitor();
    this.startGitPollingDriver();
    window.setTimeout(() => this.connectSavedRemoteMachinesOnStartup(), 0);
  }

  installGpuiBridgeCallbacks(): void {
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

      CDXC:GxserverRuntimeSplit 2026-08-22:
      `removeProject` (Settings → Projects → Remove) is dispatched by Rust over
      this same bridge and belongs in that list: it too has no inbound React
      branch, so it was reaching the message source and dying there. The union
      in `GpuiSidebarHostMessage` is what makes the remaining fall-through
      provably an extension-to-sidebar message.
      */
      if (
        message.type === 'renameSession' ||
        message.type === 'scheduleDelayedSend' ||
        message.type === 'cancelDelayedSend' ||
        message.type === 'removeProject' ||
        message.type === 'setSessionNote' ||
        message.type === 'toggleCloseAfterDone'
      ) {
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
        this.publishPresentation('patch');
      }
    };
    gpuiBridge.onBrowserTabsChanged = applyBrowserTabs;
    applyBrowserTabs(gpuiBridge.browserTabs);
    gpuiBridge.onRevealBrowserTab = (payload) => {
      const request = normalizeGpuiBrowserTabRevealRequest(payload);
      if (!request) {
        return;
      }
      this.messageSource.postMessage({
        requestId: request.requestId,
        sessionId: gpuiBrowserSidebarSessionId(request),
        type: 'revealSidebarSession',
      });
    };
    gpuiBridge.onWorkspaceTerminalLifecycleRequest = (payload) => {
      /*
      CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
      GPUI native workspace lifecycle must follow macOS ownership: Rust commits Close locally before this callback and uses the sidebar only for asynchronous provider cleanup, while Sleep/Wake still report transition success through the fixed result bridge. Payloads are bounded ids plus action/request enums only; no titles, paths, commands, terminal text, URLs, tokens, or daemon bodies cross this callback.

      CDXC:GPUIWorkspaceLifecycle 2026-06-26-05:23:
      The callback may be installed before CEF exposes `postWorkspaceTerminalLifecycleResult`. Queue normalized requests until that bridge exists so Close provider cleanup and acknowledged Sleep/Wake transitions are not lost during startup.
      */
      this.handleOrQueueWorkspaceTerminalLifecycleRequest(payload);
    };
    const applyCommandPaneSessions = (sessions: readonly GpuiCommandPaneSessionSummary[] | undefined) => {
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
    const applyDisplayedWorkspaceSessionIds = (sessionIds: readonly string[] | undefined) => {
      /*
      CDXC:AutoSleepDisplayedSessions 2026-08-20:
      Rust owns what is actually on screen. This runtime's own visible/focused
      sets are a click-history projection: they cannot see that a session's
      terminal is parked behind its chat surface, and they are dropped whenever
      gxserver goes away, so a session the user was sitting in front of could be
      retired by the "Sleep idle agent sessions" sweep. Cache the ids the same
      way the command-pane bridge does so a restored tab hydrates before React
      installs listeners.
      */
      const next = normalizeGpuiDisplayedWorkspaceSessionIds(sessionIds);
      gpuiBridge.displayedWorkspaceSessionIds = next;
      this.displayedWorkspaceSessionIds = next;
    };
    gpuiBridge.onDisplayedWorkspaceSessionIdsChanged = applyDisplayedWorkspaceSessionIds;
    applyDisplayedWorkspaceSessionIds(gpuiBridge.displayedWorkspaceSessionIds);
    const applyWorkspaceSessionDelayedSends = (
      sessions: readonly GpuiWorkspaceSessionDelayedSendSummary[] | undefined
    ) => {
      const next = normalizeGpuiWorkspaceSessionDelayedSends(sessions);
      gpuiBridge.workspaceSessionDelayedSends = next;
      const nextBySessionId = new Map(next.map((session) => [session.sessionId, session]));
      if (JSON.stringify([...this.workspaceSessionDelayedSends.values()]) === JSON.stringify(next)) {
        return;
      }
      this.workspaceSessionDelayedSends = nextBySessionId;
      if (this.presentation) {
        this.publishPresentation('patch');
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
    gpuiBridge.onStashedPromptSessionJump = (payload) => {
      void this.handleGpuiStashedPromptSessionJump(payload);
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
    gpuiBridge.onWorkspaceTerminalTitleChanged = (payload) => {
      this.handleGpuiWorkspaceTerminalTitleChanged(payload);
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
    gpuiBridge.onExportTranscriptModalCommand = (payload) => {
      void this.handleGpuiExportTranscriptModalCommand(payload);
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
    const pendingMenuBarProjectActivations = Array.isArray(gpuiBridge.pendingMenuBarProjectActivations)
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
    const pendingMenuBarSessionActivations = Array.isArray(gpuiBridge.pendingMenuBarSessionActivations)
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
      gpuiBridge.pendingCommandPaletteSessionFocusRequests
    )
      ? gpuiBridge.pendingCommandPaletteSessionFocusRequests.splice(0)
      : [];
    for (const payload of pendingCommandPaletteSessionFocusRequests) {
      void this.handleGpuiCommandPaletteSessionFocus(payload);
    }
    const pendingCommandPaletteRunSidebarCommands = Array.isArray(gpuiBridge.pendingCommandPaletteRunSidebarCommands)
      ? gpuiBridge.pendingCommandPaletteRunSidebarCommands.splice(0)
      : [];
    for (const payload of pendingCommandPaletteRunSidebarCommands) {
      this.handleGpuiCommandPaletteRunSidebarCommand(payload);
    }
    const pendingStashedPromptSessionJumps = Array.isArray(gpuiBridge.pendingStashedPromptSessionJumps)
      ? gpuiBridge.pendingStashedPromptSessionJumps.splice(0)
      : [];
    for (const payload of pendingStashedPromptSessionJumps) {
      void this.handleGpuiStashedPromptSessionJump(payload);
    }
    const pendingProjectBoardConversationRequests = Array.isArray(gpuiBridge.pendingProjectBoardConversationRequests)
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
    const pendingWorkspaceTabSessionSelections = Array.isArray(gpuiBridge.pendingWorkspaceTabSessionSelections)
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
      gpuiBridge.pendingWorkspaceTerminalLifecycleRequests
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
      gpuiBridge.pendingWorkspaceSessionAttentionAcknowledgements
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
    const pendingWorkspaceTerminalTitleChanges = Array.isArray(gpuiBridge.pendingWorkspaceTerminalTitleChanges)
      ? gpuiBridge.pendingWorkspaceTerminalTitleChanges.splice(0)
      : [];
    for (const payload of pendingWorkspaceTerminalTitleChanges) {
      this.handleGpuiWorkspaceTerminalTitleChanged(payload);
    }
    const pendingWorkspaceTerminalEscapePresses = Array.isArray(gpuiBridge.pendingWorkspaceTerminalEscapePresses)
      ? gpuiBridge.pendingWorkspaceTerminalEscapePresses.splice(0)
      : [];
    for (const payload of pendingWorkspaceTerminalEscapePresses) {
      this.handleGpuiWorkspaceTerminalEscapePressed(payload);
    }
    const pendingWorkspaceFirstPromptTitleGenerationCancels = Array.isArray(
      gpuiBridge.pendingWorkspaceFirstPromptTitleGenerationCancels
    )
      ? gpuiBridge.pendingWorkspaceFirstPromptTitleGenerationCancels.splice(0)
      : [];
    for (const payload of pendingWorkspaceFirstPromptTitleGenerationCancels) {
      void this.handleGpuiWorkspaceFirstPromptTitleGenerationCancel(payload);
    }
    const pendingWorkspaceTerminalRuntimeActions = Array.isArray(gpuiBridge.pendingWorkspaceTerminalRuntimeActions)
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
    const pendingExportTranscriptModalCommands = Array.isArray(gpuiBridge.pendingExportTranscriptModalCommands)
      ? gpuiBridge.pendingExportTranscriptModalCommands.splice(0)
      : [];
    for (const payload of pendingExportTranscriptModalCommands) {
      void this.handleGpuiExportTranscriptModalCommand(payload);
    }
    const pendingWorktreeModalCommands = Array.isArray(gpuiBridge.pendingWorktreeModalCommands)
      ? gpuiBridge.pendingWorktreeModalCommands.splice(0)
      : [];
    for (const payload of pendingWorktreeModalCommands) {
      this.handleGpuiWorktreeModalCommand(payload);
    }
    const pendingNativeAppShotPromptResults = Array.isArray(gpuiBridge.pendingNativeAppShotPromptResults)
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
      this.reconcileRemoteMachineRetryTargets();
      if (!didChange) {
        return;
      }
      this.publishHudPatch();
      this.postGpuiStatusPetState();
      this.postActiveProjectContext();
      void this.runGpuiAutoSleepMonitor('settings-change');
    };
    gpuiBridge.onGxserverBootstrapChanged = (bootstrap) => {
      this.applyGxserverBootstrapChanged(bootstrap);
    };
  }

  readonly handleGpuiSidebarRemoteEvent = (event: Event): void => {
    const remoteEvent = normalizeGpuiSidebarRemoteEvent((event as CustomEvent<unknown>).detail);
    if (!remoteEvent) {
      return;
    }
    if (remoteEvent.type === 'remoteMachineStatus') {
      this.messageSource.postMessage(remoteEvent);
      if (remoteEvent.state === 'connected') {
        this.resetRemoteReconnect(remoteEvent.machineId);
      } else if (GPUI_REMOTE_MACHINE_RECONNECT_PROGRESS_STATES.has(remoteEvent.state)) {
        this.remoteReconnectInFlight.add(remoteEvent.machineId);
        this.clearRemoteReconnectTimeout(remoteEvent.machineId);
      } else if (GPUI_REMOTE_MACHINE_RETRY_STATES.has(remoteEvent.state)) {
        this.remoteReconnectInFlight.delete(remoteEvent.machineId);
        this.scheduleRemoteReconnect(remoteEvent.machineId);
      } else if (GPUI_REMOTE_MACHINE_RECONNECT_STOP_STATES.has(remoteEvent.state)) {
        this.resetRemoteReconnect(remoteEvent.machineId);
      }
      if (GPUI_REMOTE_MACHINE_PRESENTATION_CLEAR_STATES.has(remoteEvent.state)) {
        const previousPresentation = this.remotePresentations.get(remoteEvent.machineId);
        if (previousPresentation) {
          this.syncRemotePresentationAttentionTracking(remoteEvent.machineId, previousPresentation.sessions, []);
        }
        this.remotePresentations.delete(remoteEvent.machineId);
        this.dropRemotePresentationSessionFocus(remoteEvent.machineId);
        this.publishRemotePresentationPatch();
      }
      return;
    }

    if (remoteEvent.type === 'remoteGxserverResponse') {
      this.resolveRemoteGxserverRequest(remoteEvent);
      return;
    }

    if (remoteEvent.payload.type === 'presentationSnapshot') {
      const previousSessions = this.remotePresentations.get(remoteEvent.remoteMachineId)?.sessions ?? [];
      const snapshot = this.projectRemotePresentationAttentionAcknowledgementGuards(
        remoteEvent.remoteMachineId,
        remoteEvent.payload.snapshot
      );
      const previous = this.remotePresentations.get(remoteEvent.remoteMachineId);
      if (previous && previous.revision > snapshot.revision) {
        return;
      }
      this.remotePresentations.set(remoteEvent.remoteMachineId, snapshot);
      this.pruneRemoteWorkspaceGroupAssignments(remoteEvent.remoteMachineId, snapshot);
      this.syncRemotePresentationAttentionTracking(remoteEvent.remoteMachineId, previousSessions, snapshot.sessions);
      this.publishRemotePresentationPatch();
      return;
    }

    const previous = this.remotePresentations.get(remoteEvent.remoteMachineId);
    if (!previous) {
      void this.refreshRemotePresentationFromGxserver(remoteEvent.remoteMachineId).catch(() => undefined);
      return;
    }
    if (remoteEvent.payload.revision <= previous.revision) {
      void this.refreshRemotePresentationFromGxserver(remoteEvent.remoteMachineId).catch(() => undefined);
      return;
    }
    const snapshot = this.projectRemotePresentationAttentionAcknowledgementGuards(
      remoteEvent.remoteMachineId,
      reduceGxserverPresentationDelta(previous, remoteEvent.payload.delta, remoteEvent.payload.revision)
    );
    this.remotePresentations.set(remoteEvent.remoteMachineId, snapshot);
    this.pruneRemoteWorkspaceGroupAssignments(remoteEvent.remoteMachineId, snapshot);
    this.syncRemotePresentationAttentionTracking(remoteEvent.remoteMachineId, previous.sessions, snapshot.sessions);
    this.publishRemotePresentationPatch();
  };

  readonly handleGpuiSidebarNavigationHistoryCommand = (event: Event): void => {
    const detail = (event as CustomEvent<unknown>).detail;
    const direction = detail && typeof detail === 'object' ? (detail as { direction?: unknown }).direction : undefined;
    if (direction !== 'back' && direction !== 'forward') {
      return;
    }
    void this.navigationHistory.navigate(direction);
  };

  async handleSidebarMessage(message: SidebarToExtensionMessage): Promise<void> {
    switch (message.type) {
      case 'sidebarDebugLog':
        window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
          details: message.details,
          event: message.event,
          scenarioId: message.scenarioId,
          type: 'sidebarDiagnosticLog',
        });
        return;
      case 'focusGroup':
        this.focusGroup(message.groupId, message);
        return;
      case 'focusSession':
        await this.focusSession(message.sessionId, message);
        this.postSidebarSessionFocusConfirmation(message.sessionId);
        return;
      case 'jumpToStashedPromptSession':
        await this.jumpToStashedPromptSession(message);
        return;
      case 'focusSessionMode':
        if (parseGpuiRemotePresentationSessionId(message.sessionId)) {
          await this.focusSession(message.sessionId, message);
          this.postSidebarSessionFocusConfirmation(message.sessionId);
          return;
        }
        this.handleUnsupportedSidebarMessage(message);
        return;
      case 'createSession':
        await this.createSession();
        return;
      case 'createSessionInGroup':
        await this.createSession(message.groupId);
        return;
      case 'createProjectTerminal':
        await this.createProjectTerminal(message);
        return;
      case 'createChat':
        await this.createQuickTerminal();
        return;
      case 'openBrowserChat':
        this.openQuickBrowserTab();
        return;
      case 'openBrowserPaneInGroup':
        this.openBrowserPaneInGroup(message.groupId);
        return;
      case 'runSidebarAgent':
        if (message.groupId === GPUI_GXSERVER_CHATS_GROUP_ID) {
          await this.createQuickAgentSession(message.agentId);
          return;
        }
        await this.createAgentSession(message.agentId, message.groupId);
        return;
      case 'runSidebarCommand': {
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
        this.runSidebarCommand(commandId, message, message.scope ?? 'project');
        return;
      }
      case 'runGhostexHotkeyAction': {
        this.postGhostexHotkeyAction(message);
        return;
      }
      case 'endSidebarCommandRun': {
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
      case 'setSessionSleeping':
        await this.setSessionSleeping(message.sessionId, message.sleeping);
        return;
      case 'setSessionsSleeping':
        await this.setSessionsSleeping(message.sessionIds, message.sleeping);
        return;
      case 'setGroupSleeping':
        await this.setGroupSleeping(message.groupId, message.sleeping);
        return;
      case 'closeSession':
        await this.transitionSession(message.sessionId, 'close');
        return;
      case 'closeSessions':
        await Promise.all(message.sessionIds.map((sessionId) => this.transitionSession(sessionId, 'close')));
        return;
      case 'copySessionDetails':
        this.copySessionDetails(message);
        return;
      case 'fullReloadSession':
      case 'restartSession':
        await this.fullReloadSession(message.sessionId);
        return;
      case 'fullReloadProjectZmxSessions':
        await this.fullReloadProjectZmxSessions(message.groupId);
        return;
      case 'fullReloadGroup':
        await this.fullReloadWorkspaceGroup(message.groupId);
        return;
      case 'toggleCloseAfterDone':
        this.toggleCloseAfterDone(message.sessionId);
        return;
      case 'scheduleDelayedSend':
        await this.scheduleRemoteDelayedSend(message);
        return;
      case 'cancelDelayedSend':
        await this.cancelRemoteDelayedSend(message.sessionId);
        return;
      case 'openAutomationsPage':
        /*
        CDXC:GPUIAutomationsOverview 2026-07-08:
        Mirror macOS `openQuickAutomationsPage`, `ensureQuickAutomationsProject`,
        and `focusQuickAutomationsProject` from native/sidebar/native-sidebar.tsx:
        create the session-local Quick overview row, focus it, and let the
        existing active-project context post carry the Automate workarea identity.
        */
        this.openQuickAutomationsPage();
        return;
      case 'closeInactiveProjectSessions':
        await this.closeInactiveProjectSessions(message.groupId);
        return;
      case 'sleepInactiveProjectSessions':
        await this.sleepInactiveProjectSessions(message.groupId);
        return;
      case 'wakeProjectSleepingSessions':
        await this.wakeProjectSleepingSessions(message.groupId);
        return;
      case 'forkSession':
        await this.forkSession(message.sessionId);
        return;
      case 'renameSession':
        await this.renameSession(message);
        return;
      case 'setSessionFavorite':
        await this.updateSessionFlags(message.sessionId, {
          isFavorite: message.favorite,
          sessionTag: message.favorite ? 'favorite' : null,
        });
        return;
      case 'setSessionTag':
        await this.updateSessionFlags(message.sessionId, {
          isFavorite: message.sessionTag === 'favorite',
          sessionTag: message.sessionTag ?? null,
        });
        return;
      case 'setSessionPinned':
        await this.updateSessionFlags(message.sessionId, {
          isPinned: message.pinned,
        });
        return;
      case 'setSessionParked':
        await this.updateSessionFlags(message.sessionId, {
          isParked: message.parked,
        });
        return;
      case 'setSessionNote':
        await this.saveSessionNote(message.sessionId, message.note);
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
      case 'settleSession':
        await this.runSessionLifecycleCommand(message.sessionId, '/api/settleSession', {});
        return;
      case 'unsettleSession':
        await this.runSessionLifecycleCommand(message.sessionId, '/api/unsettleSession', {});
        return;
      case 'snoozeSession':
        await this.runSessionLifecycleCommand(message.sessionId, '/api/snoozeSession', {
          snoozedUntil: message.snoozedUntil,
        });
        return;
      case 'unsnoozeSession':
        await this.runSessionLifecycleCommand(message.sessionId, '/api/unsnoozeSession', {});
        return;
      case 'syncSessionOrder':
        if (parseGpuiWorkspaceSessionSubgroupId(message.groupId)) {
          this.syncWorkspaceSubgroupSessionOrder(message.groupId, message.sessionIds);
          return;
        }
        await this.syncSessionOrder(message.groupId, message.sessionIds);
        return;
      case 'createGroup':
        this.createWorkspaceGroup(message.groupId);
        return;
      case 'createGroupFromSession':
        this.createWorkspaceGroupFromSession(message.sessionId);
        return;
      case 'renameGroup':
        this.renameWorkspaceGroup(message.groupId, message.title);
        return;
      case 'closeGroup':
        await this.closeWorkspaceGroup(message.groupId);
        return;
      case 'moveSessionToGroup':
        this.moveSessionToWorkspaceGroup(message);
        return;
      case 'syncGroupOrder':
        this.syncWorkspaceGroupOrder(message.groupIds);
        return;
      case 'updateSidebarProjectCollections':
        if (message.remoteMachineId) {
          await this.updateRemoteSidebarProjectCollections(message.remoteMachineId, message.state);
          return;
        }
        this.queueSidebarProjectCollectionsServerSync(message.state);
        return;
      case 'requestPreviousSessions':
        await this.requestPreviousSessions(message);
        return;
      case 'searchPreviousSessionsByText':
        this.searchPreviousSessionsByText();
        return;
      case 'restorePreviousSession':
        await this.restorePreviousSession(message.historyId);
        return;
      case 'deletePreviousSession':
        await this.deletePreviousSession(message.historyId);
        return;
      case 'copyAttachCommand': {
        const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
        if (remoteSession) {
          this.postRemoteSessionNativeAction('copyRemoteAttachCommand', remoteSession, message);
          return;
        }
        this.handleUnsupportedSidebarMessage(message);
        return;
      }
      case 'copyResumeCommand': {
        const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
        if (remoteSession) {
          this.postRemoteSessionNativeAction('copyRemoteResumeCommand', remoteSession, message);
          return;
        }
        this.handleUnsupportedSidebarMessage(message);
        return;
      }
      case 'requestProjectWorktrees':
        await this.requestProjectWorktrees(message);
        return;
      case 'saveScratchPad':
        await this.saveScratchPad(message.content);
        return;
      case 'savePinnedPrompt':
        await this.savePinnedPrompt(message);
        return;
      case 'createProjectWorktree':
        await this.createProjectWorktree(message);
        return;
      case 'createWorktreeSession':
        await this.createWorktreeSession(message);
        return;
      case 'removeSessionWorktree':
        await this.removeSessionWorktree(message);
        return;
      case 'promptDeleteWorktreeForGroup':
        await this.promptDeleteWorktreeForGroup(message.groupId);
        return;
      case 'confirmDeleteWorktree':
        await this.confirmDeleteWorktree(message);
        return;
      case 'promptRenameWorktreeForGroup':
        await this.promptRenameWorktreeForGroup(message.groupId);
        return;
      case 'confirmRenameWorktree':
        await this.confirmRenameWorktree(message);
        return;
      case 'updateSettingsPatch':
        this.saveSidebarSettingsPatch(message);
        return;
      case 'openExternalUrl':
        this.openExternalUrl(message);
        return;
      case 'openSettings':
        this.openAppModal('settings');
        return;
      case 'openWorkspaceWelcome':
        this.openAppModal('firstLaunchSetup');
        return;
      case 'openHighlightedFeatures':
      case 'openGhostexTutorialVideo':
        this.openAppModal('watchGhostexVideo');
        return;
      case 'reconnectRemoteMachine':
        this.reconnectRemoteMachine(message.remoteMachineId, message.installApproved === true);
        return;
      case 'pickWorkspaceFolder':
        this.pickWorkspaceFolder(message);
        return;
      case 'removeProject':
        await this.removeProject(message.projectId);
        return;
      case 'restoreRecentProject':
        await this.restoreRecentProject(message.projectId);
        return;
      case 'removeRecentProject':
        await this.removeRecentProject(message.projectId);
        return;
      case 'copyRecentProjectPath':
        {
          const remoteProject = parseGpuiRemotePresentationProjectId(message.projectId);
          if (remoteProject) {
            this.postRemoteProjectNativeAction('copyRemoteProjectPath', remoteProject, message);
            return;
          }
        }
        this.postNativeProjectPathAction('copyRecentProjectPath', message.projectId, message);
        return;
      case 'openRecentProjectInFinder':
        {
          const remoteProject = parseGpuiRemotePresentationProjectId(message.projectId);
          if (remoteProject) {
            this.postRemoteProjectNativeAction('openRemoteProjectTerminal', remoteProject, message);
            return;
          }
        }
        this.postNativeProjectPathAction('openRecentProjectInFinder', message.projectId, message);
        return;
      case 'openRecentProjectTerminal':
        {
          const remoteProject = parseGpuiRemotePresentationProjectId(message.projectId);
          if (remoteProject) {
            this.postRemoteProjectNativeAction('openRemoteProjectTerminal', remoteProject, message);
          }
        }
        return;
      case 'closeWorkspaceProjectForGroup':
        await this.closeProjectForGroup(message.groupId);
        return;
      case 'copyWorkspaceProjectPathForGroup':
        this.postProjectPathActionForGroup('copyWorkspaceProjectPath', message.groupId, message);
        return;
      case 'openWorkspaceProjectInFinderForGroup':
        this.postProjectPathActionForGroup('openWorkspaceProjectInFinder', message.groupId, message);
        return;
      case 'openWorkspaceProjectInIdeForGroup':
        this.postProjectPathActionForGroup('openWorkspaceProjectInIde', message.groupId, message);
        return;
      case 'openActiveWorkspaceProjectInFinder':
        this.postActiveProjectPathAction('openActiveWorkspaceProjectInFinder', message);
        return;
      case 'openActiveWorkspaceProjectInIde':
        if (message.targetApp !== 'vscode' && message.targetApp !== 'zed') {
          this.handleUnsupportedSidebarMessage(message);
          return;
        }
        this.postActiveProjectPathAction(
          message.targetApp === 'vscode' ? 'openActiveWorkspaceProjectInVscode' : 'openActiveWorkspaceProjectInZed',
          message
        );
        return;
      case 'removeWorkspaceProjectForGroup':
        await this.removeProjectForGroup(message.groupId);
        return;
      case 'setProjectWorktreeCommand':
        await this.updateProjectWorktreeCommand(message.projectId, message.command);
        return;
      case 'setProjectBeadsDisplayKey':
        await this.updateProjectBeadsDisplayKey(message.projectId, message.displayKey);
        return;
      case 'setProjectBeadsDirectory':
        await this.updateProjectBeadsDirectory(message.projectId, message.directory);
        return;
      case 'setProjectDocsDirectory':
        await this.updateProjectDocsDirectory(message.projectId, message.directory);
        return;
      case 'refreshGitState':
        await this.refreshGitStateForMessage(message);
        return;
      case 'setSidebarGitPrimaryAction':
        await this.persistGitPreferences({ primaryAction: message.action }, message);
        return;
      case 'setSidebarGitCommitConfirmationEnabled':
        await this.persistGitPreferences({ confirmCommit: message.enabled }, message);
        return;
      case 'setSidebarGitGenerateCommitBodyEnabled':
        await this.persistGitPreferences({ generateCommitBody: message.enabled }, message);
        return;
      case 'runSidebarGitAction':
        await this.runSidebarGitAction(message);
        return;
      case 'confirmSidebarGitCommit':
        await this.confirmSidebarGitCommit(message);
        return;
      case 'cancelSidebarGitCommit':
        this.pendingGitCommitRequests.delete(message.requestId);
        this.publishHudPatch();
        return;
      case 'runSidebarGitMultipleCommits':
        await this.runSidebarGitMultipleCommits(message.requestId, message.agentId);
        return;
      case 'confirmSidebarGitDirectMerge':
        await this.confirmSidebarGitDirectMerge(message);
        return;
      case 'commitWorktreeBeforeDelete':
        await this.runSidebarGitAction({
          action: 'commit',
          groupId: message.groupId,
          type: 'runSidebarGitAction',
        });
        return;
      case 'openSidebarGitChangedFileDiff':
        await this.openSidebarGitChangedFileDiff(message.filePath, message.requestId);
        return;
      case 'openSidebarGitChangedFile':
        await this.openSidebarGitChangedFileInIde(message);
        return;
      case 'saveSidebarAgent':
        await this.saveSidebarAgent(message);
        return;
      case 'deleteSidebarAgent':
        await this.deleteSidebarAgent(message.agentId);
        return;
      case 'syncSidebarAgentOrder':
        await this.syncSidebarAgentOrder(message.requestId, message.agentIds);
        return;
      case 'saveSidebarCommand':
        await this.saveSidebarCommand(message);
        return;
      case 'deleteSidebarCommand':
        await this.deleteSidebarCommand(message.commandId);
        return;
      case 'syncSidebarCommandOrder':
        await this.syncSidebarCommandOrder(message.requestId, message.commandIds);
        return;
      case 'saveGlobalSidebarCommand':
        await this.saveGlobalSidebarCommand(message);
        return;
      case 'deleteGlobalSidebarCommand':
        await this.deleteGlobalSidebarCommand(message.commandId);
        return;
      case 'syncGlobalSidebarCommandOrder':
        await this.syncGlobalSidebarCommandOrder(message.requestId, message.commandIds);
        return;
      default:
        this.handleUnsupportedSidebarMessage(message);
        return;
    }
  }

  handleUnsupportedSidebarMessage(_message: SidebarToExtensionMessage): void {
    /*
    CDXC:GPUISidebarGxserverRuntime 2026-06-24-11:00:
    GPUI command parity is intentionally incremental. Unsupported SidebarApp messages must be explicit no-ops in this adapter instead of mutating fixture state, inventing host behavior, logging user content, or pretending native-only Browser/Git/settings/chrome actions succeeded.
    */
  }
}

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
`GpuiSidebarRuntime` is one object with one lifetime; the split only moved its
method bodies into per-responsibility modules. Each of those modules exports a
plain object of methods declared with an explicit `this: GpuiSidebarRuntime`,
and they are copied onto the prototype here. `Object.defineProperty` (rather
than `Object.assign`) is used so the copied methods keep the exact property
attributes a `class` body would have given them: writable, configurable, and
NOT enumerable.

The declaration merge below is what makes `this.someGitMethod()` resolve from
inside `sessions-and-focus.ts` and vice versa: every module sees the whole
merged interface, so the mutual recursion the original class relied on still
type-checks. It works without a circular-inference error only because every
moved method carries an explicit return type annotation.
*/
export interface GpuiSidebarRuntime
  extends
    GpuiSidebarRuntimeGitMethods,
    GpuiSidebarRuntimeWorktreeMethods,
    GpuiSidebarRuntimeSidebarGroupMethods,
    GpuiSidebarRuntimePresentationStreamMethods,
    GpuiSidebarRuntimeSessionFocusMethods,
    GpuiSidebarRuntimeSessionCreateMethods,
    GpuiSidebarRuntimeAutoSleepMethods,
    GpuiSidebarRuntimePreviousSessionMethods,
    GpuiSidebarRuntimeProjectBoardMethods,
    GpuiSidebarRuntimeConversationJumpMethods,
    GpuiSidebarRuntimeStashedPromptJumpMethods,
    GpuiSidebarRuntimeAttentionMethods,
    GpuiSidebarRuntimeCloseAfterDoneMethods,
    GpuiSidebarRuntimeTerminalLifecycleMethods,
    GpuiSidebarRuntimeExportTranscriptMethods,
    GpuiSidebarRuntimeWorkspaceGroupMethods,
    GpuiSidebarRuntimeRemoteMachineMethods,
    GpuiSidebarRuntimeAppShotAndMiscMethods,
    GpuiSidebarRuntimeProjectAndCommandMethods {}

function installGpuiSidebarRuntimeMethods(methods: Record<string, unknown>): void {
  for (const [name, value] of Object.entries(methods)) {
    Object.defineProperty(GpuiSidebarRuntime.prototype, name, {
      value,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }
}

installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeGitMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeWorktreeMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeSidebarGroupMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimePresentationStreamMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeSessionFocusMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeSessionCreateMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeAutoSleepMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimePreviousSessionMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeProjectBoardMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeConversationJumpMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeStashedPromptJumpMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeAttentionMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeCloseAfterDoneMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeTerminalLifecycleMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeExportTranscriptMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeWorkspaceGroupMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeRemoteMachineMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeAppShotAndMiscMethods);
installGpuiSidebarRuntimeMethods(gpuiSidebarRuntimeProjectAndCommandMethods);
