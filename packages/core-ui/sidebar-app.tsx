import { Cursor, KeyboardSensor, PointerSensor } from "@dnd-kit/dom";
import { move } from "@dnd-kit/helpers";
import { DragDropProvider, type DragDropEventHandlers } from "@dnd-kit/react";
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { createPortal } from "react-dom";
import { useShallow } from "zustand/react/shallow";
import {
  type SidebarActiveSessionsSortMode,
  type ExtensionToSidebarMessage,
  type SidebarPreviousSessionItem,
} from "../shared/session-grid-contract";
import {
  getWorkspaceThemeForeground,
  normalizeWorkspaceThemeColor,
} from "../shared/workspace-project-appearance";
import { playCompletionSound, prepareCompletionSoundPlayback } from "./completion-sound-player";
import { GitCommitModal } from "./git-commit-modal";
import { SidebarPreviousSessionsSearchGroup } from "./sidebar-session-search-overlay";
import { readSidebarHiddenItems, writeSidebarHiddenItems } from "./sidebar-hidden-items";
import {
  SidebarCollapseAnimationProvider,
  useSidebarCollapsiblePresence,
} from "./sidebar-collapse-animation";
import {
  createSidebarSessionSearchResults,
  createSidebarSessionSearchSelection,
  getNextSidebarSessionSearchSelection,
  isSidebarSessionSearchSelectionMatch,
  type SidebarSessionSearchSelection,
} from "./sidebar-session-search";
import { logSidebarDebug } from "./sidebar-debug";
import {
  createSidebarRefreshDebugInstanceId,
  postSidebarRefreshDebugLog,
  summarizeSidebarRefreshMessage,
} from "./sidebar-refresh-debug-log";
import {
  hashSidebarCollapseDebugId,
  SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX,
  summarizeSidebarCollapseDebugGroupIds,
} from "./sidebar-collapse-state-debug";
import { postSidebarOrderReproLog } from "./sidebar-order-repro-log";
import { getSidebarReorderActivationConstraints } from "./sidebar-reorder-activation";
import { scrollElementIntoViewIfNeeded } from "./scroll-into-view-if-needed";
import { resetSidebarStore, useSidebarStore } from "./sidebar-store";
import {
  getClientPoint,
  getSidebarDropData,
  type SidebarGroupDropTarget,
  type SidebarSessionDropTarget,
  moveSessionIdsByDropTarget,
} from "./sidebar-dnd";
import {
  getAutoCollapseGroupIds,
  getSessionCountsByGroup,
  reconcileCollapsedGroupsById,
} from "./group-collapse";
import { SessionGroupSection } from "./session-group-section";
import { ProjectCollectionSection } from "./project-collection-section";
import {
  areSidebarProjectCollectionsStatesEqual,
  createSidebarProjectCollection,
  moveProjectsToSidebarCollection,
  parseSidebarProjectCollectionsFromGxserver,
  readSidebarProjectCollections,
  removeSidebarProjectCollection,
  reorderSidebarProjectCollectionDefinitions,
  reorderSidebarProjectCollections,
  serializeSidebarProjectCollectionsForGxserver,
  updateSidebarProjectCollection,
  writeSidebarProjectCollections,
  type SidebarProjectCollectionsState,
} from "./project-collections";
import { TOOLTIP_DELAY_MS } from "./tooltip-delay";
import {
  AppTooltip,
  setSidebarTooltipsSuppressedForDrag,
  TooltipProvider,
  useDismissSidebarTooltipsOnScroll,
} from "./app-tooltip";
import { useScrollGlowState } from "./use-scroll-glow-state";
import type { WebviewApi } from "./webview-api";
import { createDisplaySessionLayout } from "../shared/active-sessions-sort";
import {
  moveSidebarV2GroupRows,
  projectSidebarV2GroupOrderByMachine,
  type SidebarV2GroupOrderRow,
} from "../shared/sidebar-v2-group-order";
import {
  filterDefaultNamedSessionSearchItems,
  filterPreviousSessions,
} from "./previous-session-search";
import { type SidebarSessionTagFilter } from "./session-tag-ui";
import {
  getEnabledVisibleSidebarSessionTagFilters,
  normalizeSidebarSessionTagListItems,
} from "../shared/session-tags";
import { isEmptySidebarDoubleClick } from "./empty-sidebar-double-click";
import { closeAppModal, openAppModal, openQuickAccess } from "./app-modal-host-bridge";
import {
  getghostexHotkeyActionById,
  normalizeghostexHotkeySettings,
} from "../shared/ghostex-hotkeys";
import {
  DEFAULT_ghostex_SETTINGS,
  getSidebarTitlebarForegroundForBackground,
  getSidebarTitlebarGradientColors,
  isDiagnosticLoggingScenarioEnabled,
  type DiagnosticLoggingScenarioId,
  type KeepAwakeDurationMinutes,
  type SidebarNewSessionEnvMode,
  type SidebarProjectGroupingMode,
  type SidebarV2Layout,
  type SidebarVersion,
} from "../shared/ghostex-settings";
import { SidebarV2Root } from "./v2/sidebar-v2-root";
import {
  SIDEBAR_PROJECT_JUMP_EVENT,
  type SidebarProjectJumpEventDetail,
} from "../shared/sidebar-project-jump";
import type { SidebarAgentButton } from "../shared/sidebar-agents";
import {
  readRenderedSidebarSessionSlotIds,
  readRenderedSidebarSessionSlots,
  resolveAdjacentRenderedSidebarSessionSlotId,
  resolveRenderedSidebarSessionAdditiveSelection,
  resolveRenderedSidebarSessionRangeSelection,
  resolveVisibleSidebarSessionSlotId,
} from "./sidebar-visible-session-slots";
import {
  PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT,
  readPrimaryAgentLauncherId,
  writePrimaryAgentLauncherId,
  type PrimaryAgentLauncherChangedEvent,
} from "./primary-agent-launcher";
import { type ProjectSessionListCollapsedState } from "./project-session-list-toggle";
import { hasKnownSidebarProjectInventory } from "./sidebar-project-empty-state";
import {
  createLocalProjectCollectionCollapseKey,
  createRemoteProjectCollectionCollapseKey,
  DEFAULT_SIDEBAR_WINDOW_SCOPE_ID,
  normalizeSidebarWindowScopeId,
  readSidebarKeepAwakeRuntime,
  readSidebarUiCollapseState,
  summarizeSidebarUiCollapseRead,
  summarizeSidebarUiCollapseState,
  writeSidebarUiCollapseState,
} from "./sidebar-app/collapse-state";
import {
  areSameGroupDropTarget,
  areSameRemoteMachineDropTarget,
  areSameSessionDropTarget,
  createPinnedSessionDomDebugState,
  createPinnedSessionDropResolutionDebugState,
  createPinnedSessionDropTargetLogKey,
  createPinnedSessionReorderDebugState,
  createProjectCollectionIdByProjectId,
  createProjectGroupOrderItems,
  createRemoteProjectListScopeId,
  createSessionPointerDragState,
  findCreatedGroupId,
  getDragNativeEvent,
  getProjectCollectionDragMetrics,
  getProjectCollectionFamilyProjectIds,
  getProjectGroupDragHeaderMetrics,
  getRemoteMachineDragHeaderMetrics,
  getRemoteProjectCollectionFamilyProjectIds,
  LOCAL_PROJECT_LIST_SCOPE_ID,
  moveCollectionIdToDropTarget,
  moveGroupIdsByProjectDropTarget,
  movePinnedSessionIdsByDropTarget,
  moveProjectGroupFamilyToEnd,
  moveRemoteMachineIdToDropTarget,
  resolveGroupDropTargetFromPoint,
  resolvePinnedSessionDropTargetFromPoint,
  resolveProjectCollectionDropTargetFromPoint,
  resolveProjectUngroupDropScopeFromPoint,
  resolveRemoteMachineDropTargetFromPoint,
  resolveSessionDropTargetFromPoint,
  SIDEBAR_V2_LOCAL_GROUP_ORDER_KEY,
  summarizePointerEventForPinnedReorder,
  summarizeSidebarWakeScrollGeometry,
  summarizeSidebarWakeScrollOrderState,
  summarizeSidebarWakeScrollRenderedSlots,
  updateGroupDragPreviewFromEvent,
  updateSessionPointerDragState,
  type SidebarPointerDownSessionTarget,
  type SidebarProjectCollectionDropTarget,
  type SidebarRemoteMachineDropTarget,
  type SidebarSessionPointerDragState,
} from "./sidebar-app/drag-drop-geometry";
import {
  ProjectCollectionDragGhost,
  ProjectGroupDragGhost,
  ProjectListEndUngroupDropZone,
  RemoteMachineDragGhost,
  type SidebarGroupDragPreview,
  type SidebarProjectCollectionDragPreview,
  type SidebarRemoteMachineDragPreview,
} from "./sidebar-app/drag-ghosts";
import { SidebarHotkeyOverlay, useCommandHotkeyOverlay } from "./sidebar-app/hotkey-overlay";
import { SidebarReferenceFooter, SidebarReferenceSectionHeader, SidebarReferenceTopChrome, formatSidebarMenuHotkeyLabel } from "./sidebar-app/reference-chrome";
import { RemoteMachineSidebarSection } from "./sidebar-app/remote-machine-section";
import {
  countSidebarSessions,
  createDisplayedGroupIds,
  createDisplayedSessionIdsByGroup,
  createPinnedFirstSessionOrder,
  createWorkspaceSessionIdsByGroup,
  findSessionGroupId,
  getCommandPaletteHotkeyActionId,
  getSidebarSectionSessionSummary,
  getSidebarSessionSearchNavigationDirection,
  getSidebarStartupElapsedMs,
  getSidebarStartupNow,
  hasActiveSidebarHotkeyRecorder,
  haveSameSessionOrder,
  haveSameSessionSet,
  isEditableSidebarKeyboardTarget,
  isSidebarSessionSearchNavigationKey,
  postSidebarAgentIconBoundaryLog,
  summarizeSidebarAgentIconsFromGroups,
  summarizeSidebarAgentIconsFromStore,
} from "./sidebar-app/session-ordering";
import type {
  ReferenceSidebarSectionId,
  RemoteMachineRuntimeStatus,
  RemoteMachineRuntimeStatuses,
  RemoteMachineStatusMessages,
  SessionIdsByGroup,
  SidebarProjectCollectionRenderItem,
  SidebarSectionSessionSummary,
} from "./sidebar-app/types";

type SidebarEventSource = Pick<Window, "addEventListener" | "removeEventListener">;

export type SidebarAppProps = {
  enableProjectCollections?: boolean;
  messageSource?: SidebarEventSource;
  nativeHostEventSource?: SidebarEventSource | null;
  onStartGxserver?: () => void;
  vscode: WebviewApi;
  windowScopeId?: string;
};




const GHOSTEX_DISCORD_URL = "https://discord.gg/df7b3G92CS";












/**
 * CDXC:SidebarBrowserTabReveal 2026-08-18:
 * `requestId` is what makes a reveal one-shot: two consecutive middle-clicks on
 * the same link name the same session and must each scroll it back into view.
 */
type SidebarSessionRevealRequest = {
  requestId: number;
  sessionId: string;
};




const REFERENCE_SECTION_CHILD_ANIMATION_RESET_MS = 420;

const sensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
  KeyboardSensor,
];


const SIDEBAR_STARTUP_INTERACTION_BLOCK_MS = 1500;
const SIDEBAR_STARTUP_REPRO_WINDOW_MS = 15_000;
const SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID = "gxserver-unavailable";
const SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS = 20_000;
const MIN_SESSION_SEARCH_QUERY_LENGTH = 4;
const COMPLETION_FLASH_DURATION_MS = 3_000;
const DEBUG_BUILD_STAMP_STYLE: CSSProperties = {
  position: "fixed",
  right: "10px",
  bottom: "8px",
  zIndex: 20,
  padding: 0,
  border: "none",
  background: "transparent",
  color: "var(--vscode-foreground)",
  fontFamily: "var(--vscode-font-family)",
  fontSize: "10px",
  lineHeight: 1.2,
  fontVariantNumeric: "tabular-nums",
  opacity: 0.72,
};


function readSidebarProjectJumpEventDetail(event: Event): SidebarProjectJumpEventDetail | undefined {
  const detail = (event as CustomEvent<unknown>).detail;
  if (!detail || typeof detail !== "object") {
    return undefined;
  }
  const candidate = detail as Partial<SidebarProjectJumpEventDetail>;
  if (
    typeof candidate.groupId !== "string" ||
    typeof candidate.projectId !== "string" ||
    typeof candidate.expandCollapsedProject !== "boolean" ||
    typeof candidate.showLessAfterExpand !== "boolean" ||
    (
      candidate.revealFocusedSession !== undefined &&
      typeof candidate.revealFocusedSession !== "boolean"
    )
  ) {
    return undefined;
  }
  return {
    expandCollapsedProject: candidate.expandCollapsedProject,
    groupId: candidate.groupId,
    projectId: candidate.projectId,
    revealFocusedSession: candidate.revealFocusedSession === true ? true : undefined,
    showLessAfterExpand: candidate.showLessAfterExpand,
  };
}

export function SidebarApp({
  enableProjectCollections = false,
  messageSource = window,
  nativeHostEventSource = window,
  onStartGxserver,
  vscode,
  windowScopeId: rawWindowScopeId = DEFAULT_SIDEBAR_WINDOW_SCOPE_ID,
}: SidebarAppProps) {
  useDismissSidebarTooltipsOnScroll();
  const [ windowScopeId ] = useState(() => normalizeSidebarWindowScopeId(rawWindowScopeId));
  const [ initialUiCollapseStateRead ] = useState(() => readSidebarUiCollapseState(windowScopeId));
  const initialUiCollapseState = initialUiCollapseStateRead.state;
  const [ isStartupInteractionBlocked, setIsStartupInteractionBlocked ] = useState(true);
  const [ autoEditingGroupId, setAutoEditingGroupId ] = useState<string>();
  const [ agentCreateRequestId, setAgentCreateRequestId ] = useState(0);
  const [ isDaemonSessionsOpen, setIsDaemonSessionsOpen ] = useState(false);
  const [ isPinnedPromptsOpen, setIsPinnedPromptsOpen ] = useState(false);
  const [ isPreviousSessionsOpen, setIsPreviousSessionsOpen ] = useState(false);
  const [ isReferenceChatsCollapsed, setIsReferenceChatsCollapsed ] = useState(
    initialUiCollapseState.isReferenceChatsCollapsed,
  );
  const [ isReferenceProjectsCollapsed, setIsReferenceProjectsCollapsed ] = useState(
    initialUiCollapseState.isReferenceProjectsCollapsed,
  );
  const [ isScratchPadOpen, setIsScratchPadOpen ] = useState(false);
  const [ isSettingsOpen, setIsSettingsOpen ] = useState(false);
  const [ isSessionSearchOpen, setIsSessionSearchOpen ] = useState(false);
  const [ initialHiddenItems ] = useState(readSidebarHiddenItems);
  const [ hiddenGroupIds, setHiddenGroupIds ] = useState(initialHiddenItems.groupIds);
  const [ hiddenCollectionKeys, setHiddenCollectionKeys ] = useState(
    initialHiddenItems.collectionKeys,
  );
  const [ showHiddenSidebarItems, setShowHiddenSidebarItems ] = useState(false);
  const showCommandHotkeyOverlay = useCommandHotkeyOverlay();
  const [ completionFlashNonceBySessionId, setCompletionFlashNonceBySessionId ] = useState<
    Record<string, number>
  >({});
  const [ collapsedGroupsById, setCollapsedGroupsById ] = useState<Record<string, true>>(
    initialUiCollapseState.collapsedGroupsById,
  );
  const [ collapsedProjectCollectionsByKey, setCollapsedProjectCollectionsByKey ] = useState<
    Record<string, true>
  >(initialUiCollapseState.collapsedProjectCollectionsByKey);
  const [ collapsedProjectSessionListsById, setCollapsedProjectSessionListsById ] =
    useState<ProjectSessionListCollapsedState>(
      initialUiCollapseState.collapsedProjectSessionListsById,
    );
  const [ projectCollections, setProjectCollections ] = useState<SidebarProjectCollectionsState>(
    enableProjectCollections ? readSidebarProjectCollections : { collections: [], nextCollectionNumber: 1 },
  );
  const [ remoteProjectCollectionsByMachineId, setRemoteProjectCollectionsByMachineId ] = useState<
    Record<string, SidebarProjectCollectionsState>
  >({});
  /*
  CDXC:SidebarProjectCollections 2026-07-18-00:00:
  Tracks the last collection state exchanged with gxserver (pushed to it or
  adopted from it) so the write-through effect posts only real local edits.
  Without this baseline, mount and server reconciliation would echo the state
  straight back and a fresh install would clobber the server copy with its
  empty localStorage overlay.
  */
  const lastGxserverSyncedProjectCollectionsRef = useRef(projectCollections);
  const [ autoEditingProjectCollectionId, setAutoEditingProjectCollectionId ] = useState<string>();
  const [ collapsedRemoteMachineSectionsById, setCollapsedRemoteMachineSectionsById ] = useState<
    Record<string, true>
  >(initialUiCollapseState.collapsedRemoteMachineSectionsById);
  const [ referenceSectionChildAnimations, setReferenceSectionChildAnimations ] = useState<
    Record<ReferenceSidebarSectionId, boolean>
  >({
    projects: false,
    quick: false,
    remote: false,
  });
  const previousExpandedReferenceProjectGroupIdsRef = useRef<string[]>([]);
  const previousExpandedRemoteProjectGroupIdsByMachineIdRef = useRef<
    Record<string, string[]>
  >({});
  const previousExpandedProjectGroupIdsByCollectionIdRef = useRef<Record<string, string[]>>({});
  const [ sessionSearchQuery, setSessionSearchQuery ] = useState("");
  const [ selectedSessionTagFilters, setSelectedSessionTagFilters ] = useState<
    SidebarSessionTagFilter[]
  >([]);
  const [ remoteSessionSearchPreviousSessions, setRemoteSessionSearchPreviousSessions ] =
    useState<SidebarPreviousSessionItem[] | undefined>(undefined);
  const [ groupDropIndicator, setGroupDropIndicator ] = useState<SidebarGroupDropTarget>();
  const [ projectCollectionDropIndicator, setProjectCollectionDropIndicator ] =
    useState<SidebarProjectCollectionDropTarget>();
  const [ remoteMachineDropIndicator, setRemoteMachineDropIndicator ] =
    useState<SidebarRemoteMachineDropTarget>();
  const [ projectUngroupDropIndicatorScopeId, setProjectUngroupDropIndicatorScopeId ] =
    useState<string>();
  const [ groupDragPreview, setGroupDragPreview ] = useState<SidebarGroupDragPreview>();
  const [ projectCollectionDragPreview, setProjectCollectionDragPreview ] =
    useState<SidebarProjectCollectionDragPreview>();
  const [ remoteMachineDragPreview, setRemoteMachineDragPreview ] =
    useState<SidebarRemoteMachineDragPreview>();
  /*
   * CDXC:ProjectReorderScrollLock 2026-07-22:
   * While a project or collection header is being dragged, the per-project
   * session scrollers must not auto-scroll under the ghost. dnd-kit's Scroller
   * treats any computed overflow auto/scroll ancestor under the pointer as a
   * scroll target, so this flag flips those inner scrollers to overflow hidden
   * for the duration of the drag (the main sidebar scroller stays scrollable
   * to reach offscreen drop positions).
   */
  const [ isProjectReorderDragActive, setIsProjectReorderDragActive ] = useState(false);
  const [ referenceLayoutElement, setReferenceLayoutElement ] = useState<HTMLDivElement | null>(
    null,
  );
  const [ pinnedSessionDropIndicator, setPinnedSessionDropIndicator ] =
    useState<SidebarSessionDropTarget>();
  const [ sessionDropIndicator, setSessionDropIndicator ] = useState<SidebarSessionDropTarget>();
  const [ isSessionSearchSelectionVisible, setIsSessionSearchSelectionVisible ] = useState(false);
  const [ focusedSessionRevealRequestId, setFocusedSessionRevealRequestId ] = useState(0);
  /**
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * The host's pending "make this row visible" request. It is kept in state
   * rather than handled inline because the row it names can arrive after the
   * request: gpui creates a Browser tab and asks for the reveal in the same
   * turn the tab is first published, so the reveal effect re-runs with the
   * displayed session list until the row exists.
   */
  const [ sessionRevealRequest, setSessionRevealRequest ] =
    useState<SidebarSessionRevealRequest>();
  const [ showGxserverUnavailableEmptyState, setShowGxserverUnavailableEmptyState ] =
    useState(false);
  const [ selectedSessionSearchResult, setSelectedSessionSearchResult ] =
    useState<SidebarSessionSearchSelection>();
  const [ selectedSidebarSessionIds, setSelectedSidebarSessionIds ] = useState<string[]>([]);
  const pendingCreateGroupRef = useRef(false);
  const didResetStoreRef = useRef(false);
  const sessionGroupsPanelRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const groupIdsRef = useRef<string[]>([]);
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * The grouped V2 rows AS RENDERED, reported up by SidebarV2Root. A project drag
   * resolves its drop from the pointer against the rows on screen, so the
   * candidate list has to be those exact rows: re-deriving the logical grouping
   * here would be a second copy of `groupSidebarV2ProjectsByLogicalKey`'s merge
   * rules, free to drift from the list the user is actually looking at.
   *
   * It is a SEPARATE ref rather than a V2 mode on `groupIdsRef`: that ref is
   * V1's, several V1 paths read it during a drag, and swapping its contents under
   * them is a bigger change than the reorder feature needs.
   */
  const sidebarV2GroupOrderRowsRef = useRef<readonly SidebarV2GroupOrderRow[]>([]);
  const sessionIdsByGroupRef = useRef<SessionIdsByGroup>({});
  const pinnedSessionDropTargetLogKeyRef = useRef<string | undefined>(undefined);
  const previousSessionCountsByGroupRef = useRef<Record<string, number>>({});
  const latestSessionSearchPreviousRequestIdRef = useRef<string | undefined>(undefined);
  const didApplyStartupEmptyChatsCollapseRef = useRef(false);
  const hasEstablishedStartupGroupCollapseBaselineRef = useRef(false);
  const hasObservedAvailableGxserverStateRef = useRef(false);
  const previousNormalizedSessionSearchQueryRef = useRef("");
  const refreshDebugInstanceIdRef = useRef(createSidebarRefreshDebugInstanceId());
  const pointerDownSessionTargetRef = useRef<SidebarPointerDownSessionTarget | undefined>(
    undefined,
  );
  const sessionPointerDragStateRef = useRef<SidebarSessionPointerDragState | undefined>(undefined);
  const completionFlashTimeoutBySessionIdRef = useRef<Map<string, number>>(new Map());
  const referenceSectionAnimationTimeoutsRef = useRef<
    Partial<Record<ReferenceSidebarSectionId, number>>
  >({});
  const sessionGroupsContentRef = useRef<HTMLDivElement>(null);
  const sidebarStartupStartedAtRef = useRef(getSidebarStartupNow());
  const hasAppliedHydrateRef = useRef(false);
  const firstHydrateRevisionRef = useRef<number | undefined>(undefined);
  const lastSidebarStartupRenderStateKeyRef = useRef<string | undefined>(undefined);
  const didLogRefreshInstanceObservedRef = useRef(false);
  const didLogInitialUiCollapseStateReadRef = useRef(false);
  const collapseStateHydrateLogCountRef = useRef(0);
  const lastCollapseStateHydrateShapeRef = useRef<string | undefined>(undefined);
  const focusedSessionScrollLogSequenceRef = useRef(0);
  const previousFocusedSessionRevealRequestIdRef = useRef(focusedSessionRevealRequestId);
  const handledSessionRevealRequestIdRef = useRef<number | undefined>(undefined);
  const pendingSessionRevealScrollRequestIdRef = useRef<number | undefined>(undefined);

  if (!didResetStoreRef.current) {
    resetSidebarStore();
    didResetStoreRef.current = true;
  }

  useEffect(() => {
    return () => {
      setSidebarTooltipsSuppressedForDrag(false);
    };
  }, []);

  useEffect(() => {
    writeSidebarHiddenItems({ collectionKeys: hiddenCollectionKeys, groupIds: hiddenGroupIds });
  }, [hiddenCollectionKeys, hiddenGroupIds]);

  useEffect(() => {
    if (!enableProjectCollections) {
      return;
    }
    writeSidebarProjectCollections(projectCollections);
    /*
    CDXC:SidebarProjectCollections 2026-07-18-00:00:
    localStorage stays the instant-edit overlay, but every local collection
    edit also write-through-syncs the whole wire state to gxserver via the
    host so React Native Android sees the same colored "Group N" overlay. States that
    just arrived from (or were already pushed to) the server are skipped to
    avoid echo loops.
    */
    if (
      areSidebarProjectCollectionsStatesEqual(
        lastGxserverSyncedProjectCollectionsRef.current,
        projectCollections,
      )
    ) {
      return;
    }
    lastGxserverSyncedProjectCollectionsRef.current = projectCollections;
    vscode.postMessage({
      state: serializeSidebarProjectCollectionsForGxserver(projectCollections),
      type: "updateSidebarProjectCollections",
    });
  }, [enableProjectCollections, projectCollections, vscode]);

  const applyLocalFocus = useSidebarStore((state) => state.applyLocalFocus);
  const consumeFocusedSessionScrollSuppression = useSidebarStore(
    (state) => state.consumeFocusedSessionScrollSuppression,
  );
  const applyCommandRunStateClearedMessage = useSidebarStore(
    (state) => state.applyCommandRunStateClearedMessage,
  );
  const applyCommandRunStateMessage = useSidebarStore((state) => state.applyCommandRunStateMessage);
  const applyGroupsChangedMessage = useSidebarStore((state) => state.applyGroupsChangedMessage);
  const applyHudChangedMessage = useSidebarStore((state) => state.applyHudChangedMessage);
  const applyOrderSyncResultMessage = useSidebarStore((state) => state.applyOrderSyncResultMessage);
  const applySessionPresentationMessage = useSidebarStore(
    (state) => state.applySessionPresentationMessage,
  );
  const applySidebarMessage = useSidebarStore((state) => state.applySidebarMessage);
  const setDaemonSessionsState = useSidebarStore((state) => state.setDaemonSessionsState);
  const setGitCommitDraft = useSidebarStore((state) => state.setGitCommitDraft);
  const setGitFileDiffDraft = useSidebarStore((state) => state.setGitFileDiffDraft);
  const {
    activeSessionsSortMode,
    agentManagerZoomPercent,
    agents,
    createSessionOnSidebarDoubleClick,
    customThemeColor,
    debuggingMode,
    groupOrder,
    groupsById,
    previousSessions,
    projectSettingsProjects,
    recentProjects,
    settings,
    revision,
    sessionsById,
    sidebarAutoSettleAfterDays,
    sidebarAutoSettleAfterDaysByMachineId,
    sidebarLifecycleCapabilities,
    sidebarLifecycleCapabilitiesByMachineId,
    theme,
    workspaceGroupIds,
  } = useSidebarStore(
    useShallow((state) => ({
      activeSessionsSortMode: state.hud.activeSessionsSortMode,
      agentManagerZoomPercent: state.hud.agentManagerZoomPercent,
      agents: state.hud.agents,
      createSessionOnSidebarDoubleClick: state.hud.createSessionOnSidebarDoubleClick,
      customThemeColor: state.hud.customThemeColor,
      debuggingMode: state.hud.debuggingMode,
      groupOrder: state.groupOrder,
      groupsById: state.groupsById,
      /*
       * CDXC:SidebarV2LogicalProjects 2026-07-29:
       * The auto-settle window is machine-scoped for the same reason capability
       * is, so it travels on the HUD next to it rather than being read off the
       * local settings for every row.
       */
      sidebarAutoSettleAfterDays: state.hud.autoSettleAfterDays,
      sidebarAutoSettleAfterDaysByMachineId: state.hud.autoSettleAfterDaysByMachineId,
      sidebarLifecycleCapabilities: state.hud.lifecycleCapabilities,
      sidebarLifecycleCapabilitiesByMachineId: state.hud.lifecycleCapabilitiesByMachineId,
      previousSessions: state.previousSessions,
      projectSettingsProjects: state.hud.projectSettingsProjects,
      recentProjects: state.hud.recentProjects,
      revision: state.revision,
      settings: state.hud.settings,
      sessionsById: state.sessionsById,
      theme: state.hud.theme,
      workspaceGroupIds: state.workspaceGroupIds,
    })),
  );
  const gitCommitDraft = useSidebarStore((state) => state.gitCommitDraft);
  const gitFileDiffDraft = useSidebarStore((state) => state.gitFileDiffDraft);
  const authoritativeSessionIdsByGroup = useSidebarStore((state) => state.sessionIdsByGroup);
  const [ remoteMachineRuntimeStatuses, setRemoteMachineRuntimeStatuses ] =
    useState<RemoteMachineRuntimeStatuses>({});
  const [ remoteMachineStatusMessages, setRemoteMachineStatusMessages ] =
    useState<RemoteMachineStatusMessages>({});
  const [ primaryAgentLauncherId, setPrimaryAgentLauncherId ] = useState(readPrimaryAgentLauncherId);
  const [ sidebarKeepAwakeRuntime, setSidebarKeepAwakeRuntime ] = useState(
    readSidebarKeepAwakeRuntime,
  );
  const buildStamp = useSidebarStore((state) =>
    state.hud.debuggingMode ? state.hud.buildStamp : undefined,
  );
  const hasGxserverUnavailablePlaceholder = Boolean(
    groupsById[ SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID ],
  );
  const hasAvailableGxserverState =
    !hasGxserverUnavailablePlaceholder && groupOrder.length > 0;

  useEffect(() => {
    if (hasAvailableGxserverState) {
      hasObservedAvailableGxserverStateRef.current = true;
    }
  }, [ hasAvailableGxserverState ]);

  useEffect(() => {
    if (!hasGxserverUnavailablePlaceholder) {
      setShowGxserverUnavailableEmptyState(false);
      return;
    }

    /*
     * CDXC:GPUIGxserverLiveDisconnect 2026-07-14:
     * The 20-second grace period below is only for cold startup while gxserver
     * may still recover. GPUI supplies the explicit start action, so once this
     * mounted sidebar has already rendered an available daemon state, a later
     * unavailable hydrate is a live disconnect and must expose the recovery
     * message and button immediately instead of leaving Projects blank.
     */
    if (onStartGxserver && hasObservedAvailableGxserverStateRef.current) {
      setShowGxserverUnavailableEmptyState(true);
      return;
    }

    /*
     * CDXC:GxserverPresentation 2026-06-16-09:35:
     * When gxserver is off or missing during startup, the sidebar must not show
     * the raw synthetic status project row. Keep the Projects body blank while
     * startup can still recover, then after 20 seconds show the two-line restart
     * guidance using the exact reference-sidebar empty-state typography shared
     * with "No projects."
     */
    setShowGxserverUnavailableEmptyState(false);
    const timeoutId = window.setTimeout(() => {
      setShowGxserverUnavailableEmptyState(true);
    }, SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [ hasGxserverUnavailablePlaceholder, onStartGxserver ]);

  const effectiveSettings = settings ?? DEFAULT_ghostex_SETTINGS;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The sidebar version rides the shared settings pipeline, so the sidebar
   * reads it from the same hydrated snapshot every other setting comes from.
   * A host that has not sent settings yet keeps the classic sidebar.
   */
  const sidebarVersion: SidebarVersion = effectiveSettings.sidebarVersion;
  const sidebarV2Layout: SidebarV2Layout = effectiveSettings.sidebarV2Layout;
  const isSidebarV2Active = sidebarVersion === "v2";
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Grouped V2 is the ONE V2 layout that renders project rows, so it is the only
   * one that owns project reorder. The flat inbox has no project rows to drag,
   * and letting a group drag resolve against it would resolve against nothing.
   */
  const isSidebarV2GroupedActive = isSidebarV2Active && sidebarV2Layout === "byProject";
  const showSidebarKeepAwakeButton =
    effectiveSettings.showBetaFeatures && !effectiveSettings.hideKeepAwakeTitlebarControl;
  const sidebarRefreshDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
    effectiveSettings.diagnosticLogging,
    "native.sidebar.refresh",
  );
  const sidebarCollapseDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
    effectiveSettings.diagnosticLogging,
    "native.sidebar.collapse",
  );

  useEffect(() => {
    const refreshKeepAwakeRuntime = () => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime());
    };
    window.addEventListener("focus", refreshKeepAwakeRuntime);
    window.addEventListener("storage", refreshKeepAwakeRuntime);
    document.addEventListener("visibilitychange", refreshKeepAwakeRuntime);
    return () => {
      window.removeEventListener("focus", refreshKeepAwakeRuntime);
      window.removeEventListener("storage", refreshKeepAwakeRuntime);
      document.removeEventListener("visibilitychange", refreshKeepAwakeRuntime);
    };
  }, []);

  const sidebarSessionTagListItems = useMemo(
    () => normalizeSidebarSessionTagListItems(effectiveSettings.sidebarSessionTagListItems),
    [ effectiveSettings.sidebarSessionTagListItems ],
  );
  const enabledVisibleSidebarSessionTagSet = useMemo(
    () => new Set(getEnabledVisibleSidebarSessionTagFilters(sidebarSessionTagListItems)),
    [ sidebarSessionTagListItems ],
  );
  const activeSelectedSessionTagFilters = useMemo(
    () =>
      selectedSessionTagFilters.filter((tag) => enabledVisibleSidebarSessionTagSet.has(tag)),
    [ enabledVisibleSidebarSessionTagSet, selectedSessionTagFilters ],
  );

  useEffect(() => {
    /*
     * CDXC:SessionTagFilters 2026-06-13-17:50:
     * If a selected sidebar tag filter becomes hidden or disabled from
     * Settings, drop it from the active filter state so sessions are not
     * invisibly filtered by a tag the sidebar menu no longer lets users choose.
     */
    setSelectedSessionTagFilters((current) => {
      const next = current.filter((tag) => enabledVisibleSidebarSessionTagSet.has(tag));
      return next.length === current.length ? current : next;
    });
  }, [ enabledVisibleSidebarSessionTagSet ]);

  useEffect(() => {
    const refreshPrimaryAgentLauncher = (event: Event) => {
      const changedEvent = event as PrimaryAgentLauncherChangedEvent;
      setPrimaryAgentLauncherId(
        typeof changedEvent.detail?.agentId === "string"
          ? changedEvent.detail.agentId
          : readPrimaryAgentLauncherId(),
      );
    };

    window.addEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    return () => {
      window.removeEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    };
  }, []);

  const postSidebarDebugLog = useEffectEvent((
    scenarioId: DiagnosticLoggingScenarioId,
    event: string,
    details: unknown,
  ) => {
    if (!debuggingMode) {
      return;
    }

    logSidebarDebug(debuggingMode, event, details);
    vscode.postMessage({
      details,
      event,
      scenarioId,
      type: "sidebarDebugLog",
    });
  });

  const postSidebarCollapseStateLog = useEffectEvent(
    (
      event: string,
      details: Record<string, unknown>,
      options: { enabled?: boolean; } = {},
    ) => {
      /*
       * CDXC:SidebarCollapseDiagnostics 2026-06-02-23:52:
       * Sidebar restart repros need a dedicated low-volume trace for localStorage
       * collapse-state reads, writes, hydrate timing, and user toggles. Keep the
       * payload privacy-safe by recording counts, booleans, revisions, elapsed
       * timings, and hashed group identifiers instead of project names or paths.
       */
      if (!(options.enabled ?? sidebarCollapseDiagnosticLoggingEnabled)) {
        return;
      }

      vscode.postMessage({
        details: {
          ...details,
          elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
          firstHydrateRevision: firstHydrateRevisionRef.current,
          hasEstablishedStartupGroupCollapseBaseline:
            hasEstablishedStartupGroupCollapseBaselineRef.current,
          hasHydrate: hasAppliedHydrateRef.current,
          instanceId: refreshDebugInstanceIdRef.current,
          revision,
        },
        event: `${SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX}${event}`,
        scenarioId: "native.sidebar.collapse",
        type: "sidebarDebugLog",
      });
    },
  );

  const postPinnedSessionReorderLog = useEffectEvent((event: string, details: unknown) => {
    /*
     * CDXC:PinnedSessions 2026-05-28-15:33:
     * Pinned reorder failures need click-scoped repro breadcrumbs even when
     * broad Debugging Mode is off. Keep these events low-volume and explicit
     * so a user drag can reveal which guard prevented syncSessionOrder.
     */
    vscode.postMessage({
      details,
      event: `repro.pinnedSessionReorder.${event}`,
      scenarioId: "native.pane.reorder",
      type: "sidebarDebugLog",
    });
  });

  const postSidebarStartupReproLog = useEffectEvent((event: string, details: unknown) => {
    if (
      getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current) >
      SIDEBAR_STARTUP_REPRO_WINDOW_MS
    ) {
      return;
    }

    vscode.postMessage({
      details,
      event: `repro.sidebarStartup.${event}`,
      scenarioId: "native.sidebar.refresh",
      type: "sidebarDebugLog",
    });
  });

  const postSidebarRefreshLifecycleLog = useEffectEvent(
    (event: string, details: Record<string, unknown>) => {
      const currentSettings = useSidebarStore.getState().hud.settings ?? DEFAULT_ghostex_SETTINGS;
      postSidebarRefreshDebugLog(
        isDiagnosticLoggingScenarioEnabled(
          currentSettings.diagnosticLogging,
          "native.sidebar.refresh",
        ),
        vscode,
        event,
        details,
      );
    },
  );

  useLayoutEffect(() => {
    if (!hasAppliedHydrateRef.current) {
      return;
    }

    const autoCollapseGroupIds = getAutoCollapseGroupIds({
      groupsById,
      workspaceGroupIds,
    });
    const nextSessionCountsByGroup = getSessionCountsByGroup({
      groupIds: groupOrder,
      sessionIdsByGroup: authoritativeSessionIdsByGroup,
    });
    const isEstablishingStartupGroupCollapseBaseline =
      !hasEstablishedStartupGroupCollapseBaselineRef.current;
    const hasGxserverUnavailablePlaceholder = groupOrder.includes(
      SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID,
    );
    const visibleGroupIds = new Set(groupOrder);
    const unknownCollapsedGroupCount = Object.keys(collapsedGroupsById).filter(
      (groupId) => !visibleGroupIds.has(groupId),
    ).length;
    const preserveUnknownCollapsedGroups =
      isEstablishingStartupGroupCollapseBaseline && hasGxserverUnavailablePlaceholder;
    const sessionCountIncreaseGroupIds = isEstablishingStartupGroupCollapseBaseline
      ? []
      : groupOrder.filter((groupId) => {
        const previousCount = previousSessionCountsByGroupRef.current[ groupId ];
        return (
          previousCount !== undefined &&
          (authoritativeSessionIdsByGroup[ groupId ] ?? []).length > previousCount
        );
      });

    if (preserveUnknownCollapsedGroups && unknownCollapsedGroupCount > 0) {
      postSidebarCollapseStateLog("startupPartialHydratePreserved", {
        groupCount: groupOrder.length,
        placeholderGroupPresent: true,
        unknownCollapsedGroupCount,
      });
    }

    setCollapsedGroupsById((previous) =>
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds,
        expandOnSessionCountIncreaseGroupIds: groupOrder,
        groupIds: groupOrder,
        preserveUnknownCollapsedGroups,
        previousSessionCountsByGroup: previousSessionCountsByGroupRef.current,
        previousCollapsedGroupsById: previous,
        sessionIdsByGroup: authoritativeSessionIdsByGroup,
        skipExpandOnSessionCountIncrease: isEstablishingStartupGroupCollapseBaseline,
      }),
    );

    /**
     * CDXC:SidebarReference 2026-05-08-11:09
     * When creating a chat, terminal, browser pane, or agent session inside a
     * collapsed Combined sidebar area, expand the owning Chats/Projects section
     * as soon as the host hydrates the added session so the user sees the
     * result of the action.
     * CDXC:SidebarReference 2026-05-20-12:00
     * Do not expand Chats/Projects section headers on the first post-hydrate
     * baseline pass after restart. Restored session counts are not new sessions.
     */
    if (sessionCountIncreaseGroupIds.some((groupId) => groupsById[ groupId ]?.isChatCollection)) {
      postSidebarCollapseStateLog("sectionAutoExpanded", {
        reason: "session-count-increase",
        section: "quick",
        sessionCountIncreaseGroupCount: sessionCountIncreaseGroupIds.length,
      });
      setIsReferenceChatsCollapsed(false);
    }

    if (sessionCountIncreaseGroupIds.some((groupId) => !groupsById[ groupId ]?.isChatCollection)) {
      postSidebarCollapseStateLog("sectionAutoExpanded", {
        reason: "session-count-increase",
        section: "projects",
        sessionCountIncreaseGroupCount: sessionCountIncreaseGroupIds.length,
      });
      setIsReferenceProjectsCollapsed(false);
    }

    previousSessionCountsByGroupRef.current = nextSessionCountsByGroup;
    if (isEstablishingStartupGroupCollapseBaseline && !hasGxserverUnavailablePlaceholder) {
      postSidebarCollapseStateLog("startupBaselineEstablished", {
        groupCount: groupOrder.length,
        sessionCount: Object.keys(sessionsById).length,
      });
      hasEstablishedStartupGroupCollapseBaselineRef.current = true;
    }
  }, [
    authoritativeSessionIdsByGroup,
    collapsedGroupsById,
    groupOrder,
    groupsById,
    sessionsById,
    workspaceGroupIds,
  ]);

  const isSidebarInteractionBlocked = isStartupInteractionBlocked;

  const setGroupCollapsed = (groupId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedGroupsById[ groupId ] === true;
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    postSidebarCollapseStateLog("groupToggle", {
      changed: wasCollapsed !== collapsed,
      collapsed,
      collapsedGroupCountBefore,
      collapsedGroupCountExpectedAfter:
        collapsedGroupCountBefore + (wasCollapsed === collapsed ? 0 : collapsed ? 1 : -1),
      groupHash: hashSidebarCollapseDebugId(groupId),
      groupIndex: groupOrder.indexOf(groupId),
      wasCollapsed,
    });
    setCollapsedGroupsById((previous) => {
      if (collapsed) {
        if (previous[ groupId ]) {
          return previous;
        }

        return {
          ...previous,
          [ groupId ]: true,
        };
      }

      if (!previous[ groupId ]) {
        return previous;
      }

      const next = { ...previous };
      delete next[ groupId ];
      return next;
    });
  };

  const setGroupsCollapsed = (groupIds: readonly string[], collapsed: boolean) => {
    const targetGroupSet = new Set(groupIds);
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    const changedGroupCount = groupIds.filter(
      (groupId) => collapsedGroupsById[ groupId ] !== (collapsed ? true : undefined),
    ).length;
    postSidebarCollapseStateLog("groupsBulkToggle", {
      changedGroupCount,
      collapsed,
      collapsedGroupCountBefore,
      collapsedGroupCountExpectedAfter:
        collapsedGroupCountBefore + (collapsed ? changedGroupCount : -changedGroupCount),
      groupHashes: summarizeSidebarCollapseDebugGroupIds(groupIds),
      targetGroupCount: targetGroupSet.size,
    });
    setCollapsedGroupsById((previous) => {
      if (collapsed) {
        const next = { ...previous };
        let changed = false;
        for (const groupId of groupIds) {
          if (!next[ groupId ]) {
            next[ groupId ] = true;
            changed = true;
          }
        }
        return changed ? next : previous;
      }

      let next: Record<string, true> | undefined;
      for (const groupId of groupIds) {
        if (previous[ groupId ]) {
          next ??= { ...previous };
          delete next[ groupId ];
        }
      }
      return next ?? previous;
    });
  };

  const setProjectCollectionCollapsed = (collectionKey: string, collapsed: boolean) => {
    setCollapsedProjectCollectionsByKey((previous) => {
      if (collapsed) {
        return previous[ collectionKey ] ? previous : { ...previous, [ collectionKey ]: true };
      }
      if (!previous[ collectionKey ]) {
        return previous;
      }
      const next = { ...previous };
      delete next[ collectionKey ];
      return next;
    });
  };

  const setProjectSessionListCollapsed = (projectId: string, collapsed: boolean) => {
    setCollapsedProjectSessionListsById((previous) => {
      if (collapsed) {
        return previous[ projectId ] ? previous : { ...previous, [ projectId ]: true };
      }
      if (!previous[ projectId ]) {
        return previous;
      }
      const next = { ...previous };
      delete next[ projectId ];
      return next;
    });
  };

  const setRemoteMachineSectionCollapsed = (machineId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedRemoteMachineSectionsById[ machineId ] === true;
    postSidebarCollapseStateLog("remoteMachineSectionToggle", {
      changed: wasCollapsed !== collapsed,
      collapsed,
      machineHash: hashSidebarCollapseDebugId(machineId),
      wasCollapsed,
    });
    /*
     * CDXC:RemoteMachines 2026-06-09-19:02:
     * Remote machine sections are peers of Quick and Projects in the reference
     * sidebar. Persist their collapsed state by saved machine id so each machine
     * can collapse independently without affecting local project groups.
     */
    setCollapsedRemoteMachineSectionsById((previous) => {
      if (collapsed) {
        if (previous[ machineId ]) {
          return previous;
        }

        return {
          ...previous,
          [ machineId ]: true,
        };
      }

      if (!previous[ machineId ]) {
        return previous;
      }

      const next = { ...previous };
      delete next[ machineId ];
      return next;
    });
  };

  const dismissAppModalForSidebarNavigation = (area: string) => {
    /*
     * CDXC:SettingsDismissal 2026-06-15-14:07:
     * Settings is a workspace-scoped app modal, but sidebar navigation should
     * always return users to the live workspace. Dismiss the native app-modal
     * host before session focus, session creation, sidebar nav buttons,
     * top-level modals, and direct previous-session text search.
     */
    setIsSettingsOpen(false);
    if (!window.webkit?.messageHandlers?.ghostexAppModalHost) {
      return;
    }
    closeAppModal(area);
  };

  const focusSidebarSessionFromNavigation = (groupId: string, sessionId: string) => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:focusSession");
    useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    applyLocalFocus(groupId, sessionId);
  };

  const requestNewSession = () => {
    if (isSidebarInteractionBlocked) {
      return;
    }

    dismissAppModalForSidebarNavigation("SettingsDismissal:createSession");
    vscode.postMessage({ type: "createSession" });
  };

  const handleSidebarDoubleClick = (event: ReactMouseEvent<HTMLElement>) => {
    if (!createSessionOnSidebarDoubleClick) {
      return;
    }

    if (!isEmptySidebarDoubleClick(event)) {
      return;
    }

    event.preventDefault();
    requestNewSession();
  };

  const handleSidebarClickCapture = (event: ReactMouseEvent<HTMLElement>) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      return;
    }
    if (!target.closest(".session")) {
      return;
    }
    dismissAppModalForSidebarNavigation("SettingsDismissal:sessionClick");
  };

  const handleWindowMessage = useEffectEvent((event: MessageEvent<ExtensionToSidebarMessage>) => {
    if (!event.data) {
      return;
    }

    if (event.data.type === "hydrate" && enableProjectCollections) {
      const nextRemoteCollections: Record<string, SidebarProjectCollectionsState> = {};
      for (const [machineId, state] of Object.entries(
        event.data.remoteSidebarProjectCollectionsByMachineId ?? {},
      )) {
        const parsed = parseSidebarProjectCollectionsFromGxserver(state);
        if (parsed) {
          nextRemoteCollections[machineId] = parsed;
        }
      }
      setRemoteProjectCollectionsByMachineId(nextRemoteCollections);
    }

    if (event.data.type === "gpuiProjectSlotHotkey") {
      resolveGpuiProjectSlotHotkey(event.data.slotNumber);
      return;
    }

    if (event.data.type === "nativeHotkey") {
      runGhostexHotkeyAction(event.data.actionId);
      return;
    }

    if (event.data.type === "playCompletionSound") {
      const sessionId = event.data.sessionId;
      postSidebarDebugLog("native.agent.detection", "completionSound.messageReceived", {
        sound: event.data.sound,
        sessionId,
      });
      if (sessionId) {
        const existingTimeout = completionFlashTimeoutBySessionIdRef.current.get(sessionId);
        if (existingTimeout !== undefined) {
          window.clearTimeout(existingTimeout);
        }
        setCompletionFlashNonceBySessionId((previous) => ({
          ...previous,
          [ sessionId ]: (previous[ sessionId ] ?? 0) + 1,
        }));
        const timeout = window.setTimeout(() => {
          completionFlashTimeoutBySessionIdRef.current.delete(sessionId);
          setCompletionFlashNonceBySessionId((previous) => {
            if (!(sessionId in previous)) {
              return previous;
            }

            const next = { ...previous };
            delete next[ sessionId ];
            return next;
          });
        }, COMPLETION_FLASH_DURATION_MS);
        completionFlashTimeoutBySessionIdRef.current.set(sessionId, timeout);
      }
      void playCompletionSound(event.data.sound, (soundEvent, details) => {
        postSidebarDebugLog("native.agent.detection", soundEvent, details);
      });
      return;
    }

    if (event.data.type === "sessionPresentationChanged") {
      applySessionPresentationMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarGroupsChanged") {
      applyGroupsChangedMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarProjectCollectionsChanged") {
      /*
      CDXC:SidebarProjectCollections 2026-07-18-00:00:
      gxserver's normalized copy is authoritative whenever it has collections;
      adopt it into the localStorage-backed state so edits from React Native Android or
      another desktop land here. An empty server copy while local collections
      exist means gxserver has no durable state yet (first run after the
      server-backed cutover), so seed it from the local overlay instead of
      wiping the user's groups. nextCollectionNumber keeps the local maximum
      so "Group N" numbering never goes backwards.
      */
      if (!enableProjectCollections) {
        return;
      }
      const parsed = parseSidebarProjectCollectionsFromGxserver(
        event.data.sidebarProjectCollections,
      );
      if (!parsed) {
        return;
      }
      const remoteMachineId = event.data.remoteMachineId;
      if (remoteMachineId) {
        setRemoteProjectCollectionsByMachineId((previous) => ({
          ...previous,
          [remoteMachineId]: parsed,
        }));
        return;
      }
      if (parsed.collections.length === 0) {
        if (projectCollections.collections.length > 0) {
          lastGxserverSyncedProjectCollectionsRef.current = projectCollections;
          vscode.postMessage({
            state: serializeSidebarProjectCollectionsForGxserver(projectCollections),
            type: "updateSidebarProjectCollections",
          });
        }
        return;
      }
      const adopted: SidebarProjectCollectionsState = {
        collections: parsed.collections,
        nextCollectionNumber: Math.max(
          parsed.nextCollectionNumber,
          projectCollections.nextCollectionNumber,
        ),
      };
      lastGxserverSyncedProjectCollectionsRef.current = adopted;
      if (!areSidebarProjectCollectionsStatesEqual(adopted, projectCollections)) {
        setProjectCollections(adopted);
      }
      return;
    }

    if (event.data.type === "sidebarHudChanged") {
      applyHudChangedMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarCommandRunStateChanged") {
      applyCommandRunStateMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarCommandRunStateCleared") {
      applyCommandRunStateClearedMessage(event.data);
      return;
    }

    if (event.data.type === "revealSidebarSession") {
      setSessionRevealRequest({
        requestId: event.data.requestId,
        sessionId: event.data.sessionId,
      });
      return;
    }

    if (event.data.type === "sidebarOrderSyncResult") {
      postSidebarOrderReproLog(vscode, "repro.sidebarOrder.webview.syncResultReceived", {
        itemIds: event.data.itemIds,
        kind: event.data.kind,
        requestId: event.data.requestId,
        status: event.data.status,
      });
      applyOrderSyncResultMessage(event.data);
      return;
    }

    if (event.data.type === "daemonSessionsState") {
      setDaemonSessionsState(event.data);
      return;
    }

    if (event.data.type === "promptGitCommit") {
      setGitCommitDraft(event.data);
      return;
    }

    if (event.data.type === "sidebarGitFileDiff") {
      /*
      CDXC:GitReview 2026-06-24-15:22:
      Inline commit-review diffs may now arrive from any shared SidebarApp host.
      Apply them only to the matching open request so an async gxserver diff from an older review cannot populate a later modal.
      */
      if (useSidebarStore.getState().gitCommitDraft?.requestId === event.data.requestId) {
        setGitFileDiffDraft(event.data.draft);
      }
      return;
    }

    if (event.data.type === "previousSessionsResult") {
      if (event.data.requestId !== latestSessionSearchPreviousRequestIdRef.current) {
        return;
      }
      setRemoteSessionSearchPreviousSessions(event.data.previousSessions);
      return;
    }

    if (event.data.type === "remoteMachineStatus") {
      const remoteMachineStatus = event.data as RemoteMachineRuntimeStatus;
      setRemoteMachineRuntimeStatuses((current) => ({
        ...current,
        [ remoteMachineStatus.machineId ]: remoteMachineStatus.state,
      }));
      setRemoteMachineStatusMessages((current) => {
        const message = remoteMachineStatus.message?.trim();
        if (message) {
          return { ...current, [ remoteMachineStatus.machineId ]: message };
        }
        if (current[ remoteMachineStatus.machineId ] === undefined) {
          return current;
        }
        const next = { ...current };
        delete next[ remoteMachineStatus.machineId ];
        return next;
      });
      return;
    }

    if (event.data.type === "showSessionRenameModal") {
      dismissAppModalForSidebarNavigation("SettingsDismissal:renameSession");
      openAppModal({
        initialTitle: event.data.initialTitle,
        modal: "renameSession",
        sessionAgentIcon: event.data.sessionAgentIcon,
        sessionId: event.data.sessionId,
        type: "open",
      });
      return;
    }

    if (event.data.type !== "hydrate" && event.data.type !== "sessionState") {
      return;
    }

    postSidebarOrderReproLog(vscode, "repro.sidebarOrder.webview.messageReceived", {
      agentIds: event.data.hud.agents.map((agent) => agent.agentId),
      commandIds: event.data.hud.commands.map((command) => command.commandId),
      groupCount: event.data.groups.length,
      groupIds: event.data.groups.map((group) => group.groupId),
      messageType: event.data.type,
      revision: event.data.revision,
    });
    postSidebarStartupReproLog("messageReceived", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      groupCount: event.data.groups.length,
      hasHydrateBeforeMessage: hasAppliedHydrateRef.current,
      firstHydrateRevision: firstHydrateRevisionRef.current,
      messageType: event.data.type,
      previousRevision: revision,
      revision: event.data.revision,
      sessionCount: countSidebarSessions(event.data.groups),
      stale: event.data.revision < revision,
      startupInteractionBlocked: isStartupInteractionBlocked,
    });
    const messageSettings = event.data.hud.settings ?? effectiveSettings;
    const messageSidebarRefreshDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
      messageSettings.diagnosticLogging,
      "native.sidebar.refresh",
    );
    const messageSidebarCollapseDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
      messageSettings.diagnosticLogging,
      "native.sidebar.collapse",
    );
    postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, "messageReceived", {
      ...summarizeSidebarRefreshMessage(event.data, revision),
      hasHydrateBeforeMessage: hasAppliedHydrateRef.current,
      instanceId: refreshDebugInstanceIdRef.current,
    });
    const sidebarCollapseMessageSessionCount = countSidebarSessions(event.data.groups);
    const sidebarCollapseMessageShape = [
      event.data.type,
      event.data.groups.length,
      sidebarCollapseMessageSessionCount,
      event.data.revision < revision ? "stale" : "fresh",
    ].join(":");
    const shouldLogSidebarCollapseHydrateMessage =
      messageSidebarCollapseDiagnosticLoggingEnabled &&
      getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current) <=
      SIDEBAR_STARTUP_REPRO_WINDOW_MS &&
      (collapseStateHydrateLogCountRef.current < 8 ||
        lastCollapseStateHydrateShapeRef.current !== sidebarCollapseMessageShape);
    if (shouldLogSidebarCollapseHydrateMessage) {
      /**
       * CDXC:SidebarCollapseDiagnostics 2026-06-02-22:18:
       * Collapse-state startup logs need the first hydrate sequence and shape
       * changes, not every repeated gxserver presentation refresh. Limit the
       * high-frequency message logs so support bundles stay readable while
       * still capturing partial 2-group startup hydrates.
       */
      collapseStateHydrateLogCountRef.current += 1;
      lastCollapseStateHydrateShapeRef.current = sidebarCollapseMessageShape;
      postSidebarCollapseStateLog(
        "messageReceived",
        {
          collapsedGroupCount: Object.keys(collapsedGroupsById).length,
          groupCount: event.data.groups.length,
          isReferenceChatsCollapsed,
          isReferenceProjectsCollapsed,
          messageRevision: event.data.revision,
          messageType: event.data.type,
          sessionCount: sidebarCollapseMessageSessionCount,
          stale: event.data.revision < revision,
        },
        { enabled: true },
      );
    }
    if (
      messageSidebarRefreshDiagnosticLoggingEnabled &&
      !didLogRefreshInstanceObservedRef.current
    ) {
      didLogRefreshInstanceObservedRef.current = true;
      postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, "appInstanceObserved", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        instanceId: refreshDebugInstanceIdRef.current,
        messageType: event.data.type,
        revision: event.data.revision,
      });
    }
    if (event.data.type === "sessionState" && !hasAppliedHydrateRef.current) {
      postSidebarStartupReproLog("sessionStateBeforeHydrate", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        previousRevision: revision,
        revision: event.data.revision,
        sessionCount: countSidebarSessions(event.data.groups),
      });
    }
    /*
     * CDXC:AgentDetection 2026-04-27-07:29
     * Agent-icon debugging must verify the message boundary, not the CSS layer:
     * log whether native-projected agentIcon values reach the sidebar webview
     * and survive the Zustand store apply step.
     */
    postSidebarAgentIconBoundaryLog(vscode, "sidebar.agentIcon.messageReceived", {
      messageType: event.data.type,
      revision: event.data.revision,
      summary: summarizeSidebarAgentIconsFromGroups(event.data.groups),
    });

    if (pendingCreateGroupRef.current) {
      const nextGroupId = findCreatedGroupId(
        groupOrder,
        event.data.groups.map((group) => group.groupId),
      );
      if (nextGroupId) {
        setAutoEditingGroupId(nextGroupId);
        pendingCreateGroupRef.current = false;
      }
    }

    applySidebarMessage(event.data);
    postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, "messageApplied", {
      ...summarizeSidebarRefreshMessage(event.data, revision),
      hasHydrateAfterApply: hasAppliedHydrateRef.current,
      instanceId: refreshDebugInstanceIdRef.current,
      storeRevisionAfterApply: useSidebarStore.getState().revision,
      storeSessionCountAfterApply: Object.keys(useSidebarStore.getState().sessionsById).length,
    });
    postSidebarAgentIconBoundaryLog(vscode, "sidebar.agentIcon.messageApplied", {
      messageType: event.data.type,
      revision: event.data.revision,
      summary: summarizeSidebarAgentIconsFromStore(useSidebarStore.getState().sessionsById),
    });
    if (event.data.type === "hydrate" && !hasAppliedHydrateRef.current) {
      hasAppliedHydrateRef.current = true;
      firstHydrateRevisionRef.current = event.data.revision;
    }
    if (shouldLogSidebarCollapseHydrateMessage) {
      postSidebarCollapseStateLog(
        "messageApplied",
        {
          collapsedGroupCount: Object.keys(collapsedGroupsById).length,
          groupCount: event.data.groups.length,
          isReferenceChatsCollapsed,
          isReferenceProjectsCollapsed,
          messageRevision: event.data.revision,
          messageType: event.data.type,
          sessionCount: sidebarCollapseMessageSessionCount,
          storeCollapsedGroupCount: Object.keys(collapsedGroupsById).length,
          storeRevisionAfterApply: useSidebarStore.getState().revision,
        },
        { enabled: true },
      );
    }
    postSidebarStartupReproLog("messageApplied", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      groupCount: event.data.groups.length,
      hasHydrateAfterApply: hasAppliedHydrateRef.current,
      firstHydrateRevision: firstHydrateRevisionRef.current,
      messageType: event.data.type,
      previousRevision: revision,
      revision: event.data.revision,
      sessionCount: countSidebarSessions(event.data.groups),
      stale: event.data.revision < revision,
      startupInteractionBlocked: isStartupInteractionBlocked,
    });
  });

  useEffect(() => {
    /*
    CDXC:SidebarRefreshDiagnostics 2026-06-06-23:18:
    The mount/unmount diagnostic must describe the React app lifetime only. Including effect-event callbacks in this dependency list made every hydrate render look like an app remount in persistent logs, hiding the real refresh cadence and adding avoidable Debugging Mode noise.
    */
    const instanceId = refreshDebugInstanceIdRef.current;
    postSidebarStartupReproLog("appMounted", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      startupInteractionBlockMs: SIDEBAR_STARTUP_INTERACTION_BLOCK_MS,
    });
    postSidebarRefreshLifecycleLog("appMounted", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      instanceId,
      revision: useSidebarStore.getState().revision,
      sessionCount: Object.keys(useSidebarStore.getState().sessionsById).length,
    });

    return () => {
      postSidebarStartupReproLog("appUnmounted", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        finalRevision: useSidebarStore.getState().revision,
      });
      postSidebarRefreshLifecycleLog("appUnmounted", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        finalRevision: useSidebarStore.getState().revision,
        instanceId,
        sessionCount: Object.keys(useSidebarStore.getState().sessionsById).length,
      });
    };
  }, []);

  useEffect(() => {
    if (
      !sidebarCollapseDiagnosticLoggingEnabled ||
      didLogInitialUiCollapseStateReadRef.current
    ) {
      return;
    }

    didLogInitialUiCollapseStateReadRef.current = true;
    postSidebarCollapseStateLog("initialRead", {
      ...summarizeSidebarUiCollapseRead(initialUiCollapseStateRead),
      currentCollapsedGroupCount: Object.keys(collapsedGroupsById).length,
      groupCount: groupOrder.length,
      sessionCount: Object.keys(sessionsById).length,
      workspaceGroupCount: workspaceGroupIds.length,
    });
  }, [
    collapsedGroupsById,
    groupOrder,
    initialUiCollapseStateRead,
    sidebarCollapseDiagnosticLoggingEnabled,
    sessionsById,
    workspaceGroupIds,
  ]);

  useEffect(() => {
    const renderState = {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      firstHydrateRevision: firstHydrateRevisionRef.current,
      groupCount: groupOrder.length,
      hasHydrate: hasAppliedHydrateRef.current,
      revision,
      sessionCount: Object.keys(sessionsById).length,
      startupInteractionBlocked: isStartupInteractionBlocked,
      workspaceGroupCount: workspaceGroupIds.length,
    };
    const renderStateKey = JSON.stringify(renderState);
    if (lastSidebarStartupRenderStateKeyRef.current === renderStateKey) {
      return;
    }

    lastSidebarStartupRenderStateKeyRef.current = renderStateKey;
    postSidebarStartupReproLog("renderState", renderState);
    postSidebarRefreshDebugLog(sidebarRefreshDiagnosticLoggingEnabled, vscode, "renderStateChanged", {
      ...renderState,
      instanceId: refreshDebugInstanceIdRef.current,
    });
    if (hasAppliedHydrateRef.current && renderState.sessionCount === 0) {
      postSidebarStartupReproLog("emptyStateAfterHydrate", renderState);
      postSidebarRefreshDebugLog(sidebarRefreshDiagnosticLoggingEnabled, vscode, "emptyStateAfterHydrate", {
        ...renderState,
        instanceId: refreshDebugInstanceIdRef.current,
      });
    }
  }, [
    groupOrder,
    isStartupInteractionBlocked,
    postSidebarStartupReproLog,
    revision,
    sidebarRefreshDiagnosticLoggingEnabled,
    sessionsById,
    vscode,
    workspaceGroupIds,
  ]);

  useEffect(() => {
    const handleMessage = (event: Event) => {
      if (event instanceof MessageEvent) {
        handleWindowMessage(event);
      }
    };

    messageSource.addEventListener("message", handleMessage);

    return () => {
      messageSource.removeEventListener("message", handleMessage);
    };
  }, [ handleWindowMessage, messageSource ]);

  useEffect(() => {
    if (!nativeHostEventSource) {
      return;
    }

    const handleNativeHostEvent = (event: Event) => {
      if (!(event instanceof CustomEvent)) {
        return;
      }

      handleWindowMessage(
        new MessageEvent<ExtensionToSidebarMessage>("message", {
          data: event.detail,
        }),
      );
    };

    /**
     * CDXC:Hotkeys 2026-06-05-21:17:
     * Native macOS shortcuts arrive through the Ghostex host custom event, while extension-style traffic arrives through postMessage. Route both into the same sidebar action handler so Cmd+number uses the visible-row slot resolver consistently.
     *
     * CDXC:Hotkeys 2026-06-12-12:33:
     * The native sidebar wrapper owns typed nativeHotkey host events. Allow that wrapper to disable this shared listener so Cmd+T creates one terminal tab instead of running both the wrapper action and the shared SidebarApp createSession bridge.
     */
    nativeHostEventSource.addEventListener("ghostex-native-host-event", handleNativeHostEvent);

    return () => {
      nativeHostEventSource.removeEventListener("ghostex-native-host-event", handleNativeHostEvent);
    };
  }, [ handleWindowMessage, nativeHostEventSource ]);

  useEffect(() => {
    return () => {
      for (const timeout of completionFlashTimeoutBySessionIdRef.current.values()) {
        window.clearTimeout(timeout);
      }
      completionFlashTimeoutBySessionIdRef.current.clear();

      for (const timeoutId of Object.values(referenceSectionAnimationTimeoutsRef.current)) {
        if (timeoutId !== undefined) {
          window.clearTimeout(timeoutId);
        }
      }
      referenceSectionAnimationTimeoutsRef.current = {};
    };
  }, []);

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      postSidebarStartupReproLog("interactionBlockReleased", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        revision: useSidebarStore.getState().revision,
      });
      setIsStartupInteractionBlocked(false);
    }, SIDEBAR_STARTUP_INTERACTION_BLOCK_MS);

    return () => {
      window.clearTimeout(timeout);
    };
  }, []);

  useEffect(() => {
    document.body.dataset.sidebarTheme = theme;
    const normalizedThemeColor = normalizeWorkspaceThemeColor(customThemeColor);
    const customSidebarTitlebarColorsEnabled =
      effectiveSettings.customSidebarTitlebarColorsEnabled === true;
    const customSidebarTitlebarForegroundColor = getSidebarTitlebarForegroundForBackground(
      effectiveSettings.customSidebarTitlebarBackgroundColor,
    );
    const customSidebarTitlebarGradientColors = getSidebarTitlebarGradientColors(
      effectiveSettings.customSidebarTitlebarBackgroundColor,
    );
    if (normalizedThemeColor) {
      /**
       * CDXC:WorkspaceTheme 2026-05-05-02:58
       * Custom workspace colors are active-project sidebar theme overrides:
       * keep the preset data-sidebar-theme as fallback, but publish validated
       * CSS variables so the app-level theme surfaces derive from the color.
       */
      document.body.dataset.sidebarCustomTheme = "true";
      document.body.style.setProperty("--workspace-sidebar-theme-color", normalizedThemeColor);
      document.body.style.setProperty(
        "--workspace-sidebar-theme-foreground",
        getWorkspaceThemeForeground(normalizedThemeColor),
      );
    } else {
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty("--workspace-sidebar-theme-color");
      document.body.style.removeProperty("--workspace-sidebar-theme-foreground");
    }

    if (customSidebarTitlebarColorsEnabled) {
      /**
       * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
       * Custom sidebar/titlebar colors are an experimental chrome override.
       * Publish dedicated CSS variables instead of mutating app theme tokens so
       * Settings modals, sidebar dropdowns, and other overlay surfaces continue
       * to resolve their normal Dark Gray/Dark 2 colors.
       *
       * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
       * The foreground is derived from the selected background at apply time.
       * Do not preserve older stored foreground choices in the sidebar DOM.
       *
       * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
       * The sidebar custom chrome background is a fixed-strength vertical
       * gradient derived from the selected tint-adjusted background. Publish
       * explicit gradient stop variables while keeping the solid background
       * token for row/card contrast calculations.
       */
      document.body.dataset.customSidebarTitlebarColors = "true";
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-foreground-color",
        customSidebarTitlebarForegroundColor,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-background-color",
        effectiveSettings.customSidebarTitlebarBackgroundColor,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-gradient-top-color",
        customSidebarTitlebarGradientColors.sidebarTop,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-gradient-bottom-color",
        customSidebarTitlebarGradientColors.sidebarBottom,
      );
    } else {
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-top-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-bottom-color");
    }

    return () => {
      delete document.body.dataset.sidebarTheme;
      delete document.body.dataset.sidebarCustomTheme;
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--workspace-sidebar-theme-color");
      document.body.style.removeProperty("--workspace-sidebar-theme-foreground");
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-top-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-bottom-color");
    };
  }, [
    customThemeColor,
    effectiveSettings.customSidebarTitlebarBackgroundColor,
    effectiveSettings.customSidebarTitlebarColorsEnabled,
    theme,
  ]);

  useEffect(() => {
    document.body.style.setProperty("--ghostex-agent-manager-zoom", `${agentManagerZoomPercent}%`);

    return () => {
      document.body.style.removeProperty("--ghostex-agent-manager-zoom");
    };
  }, [ agentManagerZoomPercent ]);

  const closeGitCommitModal = useEffectEvent((requestId: string) => {
    setGitCommitDraft(undefined);
    setGitFileDiffDraft(undefined);
    vscode.postMessage({
      requestId,
      type: "cancelSidebarGitCommit",
    });
  });

  useEffect(() => {
    if (!sessionGroupsPanelRef.current) {
      return;
    }

    sessionGroupsPanelRef.current.inert = isSidebarInteractionBlocked;
  }, [ isSidebarInteractionBlocked ]);

  const triggerReferenceSectionChildAnimation = (section: ReferenceSidebarSectionId) => {
    /**
     * CDXC:SidebarSessions 2026-05-17-00:11:
     * Reference-sidebar child entrance motion is only for explicit section
     * expansion. Session open/close hydration must not leave a durable CSS
     * state that replays the project/session "loading in" animation.
     */
    setReferenceSectionChildAnimations((previous) =>
      previous[ section ] ? previous : { ...previous, [ section ]: true },
    );

    const existingTimeoutId = referenceSectionAnimationTimeoutsRef.current[ section ];
    if (existingTimeoutId !== undefined) {
      window.clearTimeout(existingTimeoutId);
    }

    referenceSectionAnimationTimeoutsRef.current[ section ] = window.setTimeout(() => {
      setReferenceSectionChildAnimations((previous) =>
        previous[ section ] ? { ...previous, [ section ]: false } : previous,
      );
      delete referenceSectionAnimationTimeoutsRef.current[ section ];
    }, REFERENCE_SECTION_CHILD_ANIMATION_RESET_MS);
  };

  const isManualActiveSessionsSort = activeSessionsSortMode === "manual";
  /**
   * CDXC:SidebarLayout 2026-05-13-08:11
   * The reference sidebar replaces the old visible Actions/Agents grids with
   * app-modal entries, titlebar modes, and project header controls. Do not
   * mount the obsolete hidden panels in the sidebar tree.
   */
  const { groupIds: effectiveGroupIds, sessionIdsByGroup: effectiveSessionIdsByGroup } = useMemo(
    () =>
      createDisplaySessionLayout({
        sessionIdsByGroup: createWorkspaceSessionIdsByGroup(
          workspaceGroupIds,
          authoritativeSessionIdsByGroup,
        ),
        sessionsById,
        sortMode: activeSessionsSortMode,
        workspaceGroupIds,
      }),
    [ activeSessionsSortMode, authoritativeSessionIdsByGroup, sessionsById, workspaceGroupIds ],
  );
  const normalizedSessionSearchQuery = sessionSearchQuery.trim();
  const isSessionSearchFiltering =
    isSessionSearchOpen && normalizedSessionSearchQuery.length >= MIN_SESSION_SEARCH_QUERY_LENGTH;
  const isReferenceProjectsRenderedCollapsed =
    isReferenceProjectsCollapsed && !isSessionSearchFiltering;
  const projectsSectionPresence = useSidebarCollapsiblePresence(
    isReferenceProjectsRenderedCollapsed,
    effectiveSettings.sidebarCollapseAnimationDurationMs,
  );
  const isSidebarSearchProjectGroupRenderedCollapsed = (groupId: string) =>
    !isSessionSearchFiltering && collapsedGroupsById[ groupId ] === true;
  useEffect(() => {
    if (!isSessionSearchFiltering) {
      latestSessionSearchPreviousRequestIdRef.current = undefined;
      setRemoteSessionSearchPreviousSessions(undefined);
      return;
    }
    const timeoutId = window.setTimeout(() => {
      const requestId = `sidebar-search-previous-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      latestSessionSearchPreviousRequestIdRef.current = requestId;
      /*
      CDXC:GxserverPresentationSearch 2026-06-01-15:08:
      Main sidebar search must show active-session matches immediately from the hydrated presentation snapshot, then query gxserver for previous/history metadata with a 200ms debounce. Do not depend on startup-hydrated previousSessions after the hard cutover.
      */
      vscode.postMessage({
        limit: 20,
        query: normalizedSessionSearchQuery,
        requestId,
        type: "requestPreviousSessions",
      });
    }, 200);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [ isSessionSearchFiltering, normalizedSessionSearchQuery, vscode ]);
  /**
   * CDXC:ProjectBrowserTabs 2026-05-16-12:59:
   * Do not render a standalone Browsers group in the sidebar. Browser pane
   * sessions belong in their project group, and the shared workspace display
   * layout orders those project browser sessions before terminals/agents.
   */
  const displayedWorkspaceSessionIdsByGroup = useMemo(
    () =>
      createDisplayedSessionIdsByGroup({
        groupIds: effectiveGroupIds,
        query: normalizedSessionSearchQuery,
        selectedSessionTags: activeSelectedSessionTagFilters,
        sessionIdsByGroup: effectiveSessionIdsByGroup,
        sessionsById,
        shouldFilter: isSessionSearchFiltering,
      }),
    [
      effectiveGroupIds,
      effectiveSessionIdsByGroup,
      activeSelectedSessionTagFilters,
      isSessionSearchFiltering,
      normalizedSessionSearchQuery,
      sessionsById,
    ],
  );
  const hiddenCollectionMemberGroupIds = useMemo(() => {
    const hiddenKeys = new Set(hiddenCollectionKeys);
    const hiddenGroupIds = new Set<string>();
    for (const groupId of effectiveGroupIds) {
      const group = groupsById[groupId];
      const remoteMachineId = group?.remoteMachineContext?.machineId;
      const projectId = remoteMachineId
        ? group?.remoteMachineContext?.projectId
        : group?.projectContext?.editor.projectId;
      if (!projectId) {
        continue;
      }
      const collectionState = remoteMachineId
        ? remoteProjectCollectionsByMachineId[remoteMachineId]
        : projectCollections;
      const collection = collectionState?.collections.find((candidate) =>
        candidate.projectIds.includes(projectId),
      );
      if (!collection) {
        continue;
      }
      const collectionKey = remoteMachineId
        ? `remote:${remoteMachineId}:${collection.collectionId}`
        : `local:${collection.collectionId}`;
      if (hiddenKeys.has(collectionKey)) {
        hiddenGroupIds.add(groupId);
      }
    }
    return hiddenGroupIds;
  }, [
    effectiveGroupIds,
    groupsById,
    hiddenCollectionKeys,
    projectCollections,
    remoteProjectCollectionsByMachineId,
  ]);
  const displayedWorkspaceGroupIds = useMemo(
    () =>
      createDisplayedGroupIds(
        effectiveGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        isSessionSearchFiltering || activeSelectedSessionTagFilters.length > 0,
      ).filter(
        (groupId) =>
          showHiddenSidebarItems ||
          (!hiddenGroupIds.includes(groupId) && !hiddenCollectionMemberGroupIds.has(groupId)),
      ),
    [
      activeSelectedSessionTagFilters.length,
      displayedWorkspaceSessionIdsByGroup,
      effectiveGroupIds,
      isSessionSearchFiltering,
      hiddenGroupIds,
      hiddenCollectionMemberGroupIds,
      showHiddenSidebarItems,
    ],
  );
  const displayedReferenceChatGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter((groupId) => groupsById[ groupId ]?.isChatCollection),
    [ displayedWorkspaceGroupIds, groupsById ],
  );
  /*
   * CDXC:SidebarSearch 2026-06-28-06:29:
   * Search results must reveal matching live project sessions even when the
   * user's normal section or project collapse state would hide them. Treat
   * collapse as render-only while filtering so clearing search restores the
   * user's previous sidebar shape without persisting temporary expansion.
   *
   */
  const displayedReferenceProjectGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter(
        (groupId) =>
          groupId !== SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID &&
          !groupsById[ groupId ]?.isChatCollection &&
          !groupsById[ groupId ]?.remoteMachineContext,
      ),
    [ displayedWorkspaceGroupIds, groupsById ],
  );
  const groupIdsContainingActiveSession = useMemo(
    () =>
      new Set(
        effectiveGroupIds.filter(
          (groupId) =>
            groupsById[groupId]?.isActive === true &&
            (effectiveSessionIdsByGroup[groupId] ?? []).some(
              (sessionId) => sessionsById[sessionId]?.isFocused === true,
            ),
        ),
      ),
    [effectiveGroupIds, effectiveSessionIdsByGroup, groupsById, sessionsById],
  );
  const referenceProjectsSectionSessionSummary = useMemo(
    () =>
      getSidebarSectionSessionSummary(
        displayedReferenceProjectGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        sessionsById,
      ),
    [displayedReferenceProjectGroupIds, displayedWorkspaceSessionIdsByGroup, sessionsById],
  );
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The gxserver-unavailable row is a synthetic placeholder, not a project, so
   * the Inbox sidebar must not render it as one — exactly as V1 excludes it
   * from `displayedReferenceProjectGroupIds` above. The recovery message and
   * its Load Sessions action take its place (see `sidebarV2HostEmptyState`).
   */
  const sidebarV2DisplayedGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter(
        (groupId) =>
          groupId !== SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID &&
          groupsById[groupId]?.isChatCollection !== true,
      ),
    [ displayedWorkspaceGroupIds, groupsById ],
  );
  const sidebarV2SectionSessionSummary = useMemo(
    () =>
      getSidebarSectionSessionSummary(
        sidebarV2DisplayedGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        sessionsById,
      ),
    [sidebarV2DisplayedGroupIds, displayedWorkspaceSessionIdsByGroup, sessionsById],
  );
  const projectCollectionIdByProjectId = useMemo(() => {
    const next = new Map<string, string>();
    for (const collection of projectCollections.collections) {
      for (const projectId of collection.projectIds) {
        next.set(projectId, collection.collectionId);
      }
    }
    /*
     * CDXC:RemoteProjectCollections 2026-07-21:
     * Worktree children inherit their parent's collection for remote machine
     * projects too, so iterate every displayed workspace group instead of only
     * the local Projects section.
     */
    for (const groupId of displayedWorkspaceGroupIds) {
      const projectContext = groupsById[groupId]?.projectContext;
      const projectId = projectContext?.editor.projectId;
      const parentProjectId = projectContext?.worktree?.parentProjectId;
      const parentCollectionId = parentProjectId
        ? next.get(parentProjectId)
        : undefined;
      if (projectId && parentCollectionId) {
        next.set(projectId, parentCollectionId);
      }
    }
    return next;
  }, [ displayedWorkspaceGroupIds, groupsById, projectCollections ]);
  /*
   * CDXC:RemoteProjectCollections 2026-07-21:
   * The collection/project interleaving is shared between the local Projects
   * section and each remote machine section, so the builder takes the section's
   * group ids instead of closing over the local list. Remote machines only
   * render collections that have displayed member projects on that machine.
   */
  const buildProjectCollectionRenderItems = (
    sectionGroupIds: readonly string[],
    collectionState: SidebarProjectCollectionsState = projectCollections,
    resolveProjectId: (groupId: string) => string | undefined = (groupId) =>
      groupsById[groupId]?.projectContext?.editor.projectId,
  ): SidebarProjectCollectionRenderItem[] => {
    if (!enableProjectCollections) {
      return sectionGroupIds.map((groupId) => ({ groupId, kind: "project" }));
    }
    const groupIdByProjectId = new Map<string, string>();
    const projectIdByGroupId = new Map<string, string>();
    for (const groupId of sectionGroupIds) {
      const projectId = resolveProjectId(groupId);
      if (projectId) {
        groupIdByProjectId.set(projectId, groupId);
        projectIdByGroupId.set(groupId, projectId);
      }
    }
    const collectionIdByProjectId = new Map<string, string>();
    for (const collection of collectionState.collections) {
      for (const projectId of collection.projectIds) {
        collectionIdByProjectId.set(projectId, collection.collectionId);
      }
    }
    for (const groupId of sectionGroupIds) {
      const projectId = projectIdByGroupId.get(groupId);
      const parentProjectId = groupsById[groupId]?.projectContext?.worktree?.parentProjectId;
      const inheritedCollectionId = parentProjectId
        ? collectionIdByProjectId.get(parentProjectId)
        : undefined;
      if (projectId && inheritedCollectionId) {
        collectionIdByProjectId.set(projectId, inheritedCollectionId);
      }
    }
    /*
     * CDXC:GroupedProjectsFirst 2026-07-21:
     * Collections render first, in their definition order (which collection
     * drags reorder), and ungrouped projects always stack below the last
     * group while keeping their own drag order among themselves.
     */
    const emittedCollectionIds = new Set<string>();
    const items: SidebarProjectCollectionRenderItem[] = sectionGroupIds.flatMap((groupId) =>
      projectIdByGroupId.has(groupId) ? [] : [{ groupId, kind: "project" as const }],
    );
    for (const collection of collectionState.collections) {
      const visibleProjectIds = sectionGroupIds.flatMap((candidateGroupId) => {
        const candidateProjectId = projectIdByGroupId.get(candidateGroupId);
        return candidateProjectId &&
          collectionIdByProjectId.get(candidateProjectId) === collection.collectionId
          ? [candidateProjectId]
          : [];
      });
      if (visibleProjectIds.length === 0) {
        continue;
      }
      emittedCollectionIds.add(collection.collectionId);
      items.push({
        collection: { ...collection, projectIds: visibleProjectIds },
        groupIds: visibleProjectIds
          .map((candidate) => groupIdByProjectId.get(candidate))
          .filter((candidate): candidate is string => Boolean(candidate)),
        kind: "collection",
      });
    }
    for (const groupId of sectionGroupIds) {
      const projectId = projectIdByGroupId.get(groupId);
      if (!projectId) {
        continue;
      }
      const collectionId = projectId ? collectionIdByProjectId.get(projectId) : undefined;
      if (collectionId && emittedCollectionIds.has(collectionId)) {
        continue;
      }
      items.push({ groupId, kind: "project" });
    }
    return items;
  };
  const displayedProjectCollectionItems = useMemo<SidebarProjectCollectionRenderItem[]>(
    () => buildProjectCollectionRenderItems(displayedReferenceProjectGroupIds),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      displayedReferenceProjectGroupIds,
      enableProjectCollections,
      groupsById,
      projectCollectionIdByProjectId,
      projectCollections.collections,
    ],
  );
  const createProjectCollectionForProject = useEffectEvent((projectId: string) => {
    const created = createSidebarProjectCollection(projectCollections, projectId);
    setProjectCollections(
      moveProjectsToSidebarCollection(
        created.state,
        getProjectCollectionFamilyProjectIds(
          projectId,
          displayedReferenceProjectGroupIds,
          groupsById,
        ),
        created.collectionId,
      ),
    );
    setAutoEditingProjectCollectionId(created.collectionId);
  });
  const moveProjectToCollection = useEffectEvent(
    (projectId: string, collectionId: string | undefined) => {
      setProjectCollections((previous) =>
        moveProjectsToSidebarCollection(
          previous,
          getProjectCollectionFamilyProjectIds(
            projectId,
            displayedReferenceProjectGroupIds,
            groupsById,
          ),
          collectionId,
        ),
      );
    },
  );
  const updateRemoteProjectCollections = useEffectEvent(
    (
      machineId: string,
      update: (state: SidebarProjectCollectionsState) => SidebarProjectCollectionsState,
    ) => {
      const current = remoteProjectCollectionsByMachineId[machineId] ?? {
        collections: [],
        nextCollectionNumber: 1,
      };
      const updated = update(current);
      setRemoteProjectCollectionsByMachineId((previous) => ({
        ...previous,
        [machineId]: updated,
      }));
      vscode.postMessage({
        remoteMachineId: machineId,
        state: serializeSidebarProjectCollectionsForGxserver(updated),
        type: "updateSidebarProjectCollections",
      });
    },
  );
  const createRemoteProjectCollectionForProject = useEffectEvent(
    (machineId: string, projectId: string, machineGroupIds: readonly string[]) => {
      const rawProjectIds = getRemoteProjectCollectionFamilyProjectIds(
        projectId,
        machineGroupIds,
        groupsById,
      );
      const rawProjectId = rawProjectIds[0];
      if (!rawProjectId) {
        return;
      }
      let createdCollectionId: string | undefined;
      updateRemoteProjectCollections(machineId, (previous) => {
        const created = createSidebarProjectCollection(previous, rawProjectId);
        createdCollectionId = created.collectionId;
        return moveProjectsToSidebarCollection(
          created.state,
          rawProjectIds,
          created.collectionId,
        );
      });
      if (createdCollectionId) {
        setAutoEditingProjectCollectionId(`${machineId}:${createdCollectionId}`);
      }
    },
  );
  const moveRemoteProjectToCollection = useEffectEvent(
    (
      machineId: string,
      projectId: string,
      collectionId: string | undefined,
      machineGroupIds: readonly string[],
    ) => {
      const rawProjectIds = getRemoteProjectCollectionFamilyProjectIds(
        projectId,
        machineGroupIds,
        groupsById,
      );
      updateRemoteProjectCollections(machineId, (previous) =>
        moveProjectsToSidebarCollection(previous, rawProjectIds, collectionId),
      );
    },
  );
  const displayedProjectCollectionGroupIds = useMemo(
    () =>
      displayedProjectCollectionItems.flatMap((item) =>
        item.kind === "project" ? [item.groupId] : item.groupIds,
      ),
    [displayedProjectCollectionItems],
  );
  const remoteProjectGroupIdsByMachineId = useMemo(() => {
    const next: Record<string, string[]> = {};
    for (const groupId of displayedWorkspaceGroupIds) {
      const remoteMachineContext = groupsById[ groupId ]?.remoteMachineContext;
      if (!remoteMachineContext) {
        continue;
      }
      next[ remoteMachineContext.machineId ] ??= [];
      next[ remoteMachineContext.machineId ].push(groupId);
    }
    return next;
  }, [ displayedWorkspaceGroupIds, groupsById ]);
  const remoteSectionSessionSummariesByMachineId = useMemo(() => {
    const next: Record<string, SidebarSectionSessionSummary> = {};
    for (const [machineId, groupIds] of Object.entries(remoteProjectGroupIdsByMachineId)) {
      next[machineId] = getSidebarSectionSessionSummary(
        groupIds,
        displayedWorkspaceSessionIdsByGroup,
        sessionsById,
      );
    }
    return next;
  }, [displayedWorkspaceSessionIdsByGroup, remoteProjectGroupIdsByMachineId, sessionsById]);
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Every machine's OWN project order, keyed by machine, which is the unit
   * `syncGroupOrder` accepts: gxserver rejects a list that mixes local and remote
   * ids or spans two remote machines outright. Grouped V2 projects a logical-row
   * reorder onto these lists and posts one message per machine that changed.
   *
   * The local list is the raw store order (`displayedReferenceProjectGroupIds`),
   * not V1's collection-interleaved render order: V2 renders no collections, so
   * the collection order is not the order the user just rearranged.
   */
  const sidebarV2GroupIdsByMachineId = useMemo(
    () => ({
      [SIDEBAR_V2_LOCAL_GROUP_ORDER_KEY]: displayedReferenceProjectGroupIds,
      ...remoteProjectGroupIdsByMachineId,
    }),
    [ displayedReferenceProjectGroupIds, remoteProjectGroupIdsByMachineId ],
  );
  const remoteMachines = settings?.remoteMachines ?? [];
  /*
   * CDXC:RemoteMachines 2026-08-08:
   * Forgetting a collapsed machine id is only meaningful against the
   * authoritative saved-machine list, and host settings arrive asynchronously.
   * Before they do, `settings?.remoteMachines ?? []` means "not known yet", not
   * "no machines" — pruning against that empty list deleted every restored
   * collapsed machine within the first frames of launch, and the persistence
   * effect below then wrote the emptied map back, so remote sections always
   * reopened expanded and the saved state was destroyed for good. This is the
   * same hazard `preserveUnknownCollapsedGroups` documents for project groups
   * (see group-collapse.ts), so gate on the same hydrate signal. The dependency
   * is a primitive id key because `remoteMachines` is a fresh array identity on
   * every render and re-ran this pass constantly.
   */
  const savedRemoteMachineIdsKey = settings
    ? JSON.stringify(settings.remoteMachines.map((machine) => machine.id))
    : undefined;
  useEffect(() => {
    if (savedRemoteMachineIdsKey === undefined || !hasAppliedHydrateRef.current) {
      return;
    }
    const remoteMachineIds = new Set<string>(JSON.parse(savedRemoteMachineIdsKey) as string[]);
    setCollapsedRemoteMachineSectionsById((previous) => {
      let next: Record<string, true> | undefined;
      for (const machineId of Object.keys(previous)) {
        if (!remoteMachineIds.has(machineId)) {
          next ??= { ...previous };
          delete next[ machineId ];
        }
      }
      return next ?? previous;
    });
  }, [ savedRemoteMachineIdsKey ]);
  const moveRemoteMachineSection = useEffectEvent(
    (sourceRemoteMachineId: string, target: SidebarRemoteMachineDropTarget) => {
      if (!settings) {
        return;
      }
      const nextRemoteMachineIds = moveRemoteMachineIdToDropTarget(
        settings.remoteMachines.map((machine) => machine.id),
        sourceRemoteMachineId,
        target,
      );
      if (!nextRemoteMachineIds) {
        return;
      }
      const machineById = new Map(
        settings.remoteMachines.map((machine) => [machine.id, machine]),
      );
      const nextRemoteMachines = nextRemoteMachineIds.flatMap((machineId) => {
        const machine = machineById.get(machineId);
        return machine ? [machine] : [];
      });
      /*
       * CDXC:RemoteMachines 2026-06-03-00:18:
       * Remote machine sidebar sections are user-orderable peers of Projects.
       * Persist the order in Settings.remoteMachines so app restart and the
       * Remote settings tab show the same section order.
       */
      vscode.postMessage({
        baseRevision: revision,
        patch: {
          remoteMachines: nextRemoteMachines,
        },
        source: "sidebar:remoteMachineOrder",
        type: "updateSettingsPatch",
      });
    },
  );
  const filteredPreviousSessions = useMemo(
    () => {
      if (!isSessionSearchFiltering) {
        return [];
      }
      const searchResults =
        remoteSessionSearchPreviousSessions ??
        filterPreviousSessions(previousSessions, normalizedSessionSearchQuery);
      return filterDefaultNamedSessionSearchItems(searchResults);
    },
    [
      isSessionSearchFiltering,
      normalizedSessionSearchQuery,
      previousSessions,
      remoteSessionSearchPreviousSessions,
    ],
  );
  const hasExpandedReferenceProjects = useMemo(
    () =>
      displayedReferenceProjectGroupIds.some((groupId) => collapsedGroupsById[ groupId ] !== true),
    [ collapsedGroupsById, displayedReferenceProjectGroupIds ],
  );
  const handleSidebarProjectJump = useEffectEvent((detail: SidebarProjectJumpEventDetail) => {
    const shouldRevealFocusedSession = detail.revealFocusedSession === true;
    const requestFocusedSessionReveal = () => {
      if (!shouldRevealFocusedSession) {
        return;
      }
      setFocusedSessionRevealRequestId((requestId) => requestId + 1);
    };

    if (
      !detail.expandCollapsedProject ||
      !displayedReferenceProjectGroupIds.includes(detail.groupId)
    ) {
      requestFocusedSessionReveal();
      return;
    }

    const wasProjectCollapsed = collapsedGroupsById[ detail.groupId ] === true;
    const wasSectionCollapsed = isReferenceProjectsCollapsed;
    if (!wasProjectCollapsed && !wasSectionCollapsed) {
      requestFocusedSessionReveal();
      return;
    }

    /**
     * CDXC:ProjectHotkeys 2026-06-15-11:12:
     * Jump to Project shortcuts are navigation in the visible Projects sidebar area. When configured, a keyboard jump must reveal a collapsed target row immediately through React state, and the optional Show less write is only applied when that project row was actually expanded by the jump.
     *
     * CDXC:SidebarSessionReveal 2026-06-16-07:55:
     * Project/worktree creation can ask this same event to retry focused-row
     * scrolling after the target project has been expanded, because a new
     * gxserver row may arrive after the first focus hydrate.
     */
    postSidebarCollapseStateLog("projectJumpAutoExpand", {
      projectGroupCount: displayedReferenceProjectGroupIds.length,
      groupHash: hashSidebarCollapseDebugId(detail.groupId),
      revealFocusedSession: shouldRevealFocusedSession,
      showLessAfterExpand: detail.showLessAfterExpand,
      wasProjectCollapsed,
      wasSectionCollapsed,
    });
    if (wasSectionCollapsed) {
      triggerReferenceSectionChildAnimation("projects");
      setIsReferenceProjectsCollapsed(false);
    }
    if (wasProjectCollapsed) {
      setGroupCollapsed(detail.groupId, false);
      if (detail.showLessAfterExpand) {
        setProjectSessionListCollapsed(detail.projectId, true);
      }
    }
    requestFocusedSessionReveal();
  });
  const resolveGpuiProjectSlotHotkey = useEffectEvent((slotNumber: number) => {
    if (!Number.isInteger(slotNumber) || slotNumber < 1 || slotNumber > 9) {
      return;
    }

    const groupId = displayedReferenceProjectGroupIds[ slotNumber - 1 ];
    const projectId = groupId ? groupsById[ groupId ]?.projectContext?.editor.projectId : undefined;
    if (!groupId || !projectId) {
      return;
    }

    /*
     * CDXC:GPUIProjectHotkeys 2026-06-26-23:42:
     * GPUI project slot messages resolve locally in SidebarApp because SidebarApp owns rendered Projects row order. Use displayedReferenceProjectGroupIds so slots match visible Projects rows while excluding Quick chats and remote machine projects, then focus the group's currently focused or first displayed session through the existing WorkspaceTerminalFocus bridge; GPUI has no focusGroup host bridge to materialize command panes.
    */
    handleSidebarProjectJump({
      expandCollapsedProject: effectiveSettings.expandCollapsedProjectsOnJump,
      groupId,
      projectId,
      revealFocusedSession: true,
      showLessAfterExpand: effectiveSettings.showLessForExpandedProjectJumps,
    });
    const groupSessionIds = displayedWorkspaceSessionIdsByGroup[ groupId ] ?? [];
    const targetSessionId =
      groupSessionIds.find((sessionId) => sessionsById[ sessionId ]?.isFocused === true) ??
      groupSessionIds[ 0 ];
    if (!targetSessionId) {
      return;
    }
    focusSidebarSessionFromNavigation(groupId, targetSessionId);
    vscode.postMessage({
      sessionId: targetSessionId,
      type: "focusSession",
    });
  });
  useEffect(() => {
    const handleProjectJumpEvent = (event: Event) => {
      const detail = readSidebarProjectJumpEventDetail(event);
      if (detail) {
        handleSidebarProjectJump(detail);
      }
    };
    window.addEventListener(SIDEBAR_PROJECT_JUMP_EVENT, handleProjectJumpEvent);
    return () => {
      window.removeEventListener(SIDEBAR_PROJECT_JUMP_EVENT, handleProjectJumpEvent);
    };
  }, [ handleSidebarProjectJump ]);
  const focusedSessionId = useMemo(
    () => Object.values(sessionsById).find((session) => session.isFocused)?.sessionId,
    [ sessionsById ],
  );
  const postMultiSelectSelectionDebugLog = useEffectEvent(
    (event: string, details: Record<string, unknown>) => {
      /*
       * CDXC:SidebarMultiSelect 2026-07-02-07:32:
       * Selection-change repros need the resolver inputs and outputs even when
       * the sidebar Debugging Mode toggle is off, so post directly instead of
       * going through postSidebarDebugLog. Persistence is gated natively by the
       * native.sidebar.refresh diagnostic scenario (Settings > Diagnostic
       * logging > "Sidebar refresh and hydration"), and payloads stay limited
       * to ids, indexes, counts, and booleans.
       */
      vscode.postMessage({
        details,
        event: `repro.sidebarMultiSelect.${event}`,
        scenarioId: "native.sidebar.refresh",
        type: "sidebarDebugLog",
      });
    },
  );
  const handleSidebarSessionSelectionChange = useEffectEvent(
    (request: {
      groupId: string;
      mode: "additive" | "clear" | "range";
      reason?: string;
      sessionId: string;
    }) => {
      if (request.mode === "clear") {
        if (selectedSidebarSessionIds.length > 0) {
          postMultiSelectSelectionDebugLog("selectionCleared", {
            previousCount: selectedSidebarSessionIds.length,
            reason: request.reason ?? "unknown",
            sessionId: request.sessionId,
          });
        }
        setSelectedSidebarSessionIds([]);
        return;
      }

      /*
       * CDXC:SidebarMultiSelect 2026-07-02-08:12:
       * A user repro log showed every shift/cmd selection resolving against
       * visibleCount:2 with clickedIndex:-1 while the sidebar rendered a full
       * project list. data-visible tracks surfaced workspace panes, so the
       * default slot filter reduced the selectable rows to the current split.
       * Selection must consider every rendered row the user can see and click.
       */
      const visibleSessionIds = readRenderedSidebarSessionSlotIds(
        sessionGroupsContentRef.current ?? document,
        { skipPaneHiddenRows: false },
      );
      if (request.mode === "range") {
        const nextSelection = resolveRenderedSidebarSessionRangeSelection({
          activeSessionId: focusedSessionId,
          clickedSessionId: request.sessionId,
          visibleSessionIds,
        });
        postMultiSelectSelectionDebugLog("selectionResolved", {
          activeIndex: focusedSessionId ? visibleSessionIds.indexOf(focusedSessionId) : -1,
          activeSessionId: focusedSessionId ?? null,
          clickedIndex: visibleSessionIds.indexOf(request.sessionId),
          clickedSessionId: request.sessionId,
          mode: request.mode,
          resultCount: nextSelection.length,
          resultSessionIds: nextSelection.slice(0, 30),
          visibleCount: visibleSessionIds.length,
        });
        setSelectedSidebarSessionIds(nextSelection);
        return;
      }

      const nextSelection = resolveRenderedSidebarSessionAdditiveSelection({
        clickedSessionId: request.sessionId,
        currentSelection: selectedSidebarSessionIds,
        visibleSessionIds,
      });
      postMultiSelectSelectionDebugLog("selectionResolved", {
        activeIndex: focusedSessionId ? visibleSessionIds.indexOf(focusedSessionId) : -1,
        activeSessionId: focusedSessionId ?? null,
        clickedIndex: visibleSessionIds.indexOf(request.sessionId),
        clickedSessionId: request.sessionId,
        currentCount: selectedSidebarSessionIds.length,
        mode: request.mode,
        resultCount: nextSelection.length,
        resultSessionIds: nextSelection.slice(0, 30),
        visibleCount: visibleSessionIds.length,
      });
      setSelectedSidebarSessionIds(nextSelection);
    },
  );
  useEffect(() => {
    /*
     * CDXC:SidebarMultiSelect 2026-07-01-18:33:
     * Multi-selected session ids are transient UI state. Hydration, close, and
     * remote updates can remove rows, so prune stale ids instead of letting a
     * later selected-row context menu target invisible or missing sessions.
     *
     * CDXC:SidebarMultiSelect 2026-07-02-07:32:
     * Pruning can also silently shrink a selection the user just made when a
     * hydrate briefly drops session records, so log every actual prune.
     */
    const nextSelection = selectedSidebarSessionIds.filter(
      (sessionId) => sessionsById[ sessionId ] !== undefined,
    );
    if (nextSelection.length === selectedSidebarSessionIds.length) {
      return;
    }
    postMultiSelectSelectionDebugLog("selectionPruned", {
      previousCount: selectedSidebarSessionIds.length,
      prunedSessionIds: selectedSidebarSessionIds
        .filter((sessionId) => sessionsById[ sessionId ] === undefined)
        .slice(0, 30),
      remainingCount: nextSelection.length,
    });
    setSelectedSidebarSessionIds(nextSelection);
  }, [ selectedSidebarSessionIds, sessionsById ]);
  const postSidebarWakeScrollLog = useEffectEvent(
    (event: string, targetSessionId: string, details: Record<string, unknown>) => {
      postSidebarDebugLog("native.sidebar.refresh", `repro.sidebarWakeScroll.${event}`, {
        ...details,
        ...summarizeSidebarWakeScrollOrderState({
          activeSessionsSortMode,
          displayedWorkspaceGroupIds,
          displayedWorkspaceSessionIdsByGroup,
          focusedSessionId: targetSessionId,
          groupsById,
          revision,
          sessionsById,
        }),
        ...summarizeSidebarWakeScrollRenderedSlots(
          sessionGroupsContentRef.current ?? document,
          targetSessionId,
        ),
      });
    },
  );
  const focusSidebarSessionSlot = useEffectEvent((slotNumber: number) => {
    /*
     * CDXC:Hotkeys 2026-06-05-20:53:
     * Cmd+1..9 must target sessions by the order of rows currently shown in the sidebar. Flatten the rendered Quick, Projects, and Remote project rows after group collapse and project Show less state so collapsed-project sessions are ignored instead of being selected from hidden inventory order.
     *
     * CDXC:Hotkeys 2026-06-05-21:17:
     * A user repro showed the state-derived slot list could reserve a number for a hidden row, so Cmd+5 selected the sixth visible session and Cmd+6 jumped much lower. Resolve the slot list from the rendered session-card DOM rows at key time so numbering follows the sidebar exactly as shown.
     */
    const root = sessionGroupsContentRef.current ?? document;
    const sessionId =
      slotNumber === 0 || slotNumber === -1
        ? resolveAdjacentRenderedSidebarSessionSlotId({
          direction: slotNumber === 0 ? 1 : -1,
          focusedSessionId,
          slots: readRenderedSidebarSessionSlots(root),
        })
        : resolveVisibleSidebarSessionSlotId({
          focusedSessionId,
          slotNumber,
          /*
           * data-visible describes whether a session is already surfaced in a
           * workspace pane, not whether its sidebar row is visible. Numbered
           * shortcuts must include every rendered row so Cmd+N can surface the
           * Nth session the user can actually see in the sidebar.
           */
          visibleSessionIds: readRenderedSidebarSessionSlotIds(root, {
            skipPaneHiddenRows: false,
          }),
        });
    if (!sessionId) {
      return;
    }

    const groupId = findSessionGroupId(displayedWorkspaceSessionIdsByGroup, sessionId);
    if (groupId) {
      applyLocalFocus(groupId, sessionId);
    }
    vscode.postMessage({
      sessionId,
      type: "focusSession",
    });
  });
  const runGhostexHotkeyAction = useEffectEvent((actionId: string) => {
    const action = getghostexHotkeyActionById(actionId);
    if (!action) {
      return;
    }

    if (action.kind === "focusSessionSlot") {
      dismissAppModalForSidebarNavigation("SettingsDismissal:focusSessionHotkey");
      focusSidebarSessionSlot(action.slotNumber);
      return;
    }

    if (action.kind === "createSession") {
      requestNewSession();
      return;
    }

    if (action.kind === "openCommandPalette") {
      openCommandPalette();
      return;
    }

    if (action.kind === "openSessionSearchPalette") {
      openPreviousSessions();
      return;
    }

    if (action.kind === "openSettings") {
      openSidebarSettings();
      return;
    }

    if (action.kind === "openHotkeys") {
      openHotkeys();
      return;
    }

    if (action.kind === "moveSidebar") {
      moveSidebar();
      return;
    }

    if (action.kind === "toggleSidebarCollapsed") {
      toggleSidebarCollapsed();
      return;
    }

    /*
     * CDXC:HotkeyRouting 2026-06-26-23:04:
     * Rename Active Session, Open Commands Panel, Start Action slots, and
     * Focus Previous/Next Group, Directional Focus, and Split Sideways/Downwards
     * are native-owned hotkey actions when dispatched through the shared
     * SidebarApp bridge. Forward only the action id and runGhostexHotkeyAction
     * type so native runNativeHotkeyAction resolves authority state without
     * renderer-owned private data payloads such as session ids, titles, paths,
     * command text, or URLs.
     *
     * CDXC:HotkeyRouting 2026-06-26-23:58:
     * View Mode switching is native-owned; SidebarApp forwards setViewMode
     * through the same action-id-only bridge so renderer state stays private.
     */
    if (
      action.kind === "focusAdjacentGroup" ||
      action.kind === "focusDirection" ||
      action.kind === "focusedPaneAction" ||
      action.kind === "jumpToProject" ||
      /*
       * CDXC:NavigationHistory 2026-08-19:
       * Back/Forward is host-owned: gpui walks the trail through its native
       * titlebar route and the web shell through its sidebar runtime, so the
       * palette row forwards the action id and nothing else, exactly like the
       * other host-owned navigation rows here.
       */
      action.kind === "navigateHistory" ||
      action.kind === "openCommandsPanel" ||
      action.kind === "renameActiveSession" ||
      action.kind === "runActionSlot" ||
      action.kind === "setViewMode" ||
      action.kind === "splitFocusedPane" ||
      action.kind === "switchWorkareaView" ||
      action.kind === "terminalToolbarAction" ||
      action.kind === "toggleCompanionPane"
    ) {
      vscode.postMessage({ actionId: action.id, type: "runGhostexHotkeyAction" });
    }
  });
  useLayoutEffect(() => {
    if (
      didApplyStartupEmptyChatsCollapseRef.current ||
      !hasAppliedHydrateRef.current
    ) {
      return;
    }

    didApplyStartupEmptyChatsCollapseRef.current = true;
    const hasChatSessions = displayedReferenceChatGroupIds.some(
      (groupId) => (authoritativeSessionIdsByGroup[ groupId ] ?? []).length > 0,
    );
    if (!hasChatSessions) {
      postSidebarCollapseStateLog("sectionAutoCollapsed", {
        reason: "startup-empty-quick",
        section: "quick",
      });
      /**
       * CDXC:SidebarReference 2026-05-10-15:51
       * Startup restores the user's section/group collapse state, except an empty
       * Combined Chats section must always begin collapsed so a project-only
       * workspace does not waste vertical space on an empty chat container.
       */
      setIsReferenceChatsCollapsed(true);
    }
  }, [ authoritativeSessionIdsByGroup, displayedReferenceChatGroupIds ]);

  useEffect(() => {
    /**
     * CDXC:SidebarReference 2026-05-10-15:51
     * Combined section headers and per-group collapse state are
     * UI navigation state. Persist them in the sidebar webview so restarting
     * ghostex keeps collapsed items collapsed and expanded items expanded.
     * CDXC:SidebarReference 2026-05-20-12:00
     * The first post-hydrate group-collapse reconcile seeds session-count baseline
     * without expand-on-count-increase so restored projects do not reopen on launch.
     *
     * CDXC:RemoteMachines 2026-06-09-19:02:
     * Remote machine section collapse belongs to the same UI navigation state as
     * Quick and Projects. Persist each machine independently by saved machine id.
     */
    const nextCollapseState = {
      collapsedGroupsById,
      collapsedProjectCollectionsByKey,
      collapsedProjectSessionListsById,
      collapsedRemoteMachineSectionsById,
      isReferenceChatsCollapsed,
      isReferenceProjectsCollapsed,
    };
    const writeResult = writeSidebarUiCollapseState(windowScopeId, nextCollapseState);
    postSidebarCollapseStateLog("write", {
      ...summarizeSidebarUiCollapseState(nextCollapseState),
      groupCount: groupOrder.length,
      storedByteLength: writeResult.storedByteLength ?? 0,
      writeOk: writeResult.ok,
      writeReason: writeResult.reason ?? "stored",
    });
  }, [
    collapsedGroupsById,
    collapsedProjectCollectionsByKey,
    collapsedProjectSessionListsById,
    collapsedRemoteMachineSectionsById,
    isReferenceChatsCollapsed,
    isReferenceProjectsCollapsed,
    windowScopeId,
  ]);

  const shouldShowSessionSearchEmptyState =
    isSessionSearchFiltering &&
    displayedWorkspaceGroupIds.length === 0 &&
    filteredPreviousSessions.length === 0;
  /**
   * CDXC:SidebarSearch 2026-05-08-11:26
   * A no-match search is its own result state. Hide the normal Chats and
   * Projects sections while it is visible so the empty placeholder has the
   * same visual role as the existing "No Quick Sessions" group placeholder.
   */
  const shouldHideReferenceSectionsForSearchEmptyState = shouldShowSessionSearchEmptyState;
  /**
   * CDXC:SidebarProjectsEmptyState 2026-06-18-06:01:
   * A sidebar with zero rendered project groups should guide first-time setup from the same left-aligned Projects empty-state block as the previous "No projects" placeholder. Tie the copy to the visible Projects label and its hover plus action instead of adding a separate card or fallback surface.
   */
  const hasKnownProjectInventoryForEmptyState = hasKnownSidebarProjectInventory({
    groupsById,
    projectSettingsProjectCount: projectSettingsProjects?.length ?? 0,
    recentProjectCount: recentProjects.length,
    unavailableProjectGroupId: SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID,
    workspaceGroupIds,
  });
  const shouldShowFirstProjectEmptyState =
    !isSessionSearchOpen && !hasKnownProjectInventoryForEmptyState;
  /*
   * CDXC:SidebarProjectsEmptyState 2026-06-30-03:25:
   * Sidebar search must not flash first-project onboarding after any project is
   * known. Search filtering and transient group display updates can temporarily
   * remove all visible Projects rows, so decide the first-run copy from
   * authoritative project inventory and parked Recent Projects instead of the
   * current displayed group arrays.
   */
  const referenceProjectsEmptyState = showGxserverUnavailableEmptyState ? (
    <div className="reference-sidebar-empty-state">
      Unable to load sessions.
      <br />
      {onStartGxserver ? (
        <button
          className="reference-sidebar-empty-state-action"
          onClick={onStartGxserver}
          type="button"
        >
          Load Sessions
        </button>
      ) : (
        "Restart Ghostex to try again."
      )}
    </div>
  ) : hasGxserverUnavailablePlaceholder ? null : (
    <div className="reference-sidebar-empty-state">
      {shouldShowFirstProjectEmptyState ? (
        <>
          No Projects Added.
          <br />
          <br />
          {"Hover over the Projects label and click on the plus button to add your first project and get started!"}
        </>
      ) : (
        "No projects"
      )}
    </div>
  );
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Two of the project-level empty states are host-owned recovery/onboarding
   * surfaces rather than list chrome — gxserver being unavailable (with its
   * Load Sessions action) and having no project at all — so BOTH sidebars have
   * to render them. Everything else an empty list can mean (search, tag
   * filters, project scope) stays each sidebar's own copy.
   */
  const shouldRenderHostProjectsEmptyState =
    hasGxserverUnavailablePlaceholder ||
    (shouldShowFirstProjectEmptyState &&
      !sidebarV2DisplayedGroupIds.some(
        (groupId) => (displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []).length > 0,
      ));
  const sidebarV2HostEmptyState = !shouldRenderHostProjectsEmptyState
    ? undefined
    : /*
       * `null` here is V1's deliberate blank Projects body during the gxserver
       * startup grace period. Hand V2 an empty node instead of `undefined` so it
       * stays blank too rather than falling through to "No sessions yet".
       */
      (referenceProjectsEmptyState ?? <></>);
  const {
    hasOverflow: sessionGroupsHaveScrollableOverflow,
  } = useScrollGlowState(sessionGroupsContentRef);
  const sidebarSessionSearchResults = useMemo(
    () =>
      createSidebarSessionSearchResults({
        displayedWorkspaceGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        filteredPreviousSessions,
      }),
    [
      displayedWorkspaceGroupIds,
      displayedWorkspaceSessionIdsByGroup,
      filteredPreviousSessions,
    ],
  );
  useEffect(() => {
    groupIdsRef.current = displayedProjectCollectionGroupIds;
  }, [ displayedProjectCollectionGroupIds ]);

  useEffect(() => {
    sessionIdsByGroupRef.current = displayedWorkspaceSessionIdsByGroup;
  }, [ displayedWorkspaceSessionIdsByGroup ]);

  useEffect(() => {
    const queryChanged =
      previousNormalizedSessionSearchQueryRef.current !== normalizedSessionSearchQuery;
    previousNormalizedSessionSearchQueryRef.current = normalizedSessionSearchQuery;

    if (
      !isSessionSearchOpen ||
      normalizedSessionSearchQuery.length === 0 ||
      sidebarSessionSearchResults.length === 0 ||
      queryChanged
    ) {
      setIsSessionSearchSelectionVisible(false);
    }

    setSelectedSessionSearchResult((previous) => {
      if (!isSessionSearchOpen || normalizedSessionSearchQuery.length === 0) {
        return previous;
      }

      if (sidebarSessionSearchResults.length === 0) {
        return undefined;
      }

      if (queryChanged) {
        return createSidebarSessionSearchSelection(sidebarSessionSearchResults[ 0 ]);
      }

      if (!previous) {
        return undefined;
      }

      return sidebarSessionSearchResults.some((result) =>
        isSidebarSessionSearchSelectionMatch(result, previous),
      )
        ? previous
        : createSidebarSessionSearchSelection(sidebarSessionSearchResults[ 0 ]);
    });
  }, [ isSessionSearchOpen, normalizedSessionSearchQuery, sidebarSessionSearchResults ]);

  useEffect(() => {
    if (!isSessionSearchSelectionVisible || !selectedSessionSearchResult) {
      return;
    }

    const selectedElement =
      selectedSessionSearchResult.kind === "session"
        ? document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${selectedSessionSearchResult.sessionId}"]`,
        )
        : document.querySelector<HTMLElement>(
          `[data-sidebar-history-id="${selectedSessionSearchResult.historyId}"]`,
        );
    selectedElement?.scrollIntoView({
      block: "nearest",
    });
  }, [ isSessionSearchSelectionVisible, selectedSessionSearchResult ]);

  useEffect(() => {
    const isExplicitFocusedSessionRevealRequest =
      focusedSessionRevealRequestId !== previousFocusedSessionRevealRequestIdRef.current;
    previousFocusedSessionRevealRequestIdRef.current = focusedSessionRevealRequestId;
    if (isExplicitFocusedSessionRevealRequest) {
      useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    }

    if (!focusedSessionId || !sessionGroupsContentRef.current) {
      return;
    }

    /*
     * CDXC:SidebarWakeScrollDiagnostics 2026-06-16-02:20:
     * Wake-scroll repros need to prove whether the sidebar jumped because focus-following issued scrollIntoView or because the focused row moved in the displayed order. Log only session IDs, row indexes, sort mode, and geometry metrics while the native.sidebar.refresh scenario is enabled.
     *
     * CDXC:SidebarSessionClose 2026-06-21-18:02:
     * Closing the focused terminal session should retarget native focus without reveal-scrolling the sidebar. Consume the one-shot close marker before scrollIntoViewIfNeeded so the user's list position stays stable after close.
     */
    let afterAnimationFrameId: number | undefined;
    let afterSettledTimeoutId: number | undefined;
    const sequence = ++focusedSessionScrollLogSequenceRef.current;
    const animationFrameId = window.requestAnimationFrame(() => {
      const scrollViewport = sessionGroupsContentRef.current;
      if (!scrollViewport) {
        postSidebarWakeScrollLog("focusedRowScrollSkipped", focusedSessionId, {
          reason: "missing-scroll-viewport",
          sequence,
        });
        return;
      }

      if (!isExplicitFocusedSessionRevealRequest) {
        const suppression = consumeFocusedSessionScrollSuppression();
        if (suppression) {
          postSidebarWakeScrollLog("focusedRowScrollSkipped", focusedSessionId, {
            reason: "close-driven-focus-scroll-suppressed",
            sequence,
            suppressionReason: suppression.reason,
          });
          return;
        }
      }

      const focusedSessionElement = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${focusedSessionId}"]`,
      );
      if (!focusedSessionElement) {
        postSidebarWakeScrollLog("focusedRowScrollSkipped", focusedSessionId, {
          reason: "missing-focused-row",
          sequence,
        });
        return;
      }

      const beforeScrollTop = scrollViewport.scrollTop;
      const beforeGeometry = summarizeSidebarWakeScrollGeometry(
        focusedSessionElement,
        scrollViewport,
      );
      const scrollIssued = scrollElementIntoViewIfNeeded(focusedSessionElement, scrollViewport);
      postSidebarWakeScrollLog("focusedRowScrollDecision", focusedSessionId, {
        beforeGeometry,
        scrollIssued,
        sequence,
      });

      if (!scrollIssued) {
        return;
      }

      afterAnimationFrameId = window.requestAnimationFrame(() => {
        const nextScrollViewport = sessionGroupsContentRef.current;
        const nextFocusedSessionElement = document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${focusedSessionId}"]`,
        );
        postSidebarWakeScrollLog("focusedRowScrollAfterFrame", focusedSessionId, {
          afterGeometry: nextScrollViewport && nextFocusedSessionElement
            ? summarizeSidebarWakeScrollGeometry(nextFocusedSessionElement, nextScrollViewport)
            : undefined,
          scrollDeltaTop: nextScrollViewport ? nextScrollViewport.scrollTop - beforeScrollTop : undefined,
          sequence,
        });
      });
      afterSettledTimeoutId = window.setTimeout(() => {
        const settledScrollViewport = sessionGroupsContentRef.current;
        const settledFocusedSessionElement = document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${focusedSessionId}"]`,
        );
        postSidebarWakeScrollLog("focusedRowScrollAfterSettled", focusedSessionId, {
          afterGeometry: settledScrollViewport && settledFocusedSessionElement
            ? summarizeSidebarWakeScrollGeometry(settledFocusedSessionElement, settledScrollViewport)
            : undefined,
          scrollDeltaTop: settledScrollViewport
            ? settledScrollViewport.scrollTop - beforeScrollTop
            : undefined,
          sequence,
        });
      }, 350);
    });

    return () => {
      window.cancelAnimationFrame(animationFrameId);
      if (afterAnimationFrameId !== undefined) {
        window.cancelAnimationFrame(afterAnimationFrameId);
      }
      if (afterSettledTimeoutId !== undefined) {
        window.clearTimeout(afterSettledTimeoutId);
      }
    };
  }, [ consumeFocusedSessionScrollSuppression, focusedSessionId, focusedSessionRevealRequestId ]);

  /*
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * Opening a Browser tab must leave the user able to SEE it in the sidebar.
   * Every collapsed container between the sidebar scroller and the row is
   * expanded for real (the same persisted collapse state the chevrons write, so
   * the expansion sticks), the group section is told to open the row's own
   * kind section, and the row is scrolled into view only if it is off screen.
   *
   * The scroll waits for the expand transitions the browser actually created
   * instead of a matching JS timer, exactly like `useSidebarCollapsiblePresence`
   * waits to unmount: measuring mid-animation reads a collapsed body as
   * zero-height and decides the row is already visible when it is not.
   */
  useEffect(() => {
    if (!sessionRevealRequest) {
      return;
    }
    const { requestId, sessionId } = sessionRevealRequest;
    if (handledSessionRevealRequestIdRef.current !== requestId) {
      const groupId = Object.keys(displayedWorkspaceSessionIdsByGroup).find((candidateGroupId) =>
        displayedWorkspaceSessionIdsByGroup[ candidateGroupId ]?.includes(sessionId),
      );
      if (!groupId) {
        // The row has not been published yet; this effect re-runs when it is.
        return;
      }
      handledSessionRevealRequestIdRef.current = requestId;
      pendingSessionRevealScrollRequestIdRef.current = requestId;

      if (isReferenceProjectsCollapsed) {
        triggerReferenceSectionChildAnimation("projects");
        setIsReferenceProjectsCollapsed(false);
      }
      const collectionItem = displayedProjectCollectionItems.find(
        (item) => item.kind === "collection" && item.groupIds.includes(groupId),
      );
      if (collectionItem?.kind === "collection") {
        setProjectCollectionCollapsed(
          createLocalProjectCollectionCollapseKey(collectionItem.collection.collectionId),
          false,
        );
      }
      setGroupCollapsed(groupId, false);
      const projectId = groupsById[ groupId ]?.projectContext?.editor.projectId;
      if (projectId) {
        setProjectSessionListCollapsed(projectId, false);
      }
    }

    /*
     * The expansions above happen once, but the scroll they enable must survive
     * this effect being torn down and re-run: a sidebar refresh landing in the
     * same frame changes the deps, which cancels the pending frame. Keeping the
     * scroll pending until it actually runs makes the re-run reschedule it
     * instead of dropping it.
     */
    if (pendingSessionRevealScrollRequestIdRef.current !== requestId) {
      return;
    }

    let cancelled = false;
    const scrollRevealedRowIntoView = () => {
      const scrollViewport = sessionGroupsContentRef.current;
      const revealedRow = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${sessionId}"]`,
      );
      if (cancelled || !scrollViewport || !revealedRow) {
        return;
      }
      pendingSessionRevealScrollRequestIdRef.current = undefined;
      scrollElementIntoViewIfNeeded(revealedRow, scrollViewport);
    };
    const animationFrameId = window.requestAnimationFrame(() => {
      if (cancelled) {
        return;
      }
      const scrollViewport = sessionGroupsContentRef.current;
      const revealedRow = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${sessionId}"]`,
      );
      if (!scrollViewport || !revealedRow) {
        return;
      }
      const expandAnimations: Animation[] = [];
      for (
        let ancestor = revealedRow.parentElement;
        ancestor && ancestor !== scrollViewport;
        ancestor = ancestor.parentElement
      ) {
        expandAnimations.push(...ancestor.getAnimations());
      }
      if (expandAnimations.length === 0) {
        scrollRevealedRowIntoView();
        return;
      }
      void Promise.allSettled(
        expandAnimations.map((expandAnimation) => expandAnimation.finished),
      ).then(scrollRevealedRowIntoView);
    });

    return () => {
      cancelled = true;
      window.cancelAnimationFrame(animationFrameId);
    };
  }, [
    /*
     * The collapse state is a real dependency, not incidental: expanding a
     * collapsed project mounts its body, so the row this effect scrolls to only
     * exists in the DOM on the run that follows its own expansion.
     */
    collapsedGroupsById,
    collapsedProjectCollectionsByKey,
    collapsedProjectSessionListsById,
    displayedProjectCollectionItems,
    displayedWorkspaceSessionIdsByGroup,
    groupsById,
    isReferenceProjectsCollapsed,
    sessionRevealRequest,
  ]);

  const unlockCompletionSoundPlayback = useEffectEvent(() => {
    void prepareCompletionSoundPlayback((soundEvent, details) => {
      postSidebarDebugLog("native.agent.detection", soundEvent, details);
    });
  });

  const recordPointerDownSessionTarget = useEffectEvent((event: PointerEvent) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      pointerDownSessionTargetRef.current = undefined;
      return;
    }

    const sessionElement = target.closest<HTMLElement>("[data-sidebar-session-id]");
    const groupElement = target.closest<HTMLElement>(
      "[data-sidebar-session-group-id], [data-sidebar-group-id]",
    );
    const sessionId = sessionElement?.dataset.sidebarSessionId;
    const groupId =
      groupElement?.dataset.sidebarSessionGroupId ?? groupElement?.dataset.sidebarGroupId;
    if (!sessionId || !groupId) {
      pointerDownSessionTargetRef.current = undefined;
      return;
    }

    pointerDownSessionTargetRef.current = {
      groupId,
      point: {
        x: event.clientX,
        y: event.clientY,
      },
      sessionId,
    };

    if (sessionsById[ sessionId ]?.isPinned === true) {
      /*
       * CDXC:PinnedSessions 2026-06-02-19:53:
       * Pinned project-session reorder regressions can fail before dnd-kit
       * emits a session drag. Persist one pointer-down breadcrumb for pinned
       * rows so support can distinguish "drag never started" from "drop guard
       * skipped sync" without logging titles, paths, commands, or user text.
       */
      postPinnedSessionReorderLog("pointerDown", {
        groupCollapsed: collapsedGroupsById[ groupId ] === true,
        pointer: summarizePointerEventForPinnedReorder(event),
        state: createPinnedSessionReorderDebugState(
          { groupId, kind: "session", sessionId },
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetDom: createPinnedSessionDomDebugState(groupId, sessionId),
      });
    }
  });

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      recordPointerDownSessionTarget(event);
      unlockCompletionSoundPlayback();
    };
    const handleKeyDown = () => {
      pointerDownSessionTargetRef.current = undefined;
      unlockCompletionSoundPlayback();
    };

    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown, true);

    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [ recordPointerDownSessionTarget, unlockCompletionSoundPlayback ]);

  /*
   * CDXC:RemoteGroupReorder 2026-07-12:
   * Remote machine project groups reorder among their own machine's rows only.
   * Resolve drag candidates from the source group's scope so a remote drag
   * cannot target local Projects rows (and vice versa), while local project
   * drags keep using the collection-ordered id list.
   */
  const groupDragCandidateIdsForSource = (sourceGroupId: string): readonly string[] => {
    /*
     * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
     * Grouped V2 does not have per-machine sections to scope a drag to. Its rows
     * ARE logical projects that may span machines, so every rendered row is a
     * candidate and the machine split moves to the other end of the operation:
     * the drop is projected back onto each machine's own list on release.
     */
    if (isSidebarV2GroupedActive) {
      return sidebarV2GroupOrderRowsRef.current.map((row) => row.groupId);
    }
    const machineId = groupsById[ sourceGroupId ]?.remoteMachineContext?.machineId;
    if (machineId) {
      return remoteProjectGroupIdsByMachineId[ machineId ] ?? [];
    }
    return groupIdsRef.current;
  };

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Written from V2's own render, read only inside a drag. It is a ref rather
   * than state on purpose: the rendered row list changes on every session
   * update, and mirroring it into state would re-render the whole sidebar for
   * information nothing paints. The identity is stable so V2's reporting effect
   * fires on row changes, not on every SidebarApp render.
   */
  const setSidebarV2GroupOrderRows = useCallback((rows: readonly SidebarV2GroupOrderRow[]) => {
    sidebarV2GroupOrderRowsRef.current = rows;
  }, []);

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Grouped V2's no-op answer, supplied to the shared pointer resolver so the
   * drop line appears exactly where a release would actually reorder something.
   */
  const sidebarV2GroupNoOpTargetForSource = (sourceGroupId: string) =>
    isSidebarV2GroupedActive
      ? (target: SidebarGroupDropTarget) =>
        moveSidebarV2GroupRows(sidebarV2GroupOrderRowsRef.current, sourceGroupId, target) ===
        undefined
      : undefined;

  const updateSessionDropIndicator = useEffectEvent(
    (event: Parameters<NonNullable<DragDropEventHandlers[ "onDragOver" ]>>[ 0 ]) => {
      const sourceData = getSidebarDropData(event.operation.source);
      if (sourceData?.kind === "remote-machine") {
        setGroupDropIndicator(undefined);
        setPinnedSessionDropIndicator(undefined);
        setProjectCollectionDropIndicator(undefined);
        setProjectUngroupDropIndicatorScopeId(undefined);
        setSessionDropIndicator(undefined);
        const resolvedRemoteMachineDropTarget = resolveRemoteMachineDropTargetFromPoint(
          getDragNativeEvent(event),
          remoteMachines.map((machine) => machine.id),
          sourceData.remoteMachineId,
          getSidebarDropData(event.operation.target),
        );
        setRemoteMachineDropIndicator((previous) =>
          areSameRemoteMachineDropTarget(previous, resolvedRemoteMachineDropTarget)
            ? previous
            : resolvedRemoteMachineDropTarget,
        );
        return;
      }

      setRemoteMachineDropIndicator(undefined);
      if (sourceData?.kind === "group") {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        const nativeEvent = getDragNativeEvent(event);
        const sourceProjectId =
          groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const resolvedUngroupDropScopeId =
          sourceProjectId && projectCollectionIdByProjectId.has(sourceProjectId)
            ? resolveProjectUngroupDropScopeFromPoint(
              nativeEvent,
              sourceData.groupId,
              groupsById,
            )
            : undefined;
        const resolvedGroupDropTarget = resolvedUngroupDropScopeId
          ? undefined
          : resolveGroupDropTargetFromPoint(
            nativeEvent,
            groupDragCandidateIdsForSource(sourceData.groupId),
            groupsById,
            getSidebarDropData(event.operation.target),
            sourceData,
            sidebarV2GroupNoOpTargetForSource(sourceData.groupId),
          );
        setProjectUngroupDropIndicatorScopeId((previous) =>
          previous === resolvedUngroupDropScopeId ? previous : resolvedUngroupDropScopeId,
        );
        setGroupDropIndicator((previous) =>
          areSameGroupDropTarget(previous, resolvedGroupDropTarget)
            ? previous
            : resolvedGroupDropTarget,
        );
        return;
      }

      setGroupDropIndicator(undefined);
      setProjectUngroupDropIndicatorScopeId(undefined);
      if (sourceData?.kind === "project-collection") {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        const resolvedCollectionDropTarget = resolveProjectCollectionDropTargetFromPoint(
          getDragNativeEvent(event),
          displayedProjectCollectionItems.flatMap((item) =>
            item.kind === "collection" ? [item.collection.collectionId] : [],
          ),
          sourceData.collectionId,
          getSidebarDropData(event.operation.target),
        );
        setProjectCollectionDropIndicator((previous) =>
          previous?.collectionId === resolvedCollectionDropTarget?.collectionId &&
          previous?.position === resolvedCollectionDropTarget?.position
            ? previous
            : resolvedCollectionDropTarget,
        );
        return;
      }

      setProjectCollectionDropIndicator(undefined);
      if (sourceData?.kind !== "session") {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        return;
      }

      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        setSessionDropIndicator(undefined);
        const resolvedPinnedSessionDropTarget = resolvePinnedSessionDropTargetFromPoint(
          getDragNativeEvent(event),
          sourceData,
          sessionIdsByGroupRef.current,
          sessionsById,
        );
        const pinnedTargetLogKey = createPinnedSessionDropTargetLogKey(
          sourceData,
          resolvedPinnedSessionDropTarget,
        );
        if (pinnedSessionDropTargetLogKeyRef.current !== pinnedTargetLogKey) {
          pinnedSessionDropTargetLogKeyRef.current = pinnedTargetLogKey;
          postPinnedSessionReorderLog("targetChanged", {
            point: getClientPoint(getDragNativeEvent(event)),
            resolvedPinnedSessionDropTarget,
            sourceData,
            state: createPinnedSessionReorderDebugState(
              sourceData,
              sessionIdsByGroupRef.current,
              effectiveSessionIdsByGroup,
              authoritativeSessionIdsByGroup,
              sessionsById,
            ),
          });
        }
        setPinnedSessionDropIndicator((previous) =>
          areSameSessionDropTarget(previous, resolvedPinnedSessionDropTarget)
            ? previous
            : resolvedPinnedSessionDropTarget,
        );
        return;
      }

      setPinnedSessionDropIndicator(undefined);
      const resolvedSessionDropTarget = resolveSessionDropTargetFromPoint(
        getDragNativeEvent(event),
        sessionIdsByGroupRef.current,
        getSidebarDropData(event.operation.target),
        sourceData,
      );

      /*
       * CDXC:SidebarDragDrop 2026-06-19-11:12:
       * Manual session sorting should always show an insertion line while the
       * pointer is over another session row: above the row midpoint means
       * before, below the midpoint means after. Store the resolved drop target
       * directly instead of only highlighting a target project so the visual
       * indicator does not disappear when dnd-kit reports the broader group.
       */
      setSessionDropIndicator((previous) =>
        areSameSessionDropTarget(previous, resolvedSessionDropTarget ?? undefined)
          ? previous
          : resolvedSessionDropTarget ?? undefined,
      );
    },
  );

  const handleDragStart = ((event) => {
    setSidebarTooltipsSuppressedForDrag(true);
    const nativeEvent = getDragNativeEvent(event);
    const sourceData = getSidebarDropData(event.operation.source);
    const pointerDownSessionTarget = pointerDownSessionTargetRef.current;
    setIsProjectReorderDragActive(
      sourceData?.kind === "group" ||
        sourceData?.kind === "project-collection" ||
        sourceData?.kind === "remote-machine",
    );
    if (sourceData?.kind === "group") {
      const point = getClientPoint(nativeEvent);
      const group = groupsById[ sourceData.groupId ];
      const headerMetrics = point
        ? getProjectGroupDragHeaderMetrics(sourceData.groupId, point)
        : undefined;
      /**
       * CDXC:ProjectDragPreview 2026-05-21-11:45:
       * Project drag ghosts should be anchored to the live cursor and should
       * render only the project header, even when the source project is expanded.
       * Keep the source row in the list as the faint placeholder instead of
       * cloning the whole expanded project into the moving preview.
       *
       * CDXC:ProjectDragPreview 2026-05-28-12:35:
       * The project drag ghost should preserve the grabbed header button's
       * exact left edge and width, then move only on the vertical axis. Capture
       * the header row bounds at drag start and keep the pointer's initial
       * vertical offset so horizontal pointer drift never shifts the ghost.
       */
      setGroupDragPreview(
        point && headerMetrics && group?.projectContext
          ? {
            groupId: sourceData.groupId,
            isCollapsed: collapsedGroupsById[ sourceData.groupId ] === true,
            left: headerMetrics.left,
            pointerOffsetY: headerMetrics.pointerOffsetY,
            themeColor: group.projectContext.themeColor,
            title: group.title,
            top: headerMetrics.top,
            width: headerMetrics.width,
          }
          : undefined,
      );
    } else {
      setGroupDragPreview(undefined);
    }
    if (sourceData?.kind === "project-collection") {
      const point = getClientPoint(nativeEvent);
      const collection = projectCollections.collections.find(
        (candidate) => candidate.collectionId === sourceData.collectionId,
      );
      const metrics = point
        ? getProjectCollectionDragMetrics(event.operation.source, sourceData.collectionId)
        : undefined;
      setProjectCollectionDragPreview(
        point && metrics && collection
          ? {
            collectionId: sourceData.collectionId,
            color: collection.color,
            left: metrics.left,
            pointerOffsetY: point.y - metrics.top,
            title: collection.title,
            top: metrics.top,
            width: metrics.width,
          }
          : undefined,
      );
    } else {
      setProjectCollectionDragPreview(undefined);
    }
    if (sourceData?.kind === "remote-machine") {
      const point = getClientPoint(nativeEvent);
      const machine = remoteMachines.find(
        (candidate) => candidate.id === sourceData.remoteMachineId,
      );
      const metrics = point
        ? getRemoteMachineDragHeaderMetrics(sourceData.remoteMachineId, point)
        : undefined;
      setRemoteMachineDragPreview(
        point && metrics && machine
          ? {
            collapsed:
              collapsedRemoteMachineSectionsById[sourceData.remoteMachineId] === true,
            left: metrics.left,
            machineId: sourceData.remoteMachineId,
            pointerOffsetY: metrics.pointerOffsetY,
            title: machine.name,
            top: metrics.top,
            width: metrics.width,
          }
          : undefined,
      );
    } else {
      setRemoteMachineDragPreview(undefined);
    }
    sessionPointerDragStateRef.current =
      sourceData?.kind === "session"
        ? createSessionPointerDragState(sourceData, pointerDownSessionTarget, nativeEvent)
        : undefined;
    pinnedSessionDropTargetLogKeyRef.current = undefined;
    setGroupDropIndicator(undefined);
    setPinnedSessionDropIndicator(undefined);
    setProjectCollectionDropIndicator(undefined);
    setProjectUngroupDropIndicatorScopeId(undefined);
    setRemoteMachineDropIndicator(undefined);
    setSessionDropIndicator(undefined);
    if (
      pointerDownSessionTarget &&
      sessionsById[ pointerDownSessionTarget.sessionId ]?.isPinned === true &&
      !(
        sourceData?.kind === "session" &&
        sourceData.groupId === pointerDownSessionTarget.groupId &&
        sourceData.sessionId === pointerDownSessionTarget.sessionId
      )
    ) {
      postPinnedSessionReorderLog("dragStartSourceMismatch", {
        point: getClientPoint(nativeEvent),
        pointerDownSessionTarget,
        sourceData,
        sourceKind: sourceData?.kind,
        state: createPinnedSessionReorderDebugState(
          {
            groupId: pointerDownSessionTarget.groupId,
            kind: "session",
            sessionId: pointerDownSessionTarget.sessionId,
          },
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetData: getSidebarDropData(event.operation.target),
      });
    }
    if (sourceData?.kind === "session" && sessionsById[ sourceData.sessionId ]?.isPinned === true) {
      postPinnedSessionReorderLog("dragStart", {
        point: getClientPoint(nativeEvent),
        pointerDownSessionTarget,
        sourceData,
        state: createPinnedSessionReorderDebugState(
          sourceData,
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetData: getSidebarDropData(event.operation.target),
      });
    }
    postSidebarDebugLog("native.pane.reorder", "session.dragStart", {
      nativeEventType: nativeEvent?.type,
      pointerDragState: sessionPointerDragStateRef.current,
      point: getClientPoint(nativeEvent),
      sourceData,
      targetData: getSidebarDropData(event.operation.target),
    });
  }) satisfies DragDropEventHandlers[ "onDragStart" ];

  const handleDragMove = ((event) => {
    const nativeEvent = getDragNativeEvent(event);
    updateGroupDragPreviewFromEvent(setGroupDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setProjectCollectionDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setRemoteMachineDragPreview, nativeEvent);
    updateSessionPointerDragState(sessionPointerDragStateRef.current, nativeEvent);
    updateSessionDropIndicator(event);
  }) satisfies DragDropEventHandlers[ "onDragMove" ];

  const handleDragOver = ((event) => {
    const nativeEvent = getDragNativeEvent(event);
    updateGroupDragPreviewFromEvent(setGroupDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setProjectCollectionDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setRemoteMachineDragPreview, nativeEvent);
    updateSessionPointerDragState(sessionPointerDragStateRef.current, nativeEvent);
    updateSessionDropIndicator(event);
  }) satisfies DragDropEventHandlers[ "onDragOver" ];

  const handleDragEnd = ((event) => {
    setSidebarTooltipsSuppressedForDrag(false);
    setGroupDropIndicator(undefined);
    setGroupDragPreview(undefined);
    setProjectCollectionDragPreview(undefined);
    setRemoteMachineDragPreview(undefined);
    setIsProjectReorderDragActive(false);
    setPinnedSessionDropIndicator(undefined);
    setProjectCollectionDropIndicator(undefined);
    setProjectUngroupDropIndicatorScopeId(undefined);
    setRemoteMachineDropIndicator(undefined);
    setSessionDropIndicator(undefined);
    const currentGroupIds = groupIdsRef.current;
    const currentSessionIdsByGroup = sessionIdsByGroupRef.current;
    const previousSessionIdsByGroup = effectiveSessionIdsByGroup;

    const nativeEvent = getDragNativeEvent(event);
    const sourceData = getSidebarDropData(event.operation.source);
    const targetData = getSidebarDropData(event.operation.target);
    const sessionPointerDragState = sessionPointerDragStateRef.current;
    updateSessionPointerDragState(sessionPointerDragState, nativeEvent);
    sessionPointerDragStateRef.current = undefined;
    const resolvedSessionDropTarget =
      sourceData?.kind === "session"
        ? resolveSessionDropTargetFromPoint(
          nativeEvent,
          currentSessionIdsByGroup,
          targetData,
          sourceData,
        )
        : undefined;
    postSidebarDebugLog("native.pane.reorder", "session.dragEnd", {
      canceled: event.canceled,
      nativeEventType: nativeEvent?.type,
      pointerDragState: sessionPointerDragState,
      point: getClientPoint(nativeEvent),
      resolvedSessionDropTarget,
      sourceData,
      targetData,
    });
    if (!sourceData) {
      return;
    }

    if (sourceData.kind === "project-collection") {
      setProjectCollectionDropIndicator(undefined);
      if (event.canceled) {
        return;
      }

      /*
       * A collection drag moves its complete visible project block between the
       * existing collection slots. Ungrouped projects keep their slots, child
       * project order stays intact, and the resulting flat project order is
       * persisted through the same sync contract as ordinary project drags.
       *
       * CDXC:CollectionReorder 2026-07-21:
       * Collections drag with feedback "none" (like project cards), so dnd-kit
       * never reports a rect-overlap target for them: the source shape stays at
       * its resting position for the whole drag. Resolve the insertion boundary
       * from the pointer position against the visible collection panels — the
       * same pattern project rows use via resolveGroupDropTargetFromPoint.
       */
      const collectionItems = displayedProjectCollectionItems.filter(
        (item): item is Extract<SidebarProjectCollectionRenderItem, { kind: "collection" }> =>
          item.kind === "collection",
      );
      const collectionIds = collectionItems.map((item) => item.collection.collectionId);
      const resolvedCollectionDropTarget = resolveProjectCollectionDropTargetFromPoint(
        nativeEvent,
        collectionIds,
        sourceData.collectionId,
        targetData,
      );
      if (!resolvedCollectionDropTarget) {
        return;
      }
      const nextCollectionIds = moveCollectionIdToDropTarget(
        collectionIds,
        sourceData.collectionId,
        resolvedCollectionDropTarget,
      );
      if (!nextCollectionIds) {
        return;
      }

      const collectionItemById = new Map(
        collectionItems.map((item) => [item.collection.collectionId, item]),
      );
      let nextCollectionIndex = 0;
      const nextRenderItems = displayedProjectCollectionItems.map((item) => {
        if (item.kind !== "collection") {
          return item;
        }
        const collectionId = nextCollectionIds[nextCollectionIndex];
        nextCollectionIndex += 1;
        return collectionId ? collectionItemById.get(collectionId) ?? item : item;
      });
      const nextGroupIds = nextRenderItems.flatMap((item) =>
        item.kind === "collection" ? item.groupIds : [item.groupId],
      );
      if (haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
        return;
      }

      const nextProjectIds = nextGroupIds.flatMap((groupId) => {
        const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
        return projectId ? [projectId] : [];
      });
      setProjectCollections((previous) =>
        reorderSidebarProjectCollections(
          reorderSidebarProjectCollectionDefinitions(previous, nextCollectionIds),
          nextProjectIds,
        ),
      );
      vscode.postMessage({
        groupIds: nextGroupIds,
        type: "syncGroupOrder",
      });
      return;
    }

    if (sourceData.kind === "remote-machine") {
      if (event.canceled) {
        return;
      }
      const resolvedRemoteMachineDropTarget = resolveRemoteMachineDropTargetFromPoint(
        nativeEvent,
        remoteMachines.map((machine) => machine.id),
        sourceData.remoteMachineId,
        targetData,
      );
      if (!resolvedRemoteMachineDropTarget) {
        return;
      }
      moveRemoteMachineSection(sourceData.remoteMachineId, resolvedRemoteMachineDropTarget);
      return;
    }

    if (sourceData.kind === "group") {
      if (event.canceled) {
        return;
      }

      /*
       * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
       * A grouped V2 row is a LOGICAL project: one header can stand for several
       * physical checkouts, on several machines. So the drop cannot be one
       * reordered id list — `syncGroupOrder` rejects a mixed local/remote or
       * cross-machine list, because each machine owns its own project order.
       *
       * Instead the row's new index among the LOGICAL rows is projected onto each
       * participating machine's own list, and one `syncGroupOrder` goes out per
       * machine that actually changed. The projection itself is pure and unit
       * tested (`packages/shared/sidebar-v2-group-order.ts`); everything DOM-dependent —
       * which boundary the pointer is over — stays in the shared resolver above,
       * so the committed reorder is the same boundary the drop line drew.
       *
       * This branch also bypasses V1's collection/ungroup handling below on
       * purpose: grouped V2 renders neither collections nor per-machine sections,
       * so there is no collection to drop out of and no machine list to leave.
       */
      if (isSidebarV2GroupedActive) {
        const rows = sidebarV2GroupOrderRowsRef.current;
        const resolvedTarget = resolveGroupDropTargetFromPoint(
          nativeEvent,
          rows.map((row) => row.groupId),
          groupsById,
          targetData,
          sourceData,
          sidebarV2GroupNoOpTargetForSource(sourceData.groupId),
        );
        if (!resolvedTarget) {
          return;
        }
        const projectedOrders = projectSidebarV2GroupOrderByMachine({
          groupIdsByMachineId: sidebarV2GroupIdsByMachineId,
          rows,
          sourceGroupId: sourceData.groupId,
          target: resolvedTarget,
        });
        for (const machineGroupIds of Object.values(projectedOrders)) {
          vscode.postMessage({
            groupIds: machineGroupIds,
            type: "syncGroupOrder",
          });
        }
        return;
      }

      /*
       * CDXC:RemoteGroupReorder 2026-07-12:
       * Remote machine groups reorder within their machine section and post the
       * machine-scoped id order through the same syncGroupOrder contract; the
       * host persists the per-machine order. Collections apply to local
       * projects only.
       */
      const remoteMachineId = groupsById[ sourceData.groupId ]?.remoteMachineContext?.machineId;
      if (remoteMachineId) {
        const machineGroupIds = remoteProjectGroupIdsByMachineId[ remoteMachineId ] ?? [];
        const sourceProjectId =
          groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const resolvedUngroupDropScopeId = resolveProjectUngroupDropScopeFromPoint(
          nativeEvent,
          sourceData.groupId,
          groupsById,
        );
        if (
          sourceProjectId &&
          projectCollectionIdByProjectId.has(sourceProjectId) &&
          resolvedUngroupDropScopeId === createRemoteProjectListScopeId(remoteMachineId)
        ) {
          const nextMachineGroupIds = moveProjectGroupFamilyToEnd(
            machineGroupIds,
            sourceData.groupId,
            groupsById,
          );
          setProjectCollections((previous) =>
            moveProjectsToSidebarCollection(
              previous,
              getProjectCollectionFamilyProjectIds(
                sourceProjectId,
                machineGroupIds,
                groupsById,
              ),
              undefined,
            ),
          );
          if (!haveSameSessionOrder(machineGroupIds, nextMachineGroupIds)) {
            vscode.postMessage({
              groupIds: nextMachineGroupIds,
              type: "syncGroupOrder",
            });
          }
          return;
        }
        const resolvedRemoteDropTarget = resolveGroupDropTargetFromPoint(
          nativeEvent,
          machineGroupIds,
          groupsById,
          targetData,
          sourceData,
        );
        if (!resolvedRemoteDropTarget) {
          return;
        }
        const nextMachineGroupIds = moveGroupIdsByProjectDropTarget(
          machineGroupIds,
          sourceData.groupId,
          resolvedRemoteDropTarget,
          groupsById,
        );
        if (haveSameSessionOrder(machineGroupIds, nextMachineGroupIds)) {
          return;
        }
        vscode.postMessage({
          groupIds: nextMachineGroupIds,
          type: "syncGroupOrder",
        });
        return;
      }

      const sourceProjectId =
        groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
      const resolvedUngroupDropScopeId = resolveProjectUngroupDropScopeFromPoint(
        nativeEvent,
        sourceData.groupId,
        groupsById,
      );
      if (
        sourceProjectId &&
        projectCollectionIdByProjectId.has(sourceProjectId) &&
        resolvedUngroupDropScopeId === LOCAL_PROJECT_LIST_SCOPE_ID
      ) {
        const nextGroupIds = moveProjectGroupFamilyToEnd(
          currentGroupIds,
          sourceData.groupId,
          groupsById,
        );
        setProjectCollections((previous) =>
          moveProjectsToSidebarCollection(
            previous,
            getProjectCollectionFamilyProjectIds(
              sourceProjectId,
              currentGroupIds,
              groupsById,
            ),
            undefined,
          ),
        );
        if (!haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
          vscode.postMessage({
            groupIds: nextGroupIds,
            type: "syncGroupOrder",
          });
        }
        return;
      }
      const resolvedGroupDropTarget = resolveGroupDropTargetFromPoint(
        nativeEvent,
        currentGroupIds,
        groupsById,
        targetData,
        sourceData,
      );
      const isProjectGroupOrder =
        createProjectGroupOrderItems(currentGroupIds, groupsById).length === currentGroupIds.length;
      const nextGroupIds = resolvedGroupDropTarget
        ? moveGroupIdsByProjectDropTarget(
          currentGroupIds,
          sourceData.groupId,
          resolvedGroupDropTarget,
          groupsById,
        )
        : targetData?.kind === "group" && !isProjectGroupOrder
          ? move(currentGroupIds, event)
          : currentGroupIds;
      if (haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
        return;
      }

      if (enableProjectCollections && resolvedGroupDropTarget) {
        const sourceProjectId = groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const targetProjectId =
          groupsById[resolvedGroupDropTarget.groupId]?.projectContext?.editor.projectId;
        if (sourceProjectId && targetProjectId) {
          const targetCollectionId = projectCollectionIdByProjectId.get(targetProjectId);
          const sourceFamilyProjectIds = getProjectCollectionFamilyProjectIds(
            sourceProjectId,
            currentGroupIds,
            groupsById,
          );
          const nextProjectIds = nextGroupIds.flatMap((groupId) => {
            const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
            return projectId ? [projectId] : [];
          });
          setProjectCollections((previous) =>
            reorderSidebarProjectCollections(
              moveProjectsToSidebarCollection(
                previous,
                sourceFamilyProjectIds,
                targetCollectionId,
              ),
              nextProjectIds,
            ),
          );
        }
      }

      vscode.postMessage({
        groupIds: nextGroupIds,
        type: "syncGroupOrder",
      });
      return;
    }

    if (sourceData.kind !== "session") {
      return;
    }

    if (sessionPointerDragState?.startPoint && !sessionPointerDragState.didMove) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        postPinnedSessionReorderLog("dragEndIgnoredWithoutPointerMovement", {
          point: getClientPoint(nativeEvent),
          pointerDragState: sessionPointerDragState,
          sourceData,
        });
      }
      postSidebarDebugLog("native.pane.reorder", "session.dragEndIgnoredWithoutPointerMovement", {
        point: getClientPoint(nativeEvent),
        sourceData,
      });
      return;
    }

    if (event.canceled) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        postPinnedSessionReorderLog("dragEndCanceled", {
          point: getClientPoint(nativeEvent),
          sourceData,
          targetData,
        });
      }
      return;
    }

    if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
      const resolvedPinnedSessionDropTarget = resolvePinnedSessionDropTargetFromPoint(
        nativeEvent,
        sourceData,
        currentSessionIdsByGroup,
        sessionsById,
      );
      postPinnedSessionReorderLog("dragEndResolved", {
        point: getClientPoint(nativeEvent),
        resolution: createPinnedSessionDropResolutionDebugState(
          nativeEvent,
          sourceData,
          currentSessionIdsByGroup,
          sessionsById,
        ),
        resolvedPinnedSessionDropTarget,
        resolvedSessionDropTarget,
        sourceData,
        state: createPinnedSessionReorderDebugState(
          sourceData,
          currentSessionIdsByGroup,
          previousSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetData,
      });
      if (!resolvedPinnedSessionDropTarget) {
        postPinnedSessionReorderLog("dragEndSkipped", {
          reason: "noPinnedDropTarget",
          sourceData,
          targetData,
        });
        return;
      }

      const previousPinnedSessionIds = (previousSessionIdsByGroup[ sourceData.groupId ] ?? []).filter(
        (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
      );
      const nextPinnedSessionIds = movePinnedSessionIdsByDropTarget(
        previousPinnedSessionIds,
        sourceData.sessionId,
        resolvedPinnedSessionDropTarget,
      );
      if (
        haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds) ||
        !haveSameSessionSet(previousPinnedSessionIds, nextPinnedSessionIds)
      ) {
        postPinnedSessionReorderLog("dragEndSkipped", {
          nextPinnedSessionIds,
          previousPinnedSessionIds,
          reason: haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds)
            ? "samePinnedOrder"
            : "pinnedSetMismatch",
          resolvedPinnedSessionDropTarget,
          sourceData,
        });
        return;
      }

      /**
       * CDXC:PinnedSessions 2026-05-28-14:29:
       * Dropping a pinned project session must persist exactly the row slot
       * indicated during drag. Resolve pinned drops from pointer position
       * against the pinned partition, then save pinned rows first while leaving
       * non-pinned project sessions in their authoritative order.
       */
      const nextSessionIds = createPinnedFirstSessionOrder(
        (authoritativeSessionIdsByGroup[ sourceData.groupId ] ?? []).length > 0
          ? (authoritativeSessionIdsByGroup[ sourceData.groupId ] ?? [])
          : (previousSessionIdsByGroup[ sourceData.groupId ] ?? []),
        nextPinnedSessionIds,
        sessionsById,
      );
      vscode.postMessage({
        groupId: sourceData.groupId,
        sessionIds: nextSessionIds,
        type: "syncSessionOrder",
      });
      postPinnedSessionReorderLog("syncSessionOrderPosted", {
        nextPinnedSessionIds,
        nextSessionIds,
        previousPinnedSessionIds,
        resolvedPinnedSessionDropTarget,
        sourceData,
      });
      return;
    }

    if (resolvedSessionDropTarget === null) {
      return;
    }

    if (!targetData && resolvedSessionDropTarget === undefined) {
      return;
    }

    const nextSessionIdsByGroup =
      resolvedSessionDropTarget !== undefined
        ? moveSessionIdsByDropTarget(
          currentSessionIdsByGroup,
          sourceData.sessionId,
          resolvedSessionDropTarget,
        )
        : move(currentSessionIdsByGroup, event);
    const nextListedSessionIds = new Set(Object.values(nextSessionIdsByGroup).flat());
    const omittedSessionIds = Object.values(currentSessionIdsByGroup)
      .flat()
      .filter((sessionId) => !nextListedSessionIds.has(sessionId));
    postSidebarDebugLog("native.pane.reorder", "session.dragComputedOrder", {
      currentSessionIdsByGroup,
      nextSessionIdsByGroup,
      omittedSessionIds,
      resolvedSessionDropTarget,
      sourceData,
      targetData,
    });
    const previousGroupId = findSessionGroupId(previousSessionIdsByGroup, sourceData.sessionId);
    const nextGroupId = findSessionGroupId(nextSessionIdsByGroup, sourceData.sessionId);
    if (!previousGroupId || !nextGroupId) {
      return;
    }

    if (previousGroupId !== nextGroupId) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        /**
         * CDXC:PinnedSessions 2026-05-28-12:04:
         * Project pinned sessions are only reorderable inside their owning
         * project. A pinned drag that lands over another project must not turn
         * into a cross-project move just because pinned cards are draggable in
         * the reference sidebar.
         */
        return;
      }

      const targetIndex = nextSessionIdsByGroup[ nextGroupId ]?.indexOf(sourceData.sessionId);
      if (targetIndex == null || targetIndex < 0) {
        return;
      }

      vscode.postMessage({
        groupId: nextGroupId,
        sessionId: sourceData.sessionId,
        targetIndex,
        type: "moveSessionToGroup",
      });
      return;
    }

    if (!isManualActiveSessionsSort) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        const authoritativeSessionIds = authoritativeSessionIdsByGroup[ nextGroupId ] ?? [];
        const previousSessionIds = previousSessionIdsByGroup[ nextGroupId ] ?? [];
        const nextDisplaySessionIds = nextSessionIdsByGroup[ nextGroupId ] ?? [];
        const nextPinnedSessionIds = nextDisplaySessionIds.filter(
          (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
        );
        const previousPinnedSessionIds = previousSessionIds.filter(
          (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
        );
        if (
          !haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds) &&
          haveSameSessionSet(previousPinnedSessionIds, nextPinnedSessionIds)
        ) {
          /**
           * CDXC:PinnedSessions 2026-05-28-12:04:
           * Last-activity mode still needs pinned rows to be manually
           * rearrangeable within a project. Persist only the pinned partition
           * order, then keep non-pinned sessions in their authoritative order
           * so activity sorting remains display-only for the rest of the group.
           */
          vscode.postMessage({
            groupId: nextGroupId,
            sessionIds: createPinnedFirstSessionOrder(
              authoritativeSessionIds.length > 0 ? authoritativeSessionIds : previousSessionIds,
              nextPinnedSessionIds,
              sessionsById,
            ),
            type: "syncSessionOrder",
          });
        }
      }
      return;
    }

    const previousSessionIds = previousSessionIdsByGroup[ nextGroupId ] ?? [];
    const nextSessionIds = nextSessionIdsByGroup[ nextGroupId ] ?? [];
    if (haveSameSessionOrder(previousSessionIds, nextSessionIds)) {
      return;
    }

    vscode.postMessage({
      groupId: nextGroupId,
      sessionIds: nextSessionIds,
      type: "syncSessionOrder",
    });
  }) satisfies DragDropEventHandlers[ "onDragEnd" ];

  const openSidebarSettings = () => {
    setIsPinnedPromptsOpen(false);
    if (!settings) {
      vscode.postMessage({ type: "openSettings" });
      return;
    }
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({ modal: "settings", type: "open" });
  };

  const openHotkeys = () => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Cmd+. is advertised in the sidebar Settings dropdown after the menu moved out of the titlebar. Route it to the same full-window app-modal host as Settings and Command Palette, closing transient sidebar drawers first so the shortcut opens one focused Hotkeys surface.
     */
    setIsPinnedPromptsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({ modal: "hotkeys", type: "open" });
  };

  const openCommandPalette = () => {
    /**
     * CDXC:CommandPalette 2026-06-13-10:26:
     * Cmd+Shift+P should open the full-window app-modal command palette,
     * matching Settings instead of rendering a dialog inside the narrow
     * sidebar. Close transient sidebar drawers first so the centered palette is
     * the only active command surface.
     *
     * CDXC:CommandPalette 2026-06-13-22:18:
     * Ghostex Quick Access gives Commands and Sessions separate tabs.
     * This launcher opens Commands; Cmd+P routes to the Sessions modal
     * id instead of encoding a mode in this input query.
     *
   */
    setIsPinnedPromptsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openQuickAccess("commands");
  };

  const openKeepAwakePowerSettings = () => {
    setIsPinnedPromptsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({
      initialSearchQuery: "Keep awake",
      modal: "settings",
      type: "open",
    });
  };

  const startSidebarKeepAwake = (durationMinutes: KeepAwakeDurationMinutes) => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Keep Awake moved from the macOS titlebar into the sidebar shortcut row, but the titlebar host remains the caffeinate runtime owner. Optimistically reflect the chosen duration in sidebar UI while native forwards the command to the existing titlebar start path.
     */
    setSidebarKeepAwakeRuntime({ durationMinutes });
    vscode.postMessage({
      action: "start",
      durationMinutes,
      type: "runTitlebarKeepAwakeCommand",
    });
    window.setTimeout(() => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime() ?? { durationMinutes });
    }, 250);
  };

  const stopSidebarKeepAwake = () => {
    setSidebarKeepAwakeRuntime(undefined);
    vscode.postMessage({
      action: "stop",
      type: "runTitlebarKeepAwakeCommand",
    });
    window.setTimeout(() => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime());
    }, 250);
  };

  const closeSessionSearch = () => {
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
  };

  const closeTopmostSidebarOverlay = useEffectEvent(() => {
    if (gitCommitDraft) {
      closeGitCommitModal(gitCommitDraft.requestId);
      return true;
    }

    if (isDaemonSessionsOpen) {
      setIsDaemonSessionsOpen(false);
      return true;
    }

    if (isSettingsOpen) {
      setIsSettingsOpen(false);
      return true;
    }

    if (isPreviousSessionsOpen) {
      setIsPreviousSessionsOpen(false);
      return true;
    }

    if (isPinnedPromptsOpen) {
      setIsPinnedPromptsOpen(false);
      return true;
    }

    if (isScratchPadOpen) {
      setIsScratchPadOpen(false);
      return true;
    }

    if (isSessionSearchOpen) {
      closeSessionSearch();
      return true;
    }

    return false;
  });

  const restoreSearchedPreviousSession = (historyId: string) => {
    vscode.postMessage({
      historyId,
      type: "restorePreviousSession",
    });
    closeSessionSearch();
  };

  const deleteSearchedPreviousSession = (historyId: string) => {
    vscode.postMessage({
      historyId,
      type: "deletePreviousSession",
    });
  };

  /*
   * CDXC:SidebarV2 2026-07-29:
   * Search results are one feature, not a V1 feature: the Inbox sidebar filters
   * the live list exactly as V1 does, so it must also offer the closed sessions
   * that match. This group is self-contained (it posts its own restore/delete
   * messages), so both sidebars render the same element rather than V2 shipping
   * a second previous-sessions implementation.
   */
  const previousSessionsSearchGroup = isSessionSearchFiltering ? (
    <SidebarPreviousSessionsSearchGroup
      onDeletePreviousSession={deleteSearchedPreviousSession}
      onRestorePreviousSession={restoreSearchedPreviousSession}
      previousSessions={filteredPreviousSessions}
      selectedHistoryId={
        isSessionSearchSelectionVisible && selectedSessionSearchResult?.kind === "previous"
          ? selectedSessionSearchResult.historyId
          : undefined
      }
      showDebugSessionNumbers={debuggingMode}
    />
  ) : null;

  const activateSelectedSessionSearchResult = useEffectEvent(() => {
    if (!selectedSessionSearchResult) {
      return false;
    }

    if (selectedSessionSearchResult.kind === "previous") {
      restoreSearchedPreviousSession(selectedSessionSearchResult.historyId);
      return true;
    }

    const selectedResult = sidebarSessionSearchResults.find((result) =>
      isSidebarSessionSearchSelectionMatch(result, selectedSessionSearchResult),
    );
    if (!selectedResult || selectedResult.kind !== "session") {
      return false;
    }

    dismissAppModalForSidebarNavigation("SettingsDismissal:sessionSearchActivate");
    useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    applyLocalFocus(selectedResult.groupId, selectedResult.sessionId);
    vscode.postMessage({
      sessionId: selectedResult.sessionId,
      type: "focusSession",
    });
    return true;
  });

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      const searchInput = searchInputRef.current;
      const isSearchInputTarget = searchInput !== null && target === searchInput;

      if (event.key === "Escape") {
        if (isSearchInputTarget && sessionSearchQuery.length > 0) {
          event.preventDefault();
          event.stopPropagation();
          setSessionSearchQuery("");
          searchInput.focus();
          return;
        }
        if (!closeTopmostSidebarOverlay()) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        return;
      }

      const commandPaletteHotkeyActionId = getCommandPaletteHotkeyActionId(
        event,
        settings?.hotkeys,
      );
      if (commandPaletteHotkeyActionId && !hasActiveSidebarHotkeyRecorder()) {
        event.preventDefault();
        event.stopPropagation();
        if (commandPaletteHotkeyActionId === "openCommandPalette") {
          openCommandPalette();
        } else {
          openPreviousSessions();
        }
        return;
      }

      if (
        event.defaultPrevented ||
        gitCommitDraft !== undefined ||
        isDaemonSessionsOpen ||
        isPreviousSessionsOpen ||
        isScratchPadOpen ||
        (isEditableSidebarKeyboardTarget(target) && !isSearchInputTarget)
      ) {
        return;
      }

      if (
        isSessionSearchOpen &&
        isSidebarSessionSearchNavigationKey(event) &&
        (isSearchInputTarget || !isEditableSidebarKeyboardTarget(target))
      ) {
        const nextSelection = getNextSidebarSessionSearchSelection({
          currentSelection: selectedSessionSearchResult,
          direction: getSidebarSessionSearchNavigationDirection(event),
          results: sidebarSessionSearchResults,
        });
        if (!nextSelection) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        setSelectedSessionSearchResult(nextSelection);
        setIsSessionSearchSelectionVisible(true);
        return;
      }

      if (
        isSessionSearchOpen &&
        event.key === "Enter" &&
        (isSearchInputTarget || !isEditableSidebarKeyboardTarget(target))
      ) {
        if (!activateSelectedSessionSearchResult()) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        setIsSessionSearchSelectionVisible(false);
        return;
      }

      if (isSearchInputTarget) {
        return;
      }

      /*
       * CDXC:SidebarKeyboard 2026-05-26-15:29:
       * Ordinary typing while focus is on sidebar chrome should not open or edit session search.
       * Leave non-editable sidebar keypresses unhandled so the host can provide its default invalid-key feedback instead of capturing the user's text in the sidebar.
       */
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [
    activateSelectedSessionSearchResult,
    closeTopmostSidebarOverlay,
    gitCommitDraft,
    isDaemonSessionsOpen,
    isPreviousSessionsOpen,
    isScratchPadOpen,
    isSessionSearchOpen,
    selectedSessionSearchResult,
    sessionSearchQuery,
    sidebarSessionSearchResults,
  ]);

  const setActiveSessionsSortMode = (sortMode: SidebarActiveSessionsSortMode) => {
    vscode.postMessage({
      manualSessionIdsByGroup:
        sortMode === "manual" && activeSessionsSortMode !== "manual"
          ? Object.fromEntries(
            workspaceGroupIds.map((groupId) => [
              groupId,
              [ ...(effectiveSessionIdsByGroup[ groupId ] ?? []) ],
            ]),
          )
          : undefined,
      sortMode,
      type: "setActiveSessionsSortMode",
    });
  };

  /*
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar version and its Group by Project sub-mode are persisted settings,
   * not sidebar-local view state. Write them through the same settings patch
   * channel the Settings modal uses so gpui persists them and hydrates every
   * surface back, instead of the sort-mode channel that gpui does not handle.
   */
  const updateSidebarVersionSettings = (patch: {
    sidebarV2Layout?: SidebarV2Layout;
    sidebarVersion?: SidebarVersion;
  }) => {
    vscode.postMessage({
      baseRevision: revision,
      patch,
      source: "sidebar:sidebarVersion",
      type: "updateSettingsPatch",
    });
  };

  const setSidebarVersion = (nextSidebarVersion: SidebarVersion) => {
    if (nextSidebarVersion === sidebarVersion) {
      return;
    }
    updateSidebarVersionSettings({ sidebarVersion: nextSidebarVersion });
  };

  const setSidebarV2Layout = (nextLayout: SidebarV2Layout) => {
    if (nextLayout === sidebarV2Layout) {
      return;
    }
    updateSidebarVersionSettings({ sidebarV2Layout: nextLayout });
  };

  const toggleActiveSessionsSortMode = () => {
    setActiveSessionsSortMode(
      activeSessionsSortMode === "manual" ? "lastActivity" : "manual",
    );
  };

  const toggleSessionTagFilter = (sessionTag: SidebarSessionTagFilter) => {
    if (!enabledVisibleSidebarSessionTagSet.has(sessionTag)) {
      return;
    }
    setSelectedSessionTagFilters((current) =>
      current.includes(sessionTag)
        ? current.filter((tag) => tag !== sessionTag)
        : [ ...current, sessionTag ],
    );
  };

  const moveSidebar = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:moveSidebar");
    vscode.postMessage({ type: "moveSidebarToOtherSide" });
  };

  const toggleSidebarCollapsed = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:toggleSidebar");
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Sidebar collapse is native chrome state. React requests the toggle, while
     * AppKit owns hiding the sidebar WebView, divider, and workspace border.
     */
    vscode.postMessage({ type: "toggleSidebarCollapsed" });
  };

  /*
   * CDXC:AddProject 2026-07-30:
   * Add Project opens the shared add-project dialog in the app-modal host for
   * every entry point. The local header sends no machine (the dialog resolves
   * the machine list itself and skips its machine step when there is only one),
   * while a remote machine header preselects its own machine so the flow can
   * never silently browse this computer's filesystem instead of that machine's.
   */
  const openAddProjectModal = (machineId?: string) => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:addProject");
    openAppModal({ ...(machineId ? { machineId } : {}), modal: "addProject", type: "open" });
  };

  const createReferenceAgentChat = (agent: SidebarAgentButton) => {
    const quickGroupId = displayedReferenceChatGroupIds[ 0 ];
    if (!quickGroupId) {
      return;
    }

    dismissAppModalForSidebarNavigation("SettingsDismissal:createQuickAgent");
    /**
     * CDXC:QuickAgents 2026-06-08-18:25:
     * The Quick section header should expose the same selected-agent split picker as project headers. Launch through runSidebarAgent with the synthetic Quick group id so native creates a new projectless agent chat instead of targeting the active code project.
     */
    setPrimaryAgentLauncherId(agent.agentId);
    writePrimaryAgentLauncherId(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId: quickGroupId,
      type: "runSidebarAgent",
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Sidebar V2's "+" launches through THIS function so the instant path stays
   * byte-identical to the classic sidebar's: a project click posts the same
   * `runSidebarAgent` the project header posts.
   *
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * V2 no longer reaches the `!groupId` branch from any ordinary create path.
   * Its header "+" and its agent picker both resolve a REAL project first (V2's
   * own `headerCreateGroupId`: scoped project, then active project, then the
   * first project), so a session can no longer silently land in Quick just
   * because the click came from the header rather than from a project row.
   *
   * The branch stays because `groupId` is genuinely optional in one case: a
   * workspace with ZERO project groups, where V2's resolution has nothing to
   * return. Quick is then the only place a session can go, so falling through to
   * the Quick launcher is the correct answer rather than a downgrade. Quick
   * creation ON PURPOSE happens through the chevron's explicitly-labelled
   * "Quick Terminal" / "Quick Browser Tab" items, which never come here.
   */
  const runSidebarV2Agent = (agent: SidebarAgentButton, groupId?: string) => {
    if (!groupId) {
      createReferenceAgentChat(agent);
      return;
    }
    dismissAppModalForSidebarNavigation("SettingsDismissal:createSidebarV2Agent");
    setPrimaryAgentLauncherId(agent.agentId);
    writePrimaryAgentLauncherId(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId,
      type: "runSidebarAgent",
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The "default new sessions to worktree" preference is GLOBAL and rides the
   * same settings patch channel as the sidebar version switch, so gpui persists
   * it and every surface hydrates it back.
   */
  const setNewSessionsDefaultEnvMode = (mode: SidebarNewSessionEnvMode) => {
    if (mode === effectiveSettings.newSessionsDefaultEnvMode) {
      return;
    }
    vscode.postMessage({
      baseRevision: revision,
      patch: { newSessionsDefaultEnvMode: mode },
      source: "sidebar:newSessionsDefaultEnvMode",
      type: "updateSettingsPatch",
    });
  };

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Cross-machine grouping overrides ride the SAME settings patch channel as
   * the sidebar version switch, under their own source so a grouping change can
   * never be mistaken for a remote-machine-capable save. V2 hands over the
   * whole record, so the patch is a straight replacement rather than a merge
   * the settings pipeline would have to interpret.
   */
  const setSidebarProjectGroupingOverrides = (
    overrides: Readonly<Record<string, SidebarProjectGroupingMode>>,
  ) => {
    vscode.postMessage({
      baseRevision: revision,
      patch: { sidebarProjectGroupingOverrides: overrides },
      source: "sidebar:projectGrouping",
      type: "updateSettingsPatch",
    });
  };

  const openConfigureAgentsModal = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:configureAgents");
    openAppModal({ modal: "configureAgents", type: "open" });
  };

  const openReferenceAutomations = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:automations");
    vscode.postMessage({ type: "openAutomationsPage" });
  };

  const openReferenceMobile = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:mobile");
    vscode.postMessage({ type: "openMobileBrowserChat" });
  };

  const openReferenceAgentsHub = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:agentsHub");
    openAppModal({ modal: "agentsHub", type: "open" });
  };

  const togglePinnedPrompts = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:pinnedPrompts");
    setIsDaemonSessionsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({ modal: "pinnedPrompts", type: "open" });
  };

  const openPreviousSessions = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:previousSessions");
    setIsPinnedPromptsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openQuickAccess("recentSessions");
  };

  const searchPreviousSessionsByPrompt = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:previousSessionsPromptSearch");
    setIsPinnedPromptsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    vscode.postMessage({ type: "searchPreviousSessionsByText" });
  };

  const renderReferenceProjectGroup = (groupId: string) => {
    const projectId = groupsById[ groupId ]?.projectContext?.editor.projectId;
    return (
      <SessionGroupSection
        autoEdit={autoEditingGroupId === groupId}
        allowPinnedSessionReorder={!isManualActiveSessionsSort}
        canClose={effectiveGroupIds.length > 1}
        completionFlashNonceBySessionId={completionFlashNonceBySessionId}
        draggingDisabled={isSessionSearchOpen}
        enableProjectSessionListToggle={!isSessionSearchFiltering}
        groupDropIndicator={groupDropIndicator}
        groupId={groupId}
        index={displayedReferenceProjectGroupIds.indexOf(groupId)}
        isCollapsed={isSidebarSearchProjectGroupRenderedCollapsed(groupId)}
        isHidden={hiddenGroupIds.includes(groupId)}
        isGroupDragPreviewSource={groupDragPreview?.groupId === groupId}
        key={groupId}
        onAutoEditHandled={() => setAutoEditingGroupId(undefined)}
        onCollapsedChange={setGroupCollapsed}
        onCreateProjectCollection={enableProjectCollections ? createProjectCollectionForProject : undefined}
        onFocusRequested={focusSidebarSessionFromNavigation}
        onMoveProjectToCollection={enableProjectCollections ? moveProjectToCollection : undefined}
        onProjectSessionListCollapsedChange={setProjectSessionListCollapsed}
        onHideGroup={() => setHiddenGroupIds((current) => current.includes(groupId) ? current.filter((id) => id !== groupId) : [...current, groupId])}
        onSessionSelectionChange={handleSidebarSessionSelectionChange}
        orderedSessionIds={displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []}
        pinnedSessionDropIndicator={pinnedSessionDropIndicator}
        projectCollectionId={projectId ? projectCollectionIdByProjectId.get(projectId) : undefined}
        projectCollectionOptions={enableProjectCollections ? projectCollections.collections : undefined}
        projectSessionListCollapsedState={collapsedProjectSessionListsById}
        revealSessionRequest={sessionRevealRequest}
        selectedSearchSessionId={
          isSessionSearchSelectionVisible && selectedSessionSearchResult?.kind === "session"
            ? selectedSessionSearchResult.sessionId
            : undefined
        }
        selectedSessionIds={selectedSidebarSessionIds}
        sessionDraggingDisabled={!isManualActiveSessionsSort}
        sessionDropIndicator={sessionDropIndicator}
        sessionTagListItems={sidebarSessionTagListItems}
        showHeaderActions={true}
        showSessionDropPositionIndicators={true}
        useColoredAgentIcons={effectiveSettings.useColoredSessionAgentIcons}
        vscode={vscode}
      />
    );
  };

  return (
    <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
      <SidebarCollapseAnimationProvider
        durationMs={effectiveSettings.sidebarCollapseAnimationDurationMs}
      >
      <div
        className="sidebar-reference-layout"
        data-project-reorder-drag={String(isProjectReorderDragActive)}
        data-project-group-style={effectiveSettings.sidebarProjectGroupStyle}
        data-reference-sidebar="true"
        data-session-agent-icon-color-mode={
          effectiveSettings.useColoredSessionAgentIcons ? "colored" : "monochrome"
        }
        ref={setReferenceLayoutElement}
        style={{
          "--sidebar-collapse-duration": `${effectiveSettings.sidebarCollapseAnimationDurationMs}ms`,
        } as CSSProperties}
      >
        {showCommandHotkeyOverlay ? <SidebarHotkeyOverlay hotkeys={settings?.hotkeys} /> : null}
        <SidebarReferenceTopChrome
          keepAwakeRuntime={sidebarKeepAwakeRuntime}
          onOpenAgentsHub={openReferenceAgentsHub}
          onOpenAutomations={openReferenceAutomations}
          onOpenDiscord={() => {
            vscode.postMessage({ type: "openExternalUrl", url: GHOSTEX_DISCORD_URL });
          }}
          onOpenHotkeys={openHotkeys}
          onOpenMobile={openReferenceMobile}
          onOpenPowerSettings={openKeepAwakePowerSettings}
          onOpenPreviousSessions={openPreviousSessions}
          onRunKeepAwake={startSidebarKeepAwake}
          onSearchPreviousSessionsByPrompt={searchPreviousSessionsByPrompt}
          onSearch={openPreviousSessions}
          onStopKeepAwake={stopSidebarKeepAwake}
          onTogglePetOverlay={() => {
            vscode.postMessage({ type: "togglePetOverlay" });
          }}
          settings={effectiveSettings}
          showKeepAwakeButton={showSidebarKeepAwakeButton}
        />
        <div
          className="stack"
          data-dimmed={String(isStartupInteractionBlocked)}
          data-sidebar-custom-theme={String(Boolean(normalizeWorkspaceThemeColor(customThemeColor)))}
          data-sidebar-theme={theme}
          onClickCapture={handleSidebarClickCapture}
          onDoubleClick={handleSidebarDoubleClick}
        >
          <section className="session-groups-panel" ref={sessionGroupsPanelRef}>
            <div className="session-groups-top">
              {null}
            </div>
            {/*
            CDXC:SidebarScroll 2026-06-30-01:59:
            The sidebar's project list must scroll as fast as the browser can move it.
            Do not apply the vertical scroll mask or sticky-header gradient geometry here; the user explicitly accepts losing those visual fades to remove scroll-linked paint work.
          */}
            <div
              className="session-groups-scroll-shell"
              data-scrollable-y={String(sessionGroupsHaveScrollableOverflow)}
            >
              <div
                className="session-groups-content"
                data-scrollable-y={String(sessionGroupsHaveScrollableOverflow)}
                ref={sessionGroupsContentRef}
              >
                {/*
                CDXC:SidebarSessions 2026-05-17-00:11:
                Opening or closing one session must not remount every sidebar
                project. Keep DragDropProvider stable so sortable/droppable hooks
                update the dnd registry without forcing all project rows to
                replay their entrance animation.

                CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                ONE provider now wraps BOTH sidebar bodies. Grouped V2 reorders
                projects through the same dnd-kit sortables, the same pointer drop
                resolution, and the same `syncGroupOrder` contract as V1, so a
                second provider would mean a second dnd manager, a second sensor
                set, and two registries that disagree about what is being dragged.
                It is deliberately mounted OUTSIDE the version switch so switching
                sidebars does not unmount and remount the manager mid-session.
              */}
                <DragDropProvider
                  onDragEnd={handleDragEnd}
                  onDragMove={handleDragMove}
                  onDragOver={handleDragOver}
                  onDragStart={handleDragStart}
                  plugins={(plugins) => plugins.filter((plugin) => plugin !== Cursor)}
                  sensors={sensors}
                >
                {/*
                 * CDXC:SidebarV2 2026-07-29:
                 * Sidebar V2 replaces only the session list body. Top chrome,
                 * search, and every host message path stay shared, and the V1
                 * tree below is untouched so the default sidebar renders
                 * exactly as before.
                 */}
                {isSidebarV2Active ? (
                  /*
                   * CDXC:SidebarV2 2026-07-29:
                   * The Inbox header IS this shared section header, not a
                   * parallel V2 copy. It already owns the pieces V2 needs —
                   * Sort & Filter (tag filters, Group by Project, the way back
                   * to the classic sidebar) and search — and every one of them
                   * posts the same host messages in both sidebars. V2 adds only
                   * the project scope dropdown, which lives in its own toolbar
                   * because it filters the inbox rather than acting on it.
                   *
                   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
                   * The creation cluster is the one piece V2 does NOT take from
                   * here. V2 owns a single split "+" in its own toolbar, so this
                   * mount deliberately withholds `onCreateBrowserChat`,
                   * `onCreateChat`, `onRunAgent`, and `onConfigureAgents` (plus
                   * the agent inputs that only feed that split button): every one
                   * of those props is optional, so their buttons simply do not
                   * render, and the header is left with Sort & Filter. The V1
                   * mount below still passes all of them, unchanged. Both Quick
                   * creators moved into V2's chevron menu as explicitly-labelled
                   * items, and "Configure agents" stays reachable through the
                   * command palette.
                   */
                  <SidebarReferenceSectionHeader
                    activeSessionsSortMode={activeSessionsSortMode}
                    actionsAlwaysVisible={true}
                    collapsed={isReferenceProjectsRenderedCollapsed}
                    containsActiveSession={[...groupIdsContainingActiveSession].some(
                      (groupId) => groupsById[groupId]?.isChatCollection !== true,
                    )}
                    onFilterChats={openPreviousSessions}
                    onSetActiveSessionsSortMode={setActiveSessionsSortMode}
                    onSetSidebarV2Layout={setSidebarV2Layout}
                    onSetSidebarVersion={setSidebarVersion}
                    onToggleSessionTagFilter={toggleSessionTagFilter}
                    onToggleCollapsed={() => {
                      setIsReferenceProjectsCollapsed((previous) => !previous);
                    }}
                    sectionKey="projects"
                    selectedSessionTagFilters={activeSelectedSessionTagFilters}
                    sessionSummary={sidebarV2SectionSessionSummary}
                    sessionTagListItems={sidebarSessionTagListItems}
                    sidebarV2Layout={sidebarV2Layout}
                    sidebarVersion={sidebarVersion}
                    title="Sessions"
                  />
                ) : null}
                {isSidebarV2Active && projectsSectionPresence.isPresent ? (
                  <div
                    aria-hidden={projectsSectionPresence.isVisuallyCollapsed}
                    className="sidebar-v2-section-body sidebar-animated-collapse-body"
                    data-collapsed={String(projectsSectionPresence.isVisuallyCollapsed)}
                    inert={projectsSectionPresence.isVisuallyCollapsed ? true : undefined}
                    ref={projectsSectionPresence.setCollapsibleElement}
                  >
                    <SidebarV2Root
                      /*
                       * CDXC:SidebarV2Worktree 2026-07-29:
                       * V2's creation control launches through SidebarApp's own
                       * agent path, so it needs the same two inputs the shared
                       * header uses: the configured agents and the last-used
                       * agent id. No new launch logic lives in V2.
                       */
                      agents={agents}
                      /*
                       * CDXC:SidebarV2LogicalProjects 2026-07-29:
                       * Each daemon's OWN auto-settle window, so a remote
                       * machine's sessions are partitioned with that machine's
                       * setting instead of this Mac's.
                       */
                      autoSettleAfterDays={sidebarAutoSettleAfterDays}
                      autoSettleAfterDaysByMachineId={sidebarAutoSettleAfterDaysByMachineId}
                      collapsedGroupsById={collapsedGroupsById}
                      /*
                       * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                       * Project reorder state, from the SAME dnd pipeline V1
                       * uses: the resolved insertion boundary, which row is
                       * currently being dragged, and V1's manual-sort gate. The
                       * ghost that follows the cursor is rendered by SidebarApp,
                       * so V2 only needs to paint the source placeholder and the
                       * drop line.
                       */
                      draggingGroupId={groupDragPreview?.groupId}
                      groupDropIndicator={groupDropIndicator}
                      groupIds={sidebarV2DisplayedGroupIds}
                      isGroupReorderDisabled={!isManualActiveSessionsSort}
                      isPinnedSessionReorderDisabled={isSessionSearchOpen}
                      groupsById={groupsById}
                      hostEmptyState={sidebarV2HostEmptyState}
                      isSearchFiltering={isSessionSearchFiltering}
                      layout={sidebarV2Layout}
                      /*
                       * CDXC:SidebarV2Lifecycle 2026-07-29:
                       * Settle/snooze support travels on the HUD because it is
                       * a property of the daemons behind the rows, not of the
                       * rows themselves — the local gxserver plus one entry per
                       * connected remote machine. Hosts that publish neither
                       * leave both undefined and V2 hides every lifecycle
                       * affordance.
                       */
                      lifecycleCapabilities={sidebarLifecycleCapabilities}
                      lifecycleCapabilitiesByMachineId={sidebarLifecycleCapabilitiesByMachineId}
                      /*
                       * CDXC:SidebarV2Worktree 2026-07-29:
                       * The worktree popover reads host answers off the SAME
                       * source SidebarApp listens on. gpui hands the sidebar a
                       * private message source, so passing `window` here would
                       * work in Storybook and silently never fire in the app.
                       */
                      messageSource={messageSource}
                      /*
                       * CDXC:AddProject 2026-07-30:
                       * V2 hides the classic Projects section header, so its
                       * create menu carries the same Add Project entry point
                       * the shared header exposes.
                       */
                      onAddProject={() => openAddProjectModal()}
                      onGroupedRowsChange={setSidebarV2GroupOrderRows}
                      onSetGroupCollapsed={setGroupCollapsed}
                      onSetNewSessionsDefaultEnvMode={setNewSessionsDefaultEnvMode}
                      onSetProjectGroupingOverrides={setSidebarProjectGroupingOverrides}
                      onSetSidebarVersion={setSidebarVersion}
                      pinnedSessionDropIndicator={pinnedSessionDropIndicator}
                      onSetLayout={setSidebarV2Layout}
                      onRunAgent={runSidebarV2Agent}
                      primaryAgentId={primaryAgentLauncherId}
                      searchQuery={sessionSearchQuery}
                      selectedSessionTagFilters={activeSelectedSessionTagFilters}
                      sessionIdsByGroup={displayedWorkspaceSessionIdsByGroup}
                      sessionsById={sessionsById}
                      settings={effectiveSettings}
                      vscode={vscode}
                    />
                    {previousSessionsSearchGroup}
                  </div>
                ) : null}
                {isSidebarV2Active ? null : (
                <>
                  {!shouldHideReferenceSectionsForSearchEmptyState ? (
                    <SidebarReferenceSectionHeader
                      activeSessionsSortMode={activeSessionsSortMode}
                      actionsAlwaysVisible={displayedReferenceProjectGroupIds.length === 0}
                      bulkActionLabel={
                        displayedReferenceProjectGroupIds.length > 0
                          ? hasExpandedReferenceProjects
                            ? "Collapse All"
                            : "Expand Previous"
                          : undefined
                      }
                      collapsed={isReferenceProjectsRenderedCollapsed}
                      containsActiveSession={[...groupIdsContainingActiveSession].some(
                        (groupId) =>
                          groupsById[groupId]?.isChatCollection !== true &&
                          groupsById[groupId]?.remoteMachineContext === undefined,
                      )}
                      onAddProject={() => openAddProjectModal()}
                      onToggleShowHidden={() => setShowHiddenSidebarItems((current) => !current)}
                      showHidden={showHiddenSidebarItems}
                      onBulkProjectToggle={
                        displayedReferenceProjectGroupIds.length > 0
                          ? () => {
                            postSidebarCollapseStateLog("projectBulkCommand", {
                              expandedProjectGroupCount:
                                displayedReferenceProjectGroupIds.length -
                                Object.keys(collapsedGroupsById).filter((groupId) =>
                                  displayedReferenceProjectGroupIds.includes(groupId),
                                ).length,
                              mode: hasExpandedReferenceProjects
                                ? "collapse-all"
                                : "expand-previous",
                              previousExpandedGroupCount:
                                previousExpandedReferenceProjectGroupIdsRef.current.length,
                              projectGroupCount: displayedReferenceProjectGroupIds.length,
                            });
                            if (isReferenceProjectsCollapsed && !hasExpandedReferenceProjects) {
                              triggerReferenceSectionChildAnimation("projects");
                            }
                            setIsReferenceProjectsCollapsed(false);
                            if (hasExpandedReferenceProjects) {
                              previousExpandedReferenceProjectGroupIdsRef.current =
                                displayedReferenceProjectGroupIds.filter(
                                  (groupId) => collapsedGroupsById[ groupId ] !== true,
                                );
                              setGroupsCollapsed(displayedReferenceProjectGroupIds, true);
                              return;
                            }

                            const previousExpandedProjectGroupIds =
                              previousExpandedReferenceProjectGroupIdsRef.current.filter(
                                (groupId) => displayedReferenceProjectGroupIds.includes(groupId),
                              );
                            setGroupsCollapsed(
                              previousExpandedProjectGroupIds.length > 0
                                ? previousExpandedProjectGroupIds
                                : displayedReferenceProjectGroupIds,
                              false,
                            );
                          }
                          : undefined
                      }
                      onSetActiveSessionsSortMode={setActiveSessionsSortMode}
                      onSetSidebarV2Layout={setSidebarV2Layout}
                      onSetSidebarVersion={setSidebarVersion}
                      onToggleSessionTagFilter={toggleSessionTagFilter}
                      onToggleCollapsed={() => {
                        const nextCollapsed = !isReferenceProjectsCollapsed;
                        postSidebarCollapseStateLog("sectionToggle", {
                          childGroupCount: displayedReferenceProjectGroupIds.length,
                          collapsed: nextCollapsed,
                          section: "projects",
                        });
                        if (isReferenceProjectsCollapsed) {
                          triggerReferenceSectionChildAnimation("projects");
                        }
                        setIsReferenceProjectsCollapsed((previous) => !previous);
                      }}
                      sectionKey="projects"
                      selectedSessionTagFilters={activeSelectedSessionTagFilters}
                      sessionSummary={referenceProjectsSectionSessionSummary}
                      sessionTagListItems={sidebarSessionTagListItems}
                      sidebarV2Layout={sidebarV2Layout}
                      sidebarVersion={sidebarVersion}
                      title="Projects"
                    />
                  ) : null}
                  {!shouldHideReferenceSectionsForSearchEmptyState &&
                  projectsSectionPresence.isPresent ? (
                    <div
                      aria-hidden={projectsSectionPresence.isVisuallyCollapsed}
                      className="group-list workspace-group-list reference-project-group-list reference-sidebar-collapsible-body"
                      data-animate-children={String(referenceSectionChildAnimations.projects)}
                      data-collapsed={String(projectsSectionPresence.isVisuallyCollapsed)}
                      inert={projectsSectionPresence.isVisuallyCollapsed ? true : undefined}
                      ref={projectsSectionPresence.setCollapsibleElement}
                      data-sidebar-project-list-scope={LOCAL_PROJECT_LIST_SCOPE_ID}
                    >
                      {displayedReferenceProjectGroupIds.length > 0 ? (
                        <>
                          {displayedProjectCollectionItems.map((item, itemIndex) =>
                            item.kind === "project" ? (
                              renderReferenceProjectGroup(item.groupId)
                            ) : (
                              <ProjectCollectionSection
                              autoEdit={autoEditingProjectCollectionId === item.collection.collectionId}
                              bulkProjectActionLabel={
                                item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                )
                                  ? "Collapse All"
                                  : "Expand Previous"
                              }
                              collapsed={
                                !isSessionSearchFiltering &&
                                collapsedProjectCollectionsByKey[
                                  createLocalProjectCollectionCollapseKey(
                                    item.collection.collectionId,
                                  )
                                ] === true
                              }
                              collection={item.collection}
                              containsActiveSession={item.groupIds.some((groupId) =>
                                groupIdsContainingActiveSession.has(groupId),
                              )}
                              draggingDisabled={isSessionSearchOpen}
                              dropIndicatorPosition={
                                projectCollectionDropIndicator?.collectionId ===
                                  item.collection.collectionId
                                  ? projectCollectionDropIndicator.position
                                  : undefined
                              }
                              index={itemIndex}
                              isDragPreviewSource={
                                projectCollectionDragPreview?.collectionId ===
                                item.collection.collectionId
                              }
                              isHidden={hiddenCollectionKeys.includes(
                                `local:${item.collection.collectionId}`,
                              )}
                              key={item.collection.collectionId}
                              onAutoEditHandled={() => setAutoEditingProjectCollectionId(undefined)}
                              onBulkProjectToggle={() => {
                                const hasExpandedProjects = item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                );
                                if (hasExpandedProjects) {
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    item.collection.collectionId
                                  ] = item.groupIds.filter(
                                    (groupId) => collapsedGroupsById[groupId] !== true,
                                  );
                                  setGroupsCollapsed(item.groupIds, true);
                                  return;
                                }

                                const previousExpandedProjectGroupIds =
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    item.collection.collectionId
                                  ]?.filter((groupId) => item.groupIds.includes(groupId)) ?? [];
                                setGroupsCollapsed(
                                  previousExpandedProjectGroupIds.length > 0
                                    ? previousExpandedProjectGroupIds
                                    : item.groupIds,
                                  false,
                                );
                              }}
                              onChange={(updated) => {
                                setProjectCollections((previous) =>
                                  updateSidebarProjectCollection(
                                    previous,
                                    updated.collectionId,
                                    (existing) => ({
                                      ...existing,
                                      color: updated.color,
                                      title: updated.title,
                                    }),
                                  ),
                                );
                              }}
                              onCollapsedChange={(collapsed) =>
                                setProjectCollectionCollapsed(
                                  createLocalProjectCollectionCollapseKey(
                                    item.collection.collectionId,
                                  ),
                                  collapsed,
                                )
                              }
                              onDelete={() => {
                                setProjectCollections((previous) =>
                                  removeSidebarProjectCollection(
                                    previous,
                                    item.collection.collectionId,
                                  ),
                                );
                              }}
                              onHide={() => {
                                const collectionKey = `local:${item.collection.collectionId}`;
                                setHiddenCollectionKeys((current) =>
                                  current.includes(collectionKey)
                                    ? current.filter((key) => key !== collectionKey)
                                    : [...current, collectionKey],
                                );
                              }}
                              onSelectSessions={setSelectedSidebarSessionIds}
                              sessionIds={item.groupIds.flatMap(
                                (groupId) => effectiveSessionIdsByGroup[ groupId ] ?? [],
                              )}
                              sessionTagListItems={sidebarSessionTagListItems}
                              sessionsById={sessionsById}
                              vscode={vscode}
                            >
                              {item.groupIds.map(renderReferenceProjectGroup)}
                              </ProjectCollectionSection>
                            ),
                          )}
                          <ProjectListEndUngroupDropZone
                            active={
                              projectUngroupDropIndicatorScopeId ===
                              LOCAL_PROJECT_LIST_SCOPE_ID
                            }
                            scopeId={LOCAL_PROJECT_LIST_SCOPE_ID}
                          />
                        </>
                      ) : (
                        referenceProjectsEmptyState
                      )}
                    </div>
                  ) : null}
                  {!shouldHideReferenceSectionsForSearchEmptyState && remoteMachines.length > 0 ? (
                    <div className="reference-remote-section-list">
                      {/*
	                     * CDXC:RemoteMachines 2026-06-02-23:47:
	                     * Saved Remote machines render as peer sidebar sections beside local Projects. Until the SSH/gxserver connection is active, each machine remains visible and exposes Reload instead of Add Project.
	                     *
	                     * CDXC:RemoteMachines 2026-06-09-19:02:
	                     * Remote machine section rows must collapse like Quick and Projects and use the same section-header styling, including the visible chevron and hover actions.
	                     */}
                      {remoteMachines.map((machine, index) => {
                        /*
                         * CDXC:RemoteProjectCollections 2026-07-21:
                         * Remote machine sections render the same collection
                         * panels as local Projects. Assigning a remote project
                         * to a group previously updated state with no visible
                         * result because remote lists were always flat.
                         */
                        const machineProjectGroupIds =
                          remoteProjectGroupIdsByMachineId[ machine.id ] ?? [];
                        const hasExpandedMachineProjects = machineProjectGroupIds.some(
                          (groupId) => collapsedGroupsById[groupId] !== true,
                        );
                        const machineProjectCollections =
                          remoteProjectCollectionsByMachineId[machine.id] ?? {
                            collections: [],
                            nextCollectionNumber: 1,
                          };
                        const machineCollectionIdByProjectId =
                          createProjectCollectionIdByProjectId(
                            machineProjectCollections,
                            machineProjectGroupIds,
                            groupsById,
                            (groupId) => groupsById[groupId]?.remoteMachineContext?.projectId,
                          );
                        const machineCollectionItems = enableProjectCollections
                          ? buildProjectCollectionRenderItems(
                              machineProjectGroupIds,
                              machineProjectCollections,
                              (groupId) =>
                                groupsById[groupId]?.remoteMachineContext?.projectId,
                            )
                          : undefined;
                        const renderRemoteProjectGroup = (groupId: string, groupIndex: number) => (
                          <SessionGroupSection
                            autoEdit={false}
                            canClose={!groupsById[ groupId ]?.projectContext}
                            completionFlashNonceBySessionId={completionFlashNonceBySessionId}
                            draggingDisabled={isSessionSearchOpen}
                            groupDropIndicator={groupDropIndicator}
                            groupId={groupId}
                            index={groupIndex}
                            isCollapsed={isSidebarSearchProjectGroupRenderedCollapsed(groupId)}
                            isHidden={hiddenGroupIds.includes(groupId)}
                            isGroupDragPreviewSource={groupDragPreview?.groupId === groupId}
                            key={groupId}
                            onAutoEditHandled={() => undefined}
                            onCollapsedChange={setGroupCollapsed}
                            onCreateProjectCollection={
                              enableProjectCollections
                                ? (projectId) =>
                                    createRemoteProjectCollectionForProject(
                                      machine.id,
                                      projectId,
                                      machineProjectGroupIds,
                                    )
                                : undefined
                            }
                            onFocusRequested={() => undefined}
                            onMoveProjectToCollection={
                              enableProjectCollections
                                ? (projectId, collectionId) =>
                                    moveRemoteProjectToCollection(
                                      machine.id,
                                      projectId,
                                      collectionId,
                                      machineProjectGroupIds,
                                    )
                                : undefined
                            }
                            onProjectSessionListCollapsedChange={setProjectSessionListCollapsed}
                            onHideGroup={() =>
                              setHiddenGroupIds((current) =>
                                current.includes(groupId)
                                  ? current.filter((id) => id !== groupId)
                                  : [...current, groupId],
                              )
                            }
                            onSessionSelectionChange={handleSidebarSessionSelectionChange}
                            orderedSessionIds={displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []}
                            enableProjectSessionListToggle={!isSessionSearchFiltering}
                            projectHeaderActions="all"
                            projectSessionListCollapsedState={collapsedProjectSessionListsById}
                            projectCollectionId={
                              groupsById[groupId]?.remoteMachineContext?.projectId
                                ? machineCollectionIdByProjectId.get(
                                    groupsById[groupId]!.remoteMachineContext!.projectId!,
                                  )
                                : undefined
                            }
                            projectCollectionOptions={
                              enableProjectCollections
                                ? machineProjectCollections.collections
                                : undefined
                            }
                            sessionDraggingDisabled={true}
                            sessionTagListItems={sidebarSessionTagListItems}
                            selectedSessionIds={selectedSidebarSessionIds}
                            showHeaderActions={true}
                            showSessionDropPositionIndicators={false}
                            useColoredAgentIcons={effectiveSettings.useColoredSessionAgentIcons}
                            vscode={vscode}
                          />
                        );
                        return (
                        <RemoteMachineSidebarSection
                          activeSessionsSortMode={activeSessionsSortMode}
                          bulkActionLabel={
                            machineProjectGroupIds.length > 0
                              ? hasExpandedMachineProjects
                                ? "Collapse All"
                                : "Expand Previous"
                              : undefined
                          }
                          collapsed={
                            !isSessionSearchFiltering &&
                            collapsedRemoteMachineSectionsById[ machine.id ] === true
                          }
                          containsActiveSession={[...groupIdsContainingActiveSession].some(
                            (groupId) =>
                              groupsById[groupId]?.remoteMachineContext?.machineId === machine.id,
                          )}
                          index={index}
                          key={machine.id}
                          machine={machine}
                          isDragPreviewSource={
                            remoteMachineDragPreview?.machineId === machine.id
                          }
                          remoteMachineDropIndicatorPosition={
                            remoteMachineDropIndicator?.remoteMachineId === machine.id
                              ? remoteMachineDropIndicator.position
                              : undefined
                          }
                          onAddProject={() => openAddProjectModal(machine.id)}
                          onBulkProjectToggle={
                            machineProjectGroupIds.length > 0
                              ? () => {
                                  setRemoteMachineSectionCollapsed(machine.id, false);
                                  if (hasExpandedMachineProjects) {
                                    previousExpandedRemoteProjectGroupIdsByMachineIdRef.current[
                                      machine.id
                                    ] = machineProjectGroupIds.filter(
                                      (groupId) => collapsedGroupsById[groupId] !== true,
                                    );
                                    setGroupsCollapsed(machineProjectGroupIds, true);
                                    return;
                                  }
                                  const previousExpandedProjectGroupIds =
                                    previousExpandedRemoteProjectGroupIdsByMachineIdRef.current[
                                      machine.id
                                    ]?.filter((groupId) =>
                                      machineProjectGroupIds.includes(groupId),
                                    ) ?? [];
                                  setGroupsCollapsed(
                                    previousExpandedProjectGroupIds.length > 0
                                      ? previousExpandedProjectGroupIds
                                      : machineProjectGroupIds,
                                    false,
                                  );
                                }
                              : undefined
                          }
                          onEdit={() => {
                            dismissAppModalForSidebarNavigation("SettingsDismissal:remoteEditSettings");
                            openAppModal({
                              initialRemoteMachineId: machine.id,
                              initialTab: "remote",
                              modal: "settings",
                              type: "open",
                            });
                          }}
                          onReconnect={() => {
                            dismissAppModalForSidebarNavigation("SettingsDismissal:remoteReconnect");
                            vscode.postMessage({
                              remoteMachineId: machine.id,
                              type: "reconnectRemoteMachine",
                            });
                          }}
                          onSetActiveSessionsSortMode={setActiveSessionsSortMode}
                          onSetSidebarV2Layout={setSidebarV2Layout}
                          onSetSidebarVersion={setSidebarVersion}
                          onToggleSessionTagFilter={toggleSessionTagFilter}
                          projectCollectionItems={machineCollectionItems}
                          projectUngroupDropIndicatorScopeId={
                            projectUngroupDropIndicatorScopeId
                          }
                          projectGroupIds={machineProjectGroupIds}
                          renderProjectCollection={(item, itemIndex) => (
                            <ProjectCollectionSection
                              autoEdit={
                                autoEditingProjectCollectionId ===
                                `${machine.id}:${item.collection.collectionId}`
                              }
                              bulkProjectActionLabel={
                                item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                )
                                  ? "Collapse All"
                                  : "Expand Previous"
                              }
                              collapsed={
                                !isSessionSearchFiltering &&
                                collapsedProjectCollectionsByKey[
                                  createRemoteProjectCollectionCollapseKey(
                                    machine.id,
                                    item.collection.collectionId,
                                  )
                                ] === true
                              }
                              collection={item.collection}
                              containsActiveSession={item.groupIds.some((groupId) =>
                                groupIdsContainingActiveSession.has(groupId),
                              )}
                              draggingDisabled={true}
                              index={itemIndex}
                              isHidden={hiddenCollectionKeys.includes(
                                `remote:${machine.id}:${item.collection.collectionId}`,
                              )}
                              key={`${machine.id}:${item.collection.collectionId}`}
                              onAutoEditHandled={() => setAutoEditingProjectCollectionId(undefined)}
                              onBulkProjectToggle={() => {
                                const bulkToggleKey = `${machine.id}:${item.collection.collectionId}`;
                                const hasExpandedProjects = item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                );
                                if (hasExpandedProjects) {
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    bulkToggleKey
                                  ] = item.groupIds.filter(
                                    (groupId) => collapsedGroupsById[groupId] !== true,
                                  );
                                  setGroupsCollapsed(item.groupIds, true);
                                  return;
                                }

                                const previousExpandedProjectGroupIds =
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    bulkToggleKey
                                  ]?.filter((groupId) => item.groupIds.includes(groupId)) ?? [];
                                setGroupsCollapsed(
                                  previousExpandedProjectGroupIds.length > 0
                                    ? previousExpandedProjectGroupIds
                                    : item.groupIds,
                                  false,
                                );
                              }}
                              onChange={(updated) => {
                                updateRemoteProjectCollections(machine.id, (previous) =>
                                  updateSidebarProjectCollection(
                                    previous,
                                    updated.collectionId,
                                    (existing) => ({
                                      ...existing,
                                      color: updated.color,
                                      title: updated.title,
                                    }),
                                  ),
                                );
                              }}
                              onCollapsedChange={(collapsed) =>
                                setProjectCollectionCollapsed(
                                  createRemoteProjectCollectionCollapseKey(
                                    machine.id,
                                    item.collection.collectionId,
                                  ),
                                  collapsed,
                                )
                              }
                              onDelete={() => {
                                updateRemoteProjectCollections(machine.id, (previous) =>
                                  removeSidebarProjectCollection(
                                    previous,
                                    item.collection.collectionId,
                                  ),
                                );
                              }}
                              onHide={() => {
                                const collectionKey = `remote:${machine.id}:${item.collection.collectionId}`;
                                setHiddenCollectionKeys((current) =>
                                  current.includes(collectionKey)
                                    ? current.filter((key) => key !== collectionKey)
                                    : [...current, collectionKey],
                                );
                              }}
                              onSelectSessions={setSelectedSidebarSessionIds}
                              sessionIds={item.groupIds.flatMap(
                                (groupId) => effectiveSessionIdsByGroup[ groupId ] ?? [],
                              )}
                              sessionTagListItems={sidebarSessionTagListItems}
                              sessionsById={sessionsById}
                              sortableId={`remote-project-collection:${machine.id}:${item.collection.collectionId}`}
                              vscode={vscode}
                            >
                              {item.groupIds.map((groupId) =>
                                renderRemoteProjectGroup(
                                  groupId,
                                  machineProjectGroupIds.indexOf(groupId),
                                ),
                              )}
                            </ProjectCollectionSection>
                          )}
                          renderProjectGroup={renderRemoteProjectGroup}
                          selectedSessionTagFilters={activeSelectedSessionTagFilters}
                          sessionSummary={remoteSectionSessionSummariesByMachineId[machine.id]}
                          sessionTagListItems={sidebarSessionTagListItems}
                          sidebarV2Layout={sidebarV2Layout}
                          sidebarVersion={sidebarVersion}
                          onToggleCollapsed={() => {
                            const nextCollapsed =
                              collapsedRemoteMachineSectionsById[ machine.id ] !== true;
                            if (!nextCollapsed) {
                              triggerReferenceSectionChildAnimation("remote");
                            }
                            setRemoteMachineSectionCollapsed(machine.id, nextCollapsed);
                          }}
                          status={remoteMachineRuntimeStatuses[ machine.id ] ?? "disconnected"}
                          statusMessage={remoteMachineStatusMessages[ machine.id ]}
                        />
                        );
                      })}
                    </div>
                  ) : null}
                {previousSessionsSearchGroup}
                {shouldShowSessionSearchEmptyState ? (
                  <div
                    className="group-empty-drop-target session-search-empty-drop-target"
                    data-empty-space-blocking="true"
                  >
                    <div className="group-empty-state session-search-empty-state">
                      No current sessions or sessions to reopen match that search.
                    </div>
                  </div>
                ) : displayedWorkspaceGroupIds.every(
                  (groupId) => (displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []).length === 0,
                ) &&
                  !isSessionSearchOpen ? (
                  <div className="empty" data-empty-space-blocking="true"></div>
                ) : null}
                </>
                )}
                {/*
                  * CDXC:ProjectDragPreview 2026-07-02-21:10:
                  * The ghost must live inside the .sidebar-reference-layout
                  * scope, or the reference project-header title rules do not
                  * match and the ghost renders with the base uppercase
                  * section-label styling. The layout root is display:contents,
                  * so the fixed-position ghost still anchors to the viewport.
                  *
                  * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                  * Hoisted out of the V1 branch with the provider. Grouped V2
                  * project rows drag with `feedback: "none"` exactly as V1's do,
                  * so this cursor ghost is the ONLY thing that follows the
                  * pointer during a V2 project reorder — leaving it behind in the
                  * V1 branch would have made a V2 drag invisible.
                  */}
                {groupDragPreview && referenceLayoutElement
                  ? createPortal(
                    <ProjectGroupDragGhost preview={groupDragPreview} />,
                    referenceLayoutElement,
                  )
                  : null}
                {projectCollectionDragPreview && referenceLayoutElement
                  ? createPortal(
                    <ProjectCollectionDragGhost preview={projectCollectionDragPreview} />,
                    referenceLayoutElement,
                  )
                  : null}
                {remoteMachineDragPreview && referenceLayoutElement
                  ? createPortal(
                    <RemoteMachineDragGhost preview={remoteMachineDragPreview} />,
                    referenceLayoutElement,
                  )
                  : null}
                </DragDropProvider>
              </div>
            </div>
          </section>
          <GitCommitModal
            agents={agents}
            draft={
              gitCommitDraft ?? {
                confirmLabel: "Commit",
                description: "",
                changedFiles: [],
                requestId: "",
                showCommitMessage: true,
                suggestedBody: undefined,
                suggestedSubject: "",
              }
            }
            isOpen={gitCommitDraft !== undefined}
            fileDiffDraft={gitFileDiffDraft}
            onCancel={(requestId) => {
              closeGitCommitModal(requestId);
            }}
            onConfirm={(requestId, message, options) => {
              setGitCommitDraft(undefined);
              setGitFileDiffDraft(undefined);
              vscode.postMessage({
                agentId: options.agentId,
                commitOnNewRef: options.commitOnNewRef,
                deleteWorktreeAfter: options.deleteWorktreeAfter,
                filePaths: options.filePaths,
                message,
                requestId,
                type: "confirmSidebarGitCommit",
              });
            }}
            onDirectMerge={(requestId, message, options) => {
              setGitCommitDraft(undefined);
              setGitFileDiffDraft(undefined);
              vscode.postMessage({
                agentId: options.agentId,
                deleteWorktreeAfter: options.deleteWorktreeAfter,
                filePaths: options.filePaths,
                message,
                requestId,
                type: "confirmSidebarGitDirectMerge",
              });
            }}
            onMultipleCommits={(requestId, agentId) => {
              setGitCommitDraft(undefined);
              setGitFileDiffDraft(undefined);
              vscode.postMessage({ agentId, requestId, type: "runSidebarGitMultipleCommits" });
            }}
            onOpenFileDiff={(filePath, requestId) => {
              vscode.postMessage({ filePath, requestId, type: "openSidebarGitChangedFileDiff" });
            }}
            theme={theme}
          />
          {buildStamp ? (
            <AppTooltip content="Copy build stamp">
              <button
                aria-label={`Copy build stamp ${buildStamp}`}
                className="copy-cursor"
                onClick={() => {
                  void navigator.clipboard.writeText(buildStamp).catch(() => { });
                }}
                style={DEBUG_BUILD_STAMP_STYLE}
                type="button"
              >
                {buildStamp}
              </button>
            </AppTooltip>
          ) : null}
        </div>
        <SidebarReferenceFooter
          commandPaletteHotkey={formatSidebarMenuHotkeyLabel(
            normalizeghostexHotkeySettings(effectiveSettings.hotkeys).openCommandPalette,
          )}
          onOpenQuickAccess={openCommandPalette}
          onOpenSettings={openSidebarSettings}
        />
      </div>
      </SidebarCollapseAnimationProvider>
    </TooltipProvider>
  );
}





