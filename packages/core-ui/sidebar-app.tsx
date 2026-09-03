import { Cursor, KeyboardSensor, PointerSensor } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import {
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { createPortal } from 'react-dom';
import { useShallow } from 'zustand/react/shallow';
import {
  type ApplySidebarSpaceEditorResultMessage,
  type ExtensionToSidebarMessage,
  type SidebarPreviousSessionItem,
} from '../shared/session-grid-contract';
import { normalizeWorkspaceThemeColor } from '../shared/workspace-project-appearance';
import { playCompletionSound, prepareCompletionSoundPlayback } from './completion-sound-player';
import { GitCommitModal } from './git-commit-modal';
import { SidebarPreviousSessionsSearchGroup } from './sidebar-session-search-overlay';
import { readSidebarHiddenItems, writeSidebarHiddenItems } from './sidebar-hidden-items';
import { SidebarCollapseAnimationProvider } from './sidebar-collapse-animation';
import {
  createSidebarSessionSearchResults,
  createSidebarSessionSearchSelection,
  getNextSidebarSessionSearchSelection,
  isSidebarSessionSearchSelectionMatch,
  type SidebarSessionSearchSelection,
} from './sidebar-session-search';
import {
  createSidebarRefreshDebugInstanceId,
  postSidebarRefreshDebugLog,
  summarizeSidebarRefreshMessage,
} from './sidebar-refresh-debug-log';
import { hashSidebarCollapseDebugId } from './sidebar-collapse-state-debug';
import { postSidebarOrderReproLog } from './sidebar-order-repro-log';
import { getSidebarReorderActivationConstraints } from './sidebar-reorder-activation';
import { scrollElementIntoViewIfNeeded } from './scroll-into-view-if-needed';
import { resetSidebarStore, useSidebarStore } from './sidebar-store';
import { type SidebarGroupDropTarget, type SidebarSessionDropTarget } from './sidebar-dnd';
import { getAutoCollapseGroupIds, getSessionCountsByGroup, reconcileCollapsedGroupsById } from './group-collapse';
import { SessionGroupSection } from './session-group-section';
import { ProjectCollectionSection } from './project-collection-section';
import {
  areSidebarProjectCollectionsStatesEqual,
  createSidebarProjectCollection,
  moveProjectsToSidebarCollection,
  parseSidebarProjectCollectionsFromGxserver,
  readSidebarProjectCollections,
  removeSidebarProjectCollection,
  serializeSidebarProjectCollectionsForGxserver,
  updateSidebarProjectCollection,
  writeSidebarProjectCollections,
  type SidebarProjectCollectionsState,
} from './project-collections';
import {
  applySidebarSpaceEditorResult,
  areSidebarSpacesStatesEqual,
  getSidebarSpaceIdsContainingCollection,
  getSidebarSpaceIdsContainingProject,
  parseSidebarSpacesFromGxserver,
  reorderSidebarSpaces,
  serializeSidebarSpacesForGxserver,
  toggleSpaceCollectionMembership,
  toggleSpaceProjectMembership,
  type SidebarSpacesState,
} from './spaces';
import { SpaceFilterRow } from './space-filter-row';
import {
  createRemoteSidebarSpaceSectionKey,
  createSelectedSidebarSpaceVisibility,
  LOCAL_SIDEBAR_SPACE_SECTION_KEY,
  resolveSelectedSidebarSpace,
} from './sidebar-app/space-filtering';
import { SidebarTooltipDelayProvider } from './tooltip-delay';
import { AppTooltip, setSidebarTooltipsSuppressedForDrag, useDismissSidebarTooltipsOnScroll } from './app-tooltip';
import { useScrollGlowState } from './use-scroll-glow-state';
import type { WebviewApi } from './webview-api';
import { createDisplaySessionLayout } from '../shared/active-sessions-sort';
import { filterDefaultNamedSessionSearchItems, filterPreviousSessions } from './previous-session-search';
import { type SidebarSessionTagFilter } from './session-tag-ui';
import { getEnabledVisibleSidebarSessionTagFilters, normalizeSidebarSessionTagListItems } from '../shared/session-tags';
import { isEmptySidebarDoubleClick } from './empty-sidebar-double-click';
import { closeAppModal, openAppModal } from './app-modal-host-bridge';
import { getghostexHotkeyActionById, normalizeghostexHotkeySettings } from '../shared/ghostex-hotkeys';
import {
  DEFAULT_ghostex_SETTINGS,
  isDiagnosticLoggingScenarioEnabled,
  isRemoteMachineEnabledInSidebar,
  applyEnabledRemoteMachineOrder,
} from '../shared/ghostex-settings';
import { SIDEBAR_PROJECT_JUMP_EVENT, type SidebarProjectJumpEventDetail } from '../shared/sidebar-project-jump';
import { GHOSTEX_DISCORD_URL } from '../shared/sidebar-commands';
import {
  readRenderedSidebarSessionSlotIds,
  readRenderedSidebarSessionSlots,
  resolveAdjacentRenderedSidebarSessionSlotId,
  resolveRenderedSidebarSessionAdditiveSelection,
  resolveRenderedSidebarSessionRangeSelection,
  resolveVisibleSidebarSessionSlotId,
} from './sidebar-visible-session-slots';
import {
  PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT,
  readPrimaryAgentLauncherId,
  type PrimaryAgentLauncherChangedEvent,
} from './primary-agent-launcher';
import { type ProjectSessionListCollapsedState } from './project-session-list-toggle';
import { hasKnownSidebarProjectInventory } from './sidebar-project-empty-state';
import {
  createLocalProjectCollectionCollapseKey,
  createRemoteProjectCollectionCollapseKey,
  DEFAULT_SIDEBAR_WINDOW_SCOPE_ID,
  normalizeSidebarWindowScopeId,
  readSidebarKeepAwakeRuntime,
  readSidebarUiCollapseState,
  summarizeSidebarUiCollapseState,
  writeSidebarUiCollapseState,
} from './sidebar-app/collapse-state';
import {
  createPinnedSessionDomDebugState,
  createPinnedSessionReorderDebugState,
  createProjectCollectionIdByProjectId,
  findCreatedGroupId,
  getProjectCollectionFamilyProjectIds,
  getRemoteProjectCollectionFamilyProjectIds,
  LOCAL_PROJECT_LIST_SCOPE_ID,
  moveRemoteMachineIdToDropTarget,
  summarizePointerEventForPinnedReorder,
  summarizeSidebarWakeScrollGeometry,
  summarizeSidebarWakeScrollOrderState,
  summarizeSidebarWakeScrollRenderedSlots,
  type SidebarPointerDownSessionTarget,
  type SidebarProjectCollectionDropTarget,
  type SidebarRemoteMachineDropTarget,
  type SidebarSessionPointerDragState,
} from './sidebar-app/drag-drop-geometry';
import {
  ProjectCollectionDragGhost,
  ProjectGroupDragGhost,
  ProjectListEndUngroupDropZone,
  RemoteMachineDragGhost,
  type SidebarGroupDragPreview,
  type SidebarProjectCollectionDragPreview,
  type SidebarRemoteMachineDragPreview,
} from './sidebar-app/drag-ghosts';
import { SidebarHotkeyOverlay, useCommandHotkeyOverlay } from './sidebar-app/hotkey-overlay';
import {
  SidebarReferenceFooter,
  SidebarReferenceTopChrome,
  formatSidebarMenuHotkeyLabel,
  type SidebarReferenceProjectMenu,
} from './sidebar-app/reference-chrome';
import { SidebarMachineTabs, type SidebarMachineTabItem } from './sidebar-app/machine-tabs';
import {
  LOCAL_SIDEBAR_MACHINE_TAB_ID,
  readSidebarSelectedMachineTabId,
  writeSidebarSelectedMachineTabId,
} from './sidebar-app/machine-tab-selection';
import {
  RemoteMachineSidebarSection,
  remoteMachineBusyLabel,
  remoteMachineFailureLabel,
} from './sidebar-app/remote-machine-section';
import {
  countSidebarSessions,
  createDisplayedGroupIds,
  createDisplayedSessionIdsByGroup,
  createWorkspaceSessionIdsByGroup,
  findSessionGroupId,
  getCommandPaletteHotkeyActionId,
  getSidebarSectionSessionSummary,
  getSidebarSessionSearchNavigationDirection,
  getSidebarStartupElapsedMs,
  getSidebarStartupNow,
  hasActiveSidebarHotkeyRecorder,
  isEditableSidebarKeyboardTarget,
  isSidebarSessionSearchNavigationKey,
  postSidebarAgentIconBoundaryLog,
  summarizeSidebarAgentIconsFromGroups,
  summarizeSidebarAgentIconsFromStore,
} from './sidebar-app/session-ordering';
import { useSidebarCollapseActions } from './sidebar-app/collapse-actions';
import { SIDEBAR_STARTUP_REPRO_WINDOW_MS, useSidebarDiagnosticLogs } from './sidebar-app/diagnostic-logs';
import { useSidebarDragHandlers } from './sidebar-app/drag-handlers';
import {
  useSidebarDocumentChromeEffects,
  useSidebarHostMessageListeners,
  useSidebarStartupDiagnosticEffects,
  useSidebarStartupInteractionBlock,
  useSidebarTimeoutCleanup,
} from './sidebar-app/lifecycle-effects';
import { useSidebarOverlayActions } from './sidebar-app/overlay-actions';
import { useSidebarActions } from './sidebar-app/sidebar-actions';
import type {
  RemoteMachineRuntimeStatus,
  RemoteMachineRuntimeStatuses,
  RemoteMachineStatusMessages,
  SessionIdsByGroup,
  SidebarEventSource,
  SidebarProjectCollectionRenderItem,
  SidebarSectionSessionSummary,
} from './sidebar-app/types';

export type SidebarAppProps = {
  enableProjectCollections?: boolean;
  messageSource?: SidebarEventSource;
  nativeHostEventSource?: SidebarEventSource | null;
  onStartGxserver?: () => void;
  vscode: WebviewApi;
  windowScopeId?: string;
};

/**
 * CDXC:SidebarBrowserTabReveal 2026-08-18:
 * `requestId` is what makes a reveal one-shot: two consecutive middle-clicks on
 * the same link name the same session and must each scroll it back into view.
 */
type SidebarSessionRevealRequest = {
  requestId: number;
  sessionId: string;
};

const sensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
  KeyboardSensor,
];

const SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID = 'gxserver-unavailable';
const SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS = 20_000;
const MIN_SESSION_SEARCH_QUERY_LENGTH = 4;
const COMPLETION_FLASH_DURATION_MS = 3_000;
/**
 * The map every remote section sees while Spaces are disabled. It is a module
 * constant so the gate keeps a stable identity across renders and never
 * invalidates the memos that filter remote sections by Space.
 */
const EMPTY_REMOTE_SIDEBAR_SPACES_BY_MACHINE_ID: Record<string, SidebarSpacesState> = {};
const DEBUG_BUILD_STAMP_STYLE: CSSProperties = {
  position: 'fixed',
  right: '10px',
  bottom: '8px',
  zIndex: 20,
  padding: 0,
  border: 'none',
  background: 'transparent',
  color: 'var(--vscode-foreground)',
  fontFamily: 'var(--vscode-font-family)',
  fontSize: '10px',
  lineHeight: 1.2,
  fontVariantNumeric: 'tabular-nums',
  opacity: 0.72,
};

function readSidebarProjectJumpEventDetail(event: Event): SidebarProjectJumpEventDetail | undefined {
  const detail = (event as CustomEvent<unknown>).detail;
  if (!detail || typeof detail !== 'object') {
    return undefined;
  }
  const candidate = detail as Partial<SidebarProjectJumpEventDetail>;
  if (
    typeof candidate.groupId !== 'string' ||
    typeof candidate.projectId !== 'string' ||
    typeof candidate.expandCollapsedProject !== 'boolean' ||
    typeof candidate.showLessAfterExpand !== 'boolean' ||
    (candidate.revealFocusedSession !== undefined && typeof candidate.revealFocusedSession !== 'boolean')
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
  const [windowScopeId] = useState(() => normalizeSidebarWindowScopeId(rawWindowScopeId));
  const [initialUiCollapseStateRead] = useState(() => readSidebarUiCollapseState(windowScopeId));
  const initialUiCollapseState = initialUiCollapseStateRead.state;
  const [isStartupInteractionBlocked, setIsStartupInteractionBlocked] = useState(true);
  const [autoEditingGroupId, setAutoEditingGroupId] = useState<string>();
  const [agentCreateRequestId, setAgentCreateRequestId] = useState(0);
  const [isPreviousSessionsOpen, setIsPreviousSessionsOpen] = useState(false);
  const [isReferenceChatsCollapsed, setIsReferenceChatsCollapsed] = useState(
    initialUiCollapseState.isReferenceChatsCollapsed
  );
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isSessionSearchOpen, setIsSessionSearchOpen] = useState(false);
  const [initialHiddenItems] = useState(readSidebarHiddenItems);
  const [hiddenGroupIds, setHiddenGroupIds] = useState(initialHiddenItems.groupIds);
  const [hiddenCollectionKeys, setHiddenCollectionKeys] = useState(initialHiddenItems.collectionKeys);
  const [showHiddenSidebarItems, setShowHiddenSidebarItems] = useState(false);
  const showCommandHotkeyOverlay = useCommandHotkeyOverlay();
  const [completionFlashNonceBySessionId, setCompletionFlashNonceBySessionId] = useState<Record<string, number>>({});
  const [collapsedGroupsById, setCollapsedGroupsById] = useState<Record<string, true>>(
    initialUiCollapseState.collapsedGroupsById
  );
  const [collapsedProjectCollectionsByKey, setCollapsedProjectCollectionsByKey] = useState<Record<string, true>>(
    initialUiCollapseState.collapsedProjectCollectionsByKey
  );
  const [collapsedProjectSessionListsById, setCollapsedProjectSessionListsById] =
    useState<ProjectSessionListCollapsedState>(initialUiCollapseState.collapsedProjectSessionListsById);
  const [projectCollections, setProjectCollections] = useState<SidebarProjectCollectionsState>(
    enableProjectCollections ? readSidebarProjectCollections : { collections: [], nextCollectionNumber: 1 }
  );
  const [remoteProjectCollectionsByMachineId, setRemoteProjectCollectionsByMachineId] = useState<
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
  /*
  CDXC:SidebarSpaces 2026-08-27:
  Spaces are entirely gxserver-owned — there is no localStorage overlay to seed
  from — so `undefined` here means "this daemon has never delivered a Space
  state", which is the capability signal: a section with no state renders no
  Space row and performs no Space filtering at all. An empty (but delivered)
  state is a capable daemon with zero Spaces, which still shows the row so New
  Space stays reachable.

  Desktop and web deliver the local state differently: the desktop host sends it
  only through `sidebarSpacesChanged`, while the web host also carries it on
  hydrate. Both paths land in the same setter. The last-synced ref is the same
  echo guard the project-collection twins use, so a state that just arrived from
  the daemon is never posted straight back to it.
  */
  const [storedSpacesState, setSpacesState] = useState<SidebarSpacesState | undefined>(undefined);
  const [storedRemoteSpacesByMachineId, setRemoteSpacesByMachineId] = useState<Record<string, SidebarSpacesState>>({});
  const lastGxserverSyncedSpacesRef = useRef<SidebarSpacesState | undefined>(undefined);
  const [selectedSpaceIdBySectionKey, setSelectedSpaceIdBySectionKey] = useState<Record<string, string>>(
    initialUiCollapseState.selectedSpaceIdBySectionKey
  );
  const [autoEditingProjectCollectionId, setAutoEditingProjectCollectionId] = useState<string>();
  /*
   * CDXC:SidebarMachineTabs 2026-08-28:
   * Machines are top-level tabs, so exactly one machine's projects are in the
   * sidebar body at a time. `local` is the reserved local tab and the default.
   */
  const [selectedMachineTabId, setSelectedMachineTabId] = useState(() =>
    readSidebarSelectedMachineTabId(windowScopeId)
  );
  const previousExpandedReferenceProjectGroupIdsRef = useRef<string[]>([]);
  const previousExpandedRemoteProjectGroupIdsByMachineIdRef = useRef<Record<string, string[]>>({});
  const previousExpandedProjectGroupIdsByCollectionIdRef = useRef<Record<string, string[]>>({});
  const [sessionSearchQuery, setSessionSearchQuery] = useState('');
  const [selectedSessionTagFilters, setSelectedSessionTagFilters] = useState<SidebarSessionTagFilter[]>([]);
  const [remoteSessionSearchPreviousSessions, setRemoteSessionSearchPreviousSessions] = useState<
    SidebarPreviousSessionItem[] | undefined
  >(undefined);
  const [groupDropIndicator, setGroupDropIndicator] = useState<SidebarGroupDropTarget>();
  const [projectCollectionDropIndicator, setProjectCollectionDropIndicator] =
    useState<SidebarProjectCollectionDropTarget>();
  const [remoteMachineDropIndicator, setRemoteMachineDropIndicator] = useState<SidebarRemoteMachineDropTarget>();
  const [projectUngroupDropIndicatorScopeId, setProjectUngroupDropIndicatorScopeId] = useState<string>();
  const [groupDragPreview, setGroupDragPreview] = useState<SidebarGroupDragPreview>();
  const [projectCollectionDragPreview, setProjectCollectionDragPreview] =
    useState<SidebarProjectCollectionDragPreview>();
  const [remoteMachineDragPreview, setRemoteMachineDragPreview] = useState<SidebarRemoteMachineDragPreview>();
  /*
   * CDXC:ProjectReorderScrollLock 2026-07-22:
   * While a project or collection header is being dragged, the per-project
   * session scrollers must not auto-scroll under the ghost. dnd-kit's Scroller
   * treats any computed overflow auto/scroll ancestor under the pointer as a
   * scroll target, so this flag flips those inner scrollers to overflow hidden
   * for the duration of the drag (the main sidebar scroller stays scrollable
   * to reach offscreen drop positions).
   */
  const [isProjectReorderDragActive, setIsProjectReorderDragActive] = useState(false);
  const [referenceLayoutElement, setReferenceLayoutElement] = useState<HTMLDivElement | null>(null);
  const [pinnedSessionDropIndicator, setPinnedSessionDropIndicator] = useState<SidebarSessionDropTarget>();
  const [sessionDropIndicator, setSessionDropIndicator] = useState<SidebarSessionDropTarget>();
  const [isSessionSearchSelectionVisible, setIsSessionSearchSelectionVisible] = useState(false);
  const [focusedSessionRevealRequestId, setFocusedSessionRevealRequestId] = useState(0);
  /**
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * The host's pending "make this row visible" request. It is kept in state
   * rather than handled inline because the row it names can arrive after the
   * request: gpui creates a Browser tab and asks for the reveal in the same
   * turn the tab is first published, so the reveal effect re-runs with the
   * displayed session list until the row exists.
   */
  const [sessionRevealRequest, setSessionRevealRequest] = useState<SidebarSessionRevealRequest>();
  const [showGxserverUnavailableEmptyState, setShowGxserverUnavailableEmptyState] = useState(false);
  const [selectedSessionSearchResult, setSelectedSessionSearchResult] = useState<SidebarSessionSearchSelection>();
  const [selectedSidebarSessionIds, setSelectedSidebarSessionIds] = useState<string[]>([]);
  const pendingCreateGroupRef = useRef(false);
  const didResetStoreRef = useRef(false);
  const sessionGroupsPanelRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const groupIdsRef = useRef<string[]>([]);
  const sessionIdsByGroupRef = useRef<SessionIdsByGroup>({});
  const pinnedSessionDropTargetLogKeyRef = useRef<string | undefined>(undefined);
  const previousSessionCountsByGroupRef = useRef<Record<string, number>>({});
  const latestSessionSearchPreviousRequestIdRef = useRef<string | undefined>(undefined);
  const didApplyStartupEmptyChatsCollapseRef = useRef(false);
  const hasEstablishedStartupGroupCollapseBaselineRef = useRef(false);
  const hasObservedAvailableGxserverStateRef = useRef(false);
  const previousNormalizedSessionSearchQueryRef = useRef('');
  const refreshDebugInstanceIdRef = useRef(createSidebarRefreshDebugInstanceId());
  const pointerDownSessionTargetRef = useRef<SidebarPointerDownSessionTarget | undefined>(undefined);
  const sessionPointerDragStateRef = useRef<SidebarSessionPointerDragState | undefined>(undefined);
  const completionFlashTimeoutBySessionIdRef = useRef<Map<string, number>>(new Map());
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
    if (areSidebarProjectCollectionsStatesEqual(lastGxserverSyncedProjectCollectionsRef.current, projectCollections)) {
      return;
    }
    lastGxserverSyncedProjectCollectionsRef.current = projectCollections;
    vscode.postMessage({
      state: serializeSidebarProjectCollectionsForGxserver(projectCollections),
      type: 'updateSidebarProjectCollections',
    });
  }, [enableProjectCollections, projectCollections, vscode]);

  useEffect(() => {
    /*
    CDXC:SidebarSpaces 2026-08-27:
    Local Space edits write through to the owning daemon so every client
    connected to it sees the same Spaces. States that just arrived from the
    daemon carry the same baseline, so they are skipped instead of echoing back.
    */
    const lastSyncedSpaces = lastGxserverSyncedSpacesRef.current;
    if (!storedSpacesState || !lastSyncedSpaces || areSidebarSpacesStatesEqual(lastSyncedSpaces, storedSpacesState)) {
      return;
    }
    lastGxserverSyncedSpacesRef.current = storedSpacesState;
    vscode.postMessage({
      state: serializeSidebarSpacesForGxserver(storedSpacesState),
      type: 'updateSidebarSpaces',
    });
  }, [storedSpacesState, vscode]);

  const applyLocalFocus = useSidebarStore((state) => state.applyLocalFocus);
  const consumeFocusedSessionScrollSuppression = useSidebarStore(
    (state) => state.consumeFocusedSessionScrollSuppression
  );
  const applyCommandRunStateClearedMessage = useSidebarStore((state) => state.applyCommandRunStateClearedMessage);
  const applyCommandRunStateMessage = useSidebarStore((state) => state.applyCommandRunStateMessage);
  const applyGroupsChangedMessage = useSidebarStore((state) => state.applyGroupsChangedMessage);
  const applyHudChangedMessage = useSidebarStore((state) => state.applyHudChangedMessage);
  const applyOrderSyncResultMessage = useSidebarStore((state) => state.applyOrderSyncResultMessage);
  const applySessionPresentationMessage = useSidebarStore((state) => state.applySessionPresentationMessage);
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
      previousSessions: state.previousSessions,
      projectSettingsProjects: state.hud.projectSettingsProjects,
      recentProjects: state.hud.recentProjects,
      revision: state.revision,
      settings: state.hud.settings,
      sessionsById: state.sessionsById,
      theme: state.hud.theme,
      workspaceGroupIds: state.workspaceGroupIds,
    }))
  );
  const gitCommitDraft = useSidebarStore((state) => state.gitCommitDraft);
  const gitFileDiffDraft = useSidebarStore((state) => state.gitFileDiffDraft);
  const authoritativeSessionIdsByGroup = useSidebarStore((state) => state.sessionIdsByGroup);
  const [remoteMachineRuntimeStatuses, setRemoteMachineRuntimeStatuses] = useState<RemoteMachineRuntimeStatuses>({});
  const [remoteMachineStatusMessages, setRemoteMachineStatusMessages] = useState<RemoteMachineStatusMessages>({});
  const [primaryAgentLauncherId, setPrimaryAgentLauncherId] = useState(readPrimaryAgentLauncherId);
  const [sidebarKeepAwakeRuntime, setSidebarKeepAwakeRuntime] = useState(readSidebarKeepAwakeRuntime);
  const buildStamp = useSidebarStore((state) => (state.hud.debuggingMode ? state.hud.buildStamp : undefined));
  const hasGxserverUnavailablePlaceholder = Boolean(groupsById[SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID]);
  const hasAvailableGxserverState = !hasGxserverUnavailablePlaceholder && groupOrder.length > 0;

  useEffect(() => {
    if (hasAvailableGxserverState) {
      hasObservedAvailableGxserverStateRef.current = true;
    }
  }, [hasAvailableGxserverState]);

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
  }, [hasGxserverUnavailablePlaceholder, onStartGxserver]);

  const effectiveSettings = settings ?? DEFAULT_ghostex_SETTINGS;
  /*
   * CDXC:SidebarSpaces 2026-08-28:
   * Spaces are opt-in. The daemon keeps owning the Space document either way —
   * the setting is a UI gate, so a disabled sidebar simply never sees a Space
   * state: no Space row, no Space filtering, no Spaces submenu, and no way to
   * reach the editor. Hiding the state here rather than at each consumer is what
   * makes the disabled sidebar byte-for-byte the pre-Spaces sidebar, and it
   * leaves the persisted per-section selection untouched so turning the setting
   * back on restores exactly the view the user left.
   */
  const spacesState = effectiveSettings.sidebarSpacesEnabled ? storedSpacesState : undefined;
  const remoteSpacesByMachineId = effectiveSettings.sidebarSpacesEnabled
    ? storedRemoteSpacesByMachineId
    : EMPTY_REMOTE_SIDEBAR_SPACES_BY_MACHINE_ID;
  const showSidebarKeepAwakeButton =
    effectiveSettings.showBetaFeatures && !effectiveSettings.hideKeepAwakeTitlebarControl;
  const sidebarRefreshDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
    effectiveSettings.diagnosticLogging,
    'native.sidebar.refresh'
  );
  const sidebarCollapseDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
    effectiveSettings.diagnosticLogging,
    'native.sidebar.collapse'
  );

  useEffect(() => {
    const refreshKeepAwakeRuntime = () => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime());
    };
    window.addEventListener('focus', refreshKeepAwakeRuntime);
    window.addEventListener('storage', refreshKeepAwakeRuntime);
    document.addEventListener('visibilitychange', refreshKeepAwakeRuntime);
    return () => {
      window.removeEventListener('focus', refreshKeepAwakeRuntime);
      window.removeEventListener('storage', refreshKeepAwakeRuntime);
      document.removeEventListener('visibilitychange', refreshKeepAwakeRuntime);
    };
  }, []);

  const sidebarSessionTagListItems = useMemo(
    () => normalizeSidebarSessionTagListItems(effectiveSettings.sidebarSessionTagListItems),
    [effectiveSettings.sidebarSessionTagListItems]
  );
  const enabledVisibleSidebarSessionTagSet = useMemo(
    () => new Set(getEnabledVisibleSidebarSessionTagFilters(sidebarSessionTagListItems)),
    [sidebarSessionTagListItems]
  );
  const activeSelectedSessionTagFilters = useMemo(
    () => selectedSessionTagFilters.filter((tag) => enabledVisibleSidebarSessionTagSet.has(tag)),
    [enabledVisibleSidebarSessionTagSet, selectedSessionTagFilters]
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
  }, [enabledVisibleSidebarSessionTagSet]);

  useEffect(() => {
    const refreshPrimaryAgentLauncher = (event: Event) => {
      const changedEvent = event as PrimaryAgentLauncherChangedEvent;
      setPrimaryAgentLauncherId(
        typeof changedEvent.detail?.agentId === 'string' ? changedEvent.detail.agentId : readPrimaryAgentLauncherId()
      );
    };

    window.addEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    return () => {
      window.removeEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    };
  }, []);

  const {
    postPinnedSessionReorderLog,
    postSidebarCollapseStateLog,
    postSidebarDebugLog,
    postSidebarRefreshLifecycleLog,
    postSidebarStartupReproLog,
  } = useSidebarDiagnosticLogs({
    debuggingMode,
    firstHydrateRevisionRef,
    hasAppliedHydrateRef,
    hasEstablishedStartupGroupCollapseBaselineRef,
    refreshDebugInstanceIdRef,
    revision,
    sidebarCollapseDiagnosticLoggingEnabled,
    sidebarStartupStartedAtRef,
    vscode,
  });

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
    const isEstablishingStartupGroupCollapseBaseline = !hasEstablishedStartupGroupCollapseBaselineRef.current;
    const hasGxserverUnavailablePlaceholder = groupOrder.includes(SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID);
    const visibleGroupIds = new Set(groupOrder);
    const unknownCollapsedGroupCount = Object.keys(collapsedGroupsById).filter(
      (groupId) => !visibleGroupIds.has(groupId)
    ).length;
    const preserveUnknownCollapsedGroups =
      isEstablishingStartupGroupCollapseBaseline && hasGxserverUnavailablePlaceholder;
    const sessionCountIncreaseGroupIds = isEstablishingStartupGroupCollapseBaseline
      ? []
      : groupOrder.filter((groupId) => {
          const previousCount = previousSessionCountsByGroupRef.current[groupId];
          return previousCount !== undefined && (authoritativeSessionIdsByGroup[groupId] ?? []).length > previousCount;
        });

    if (preserveUnknownCollapsedGroups && unknownCollapsedGroupCount > 0) {
      postSidebarCollapseStateLog('startupPartialHydratePreserved', {
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
      })
    );

    /**
     * CDXC:SidebarReference 2026-05-08-11:09
     * When creating a chat, terminal, browser pane, or agent session inside a
     * collapsed Combined sidebar area, expand the owning Chats section
     * as soon as the host hydrates the added session so the user sees the
     * result of the action. Projects is always expanded, so it needs no
     * equivalent auto-expand.
     * CDXC:SidebarReference 2026-05-20-12:00
     * Do not expand the Chats section header on the first post-hydrate
     * baseline pass after restart. Restored session counts are not new sessions.
     */
    if (sessionCountIncreaseGroupIds.some((groupId) => groupsById[groupId]?.isChatCollection)) {
      postSidebarCollapseStateLog('sectionAutoExpanded', {
        reason: 'session-count-increase',
        section: 'quick',
        sessionCountIncreaseGroupCount: sessionCountIncreaseGroupIds.length,
      });
      setIsReferenceChatsCollapsed(false);
    }

    previousSessionCountsByGroupRef.current = nextSessionCountsByGroup;
    if (isEstablishingStartupGroupCollapseBaseline && !hasGxserverUnavailablePlaceholder) {
      postSidebarCollapseStateLog('startupBaselineEstablished', {
        groupCount: groupOrder.length,
        sessionCount: Object.keys(sessionsById).length,
      });
      hasEstablishedStartupGroupCollapseBaselineRef.current = true;
    }
  }, [authoritativeSessionIdsByGroup, collapsedGroupsById, groupOrder, groupsById, sessionsById, workspaceGroupIds]);

  const isSidebarInteractionBlocked = isStartupInteractionBlocked;

  const { setGroupCollapsed, setGroupsCollapsed, setProjectCollectionCollapsed, setProjectSessionListCollapsed } =
    useSidebarCollapseActions({
      collapsedGroupsById,
      groupOrder,
      postSidebarCollapseStateLog,
      setCollapsedGroupsById,
      setCollapsedProjectCollectionsByKey,
      setCollapsedProjectSessionListsById,
    });

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
    dismissAppModalForSidebarNavigation('SettingsDismissal:focusSession');
    useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    applyLocalFocus(groupId, sessionId);
  };

  const requestNewSession = () => {
    if (isSidebarInteractionBlocked) {
      return;
    }

    dismissAppModalForSidebarNavigation('SettingsDismissal:createSession');
    vscode.postMessage({ type: 'createSession' });
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
    if (!target.closest('.session')) {
      return;
    }
    dismissAppModalForSidebarNavigation('SettingsDismissal:sessionClick');
  };

  const handleWindowMessage = useEffectEvent((event: MessageEvent<ExtensionToSidebarMessage>) => {
    if (!event.data) {
      return;
    }

    if (event.data.type === 'hydrate' || event.data.type === 'sessionState') {
      /*
      CDXC:SidebarSpaces 2026-08-27:
      Remote Space states arrive on hydrate and on every presentation patch, so
      the whole map is replaced whenever the field is present. The LOCAL state is
      asymmetric: the desktop host publishes it only through
      `sidebarSpacesChanged`, so an absent `sidebarSpaces` here means "not
      carried by this transport", never "the daemon has no Spaces", and must
      leave the capability signal untouched.
      */
      if (event.data.type === 'hydrate' || event.data.remoteSidebarSpacesByMachineId !== undefined) {
        const nextRemoteSpaces: Record<string, SidebarSpacesState> = {};
        for (const [machineId, state] of Object.entries(event.data.remoteSidebarSpacesByMachineId ?? {})) {
          const parsed = parseSidebarSpacesFromGxserver(state);
          if (parsed) {
            nextRemoteSpaces[machineId] = parsed;
          }
        }
        setRemoteSpacesByMachineId(nextRemoteSpaces);
      }
      const parsedLocalSpaces = parseSidebarSpacesFromGxserver(event.data.sidebarSpaces);
      if (parsedLocalSpaces) {
        lastGxserverSyncedSpacesRef.current = parsedLocalSpaces;
        setSpacesState((previous) =>
          previous && areSidebarSpacesStatesEqual(previous, parsedLocalSpaces) ? previous : parsedLocalSpaces
        );
      }
    }

    if ((event.data.type === 'hydrate' || event.data.type === 'sessionState') && enableProjectCollections) {
      if (event.data.type === 'hydrate' || event.data.remoteSidebarProjectCollectionsByMachineId !== undefined) {
        const nextRemoteCollections: Record<string, SidebarProjectCollectionsState> = {};
        for (const [machineId, state] of Object.entries(event.data.remoteSidebarProjectCollectionsByMachineId ?? {})) {
          const parsed = parseSidebarProjectCollectionsFromGxserver(state);
          if (parsed) {
            nextRemoteCollections[machineId] = parsed;
          }
        }
        setRemoteProjectCollectionsByMachineId(nextRemoteCollections);
      }

      const parsedLocalCollections = parseSidebarProjectCollectionsFromGxserver(event.data.sidebarProjectCollections);
      if (parsedLocalCollections) {
        if (parsedLocalCollections.collections.length === 0 && projectCollections.collections.length > 0) {
          lastGxserverSyncedProjectCollectionsRef.current = projectCollections;
          vscode.postMessage({
            state: serializeSidebarProjectCollectionsForGxserver(projectCollections),
            type: 'updateSidebarProjectCollections',
          });
        } else {
          const adopted: SidebarProjectCollectionsState = {
            collections: parsedLocalCollections.collections,
            nextCollectionNumber: Math.max(
              parsedLocalCollections.nextCollectionNumber,
              projectCollections.nextCollectionNumber
            ),
          };
          lastGxserverSyncedProjectCollectionsRef.current = adopted;
          if (!areSidebarProjectCollectionsStatesEqual(adopted, projectCollections)) {
            setProjectCollections(adopted);
          }
        }
      }
    }

    if (event.data.type === 'gpuiProjectSlotHotkey') {
      resolveGpuiProjectSlotHotkey(event.data.slotNumber);
      return;
    }

    if (event.data.type === 'nativeHotkey') {
      runGhostexHotkeyAction(event.data.actionId);
      return;
    }

    if (event.data.type === 'playCompletionSound') {
      const sessionId = event.data.sessionId;
      postSidebarDebugLog('native.agent.detection', 'completionSound.messageReceived', {
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
          [sessionId]: (previous[sessionId] ?? 0) + 1,
        }));
        const timeout = window.setTimeout(() => {
          completionFlashTimeoutBySessionIdRef.current.delete(sessionId);
          setCompletionFlashNonceBySessionId((previous) => {
            if (!(sessionId in previous)) {
              return previous;
            }

            const next = { ...previous };
            delete next[sessionId];
            return next;
          });
        }, COMPLETION_FLASH_DURATION_MS);
        completionFlashTimeoutBySessionIdRef.current.set(sessionId, timeout);
      }
      void playCompletionSound(event.data.sound, (soundEvent, details) => {
        postSidebarDebugLog('native.agent.detection', soundEvent, details);
      });
      return;
    }

    if (event.data.type === 'sessionPresentationChanged') {
      applySessionPresentationMessage(event.data);
      return;
    }

    if (event.data.type === 'sidebarGroupsChanged') {
      applyGroupsChangedMessage(event.data);
      return;
    }

    if (event.data.type === 'sidebarProjectCollectionsChanged') {
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
      const parsed = parseSidebarProjectCollectionsFromGxserver(event.data.sidebarProjectCollections);
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
            type: 'updateSidebarProjectCollections',
          });
        }
        return;
      }
      const adopted: SidebarProjectCollectionsState = {
        collections: parsed.collections,
        nextCollectionNumber: Math.max(parsed.nextCollectionNumber, projectCollections.nextCollectionNumber),
      };
      lastGxserverSyncedProjectCollectionsRef.current = adopted;
      if (!areSidebarProjectCollectionsStatesEqual(adopted, projectCollections)) {
        setProjectCollections(adopted);
      }
      return;
    }

    if (event.data.type === 'sidebarSpacesChanged') {
      /*
      CDXC:SidebarSpaces 2026-08-27:
      The daemon owns the whole Space document, so its copy is simply adopted —
      including an empty one, which is a real "this daemon has no Spaces" answer
      rather than a missing overlay to seed. Adopting also re-baselines the echo
      guard so the write-through effect does not post the state back.
      */
      const parsedSpaces = parseSidebarSpacesFromGxserver(event.data.sidebarSpaces);
      if (!parsedSpaces) {
        return;
      }
      const spacesRemoteMachineId = event.data.remoteMachineId;
      if (spacesRemoteMachineId) {
        setRemoteSpacesByMachineId((previous) => ({
          ...previous,
          [spacesRemoteMachineId]: parsedSpaces,
        }));
        return;
      }
      lastGxserverSyncedSpacesRef.current = parsedSpaces;
      setSpacesState((previous) =>
        previous && areSidebarSpacesStatesEqual(previous, parsedSpaces) ? previous : parsedSpaces
      );
      return;
    }

    if (event.data.type === 'applySidebarSpaceEditorResult') {
      /*
      CDXC:SidebarSpaces 2026-08-27:
      The New/Edit Space dialog's confirm/delete, bounced back from the host.
      The dialog carries field values only, so the edit lands on the CURRENT
      Space document here; the write-through effect (local) or
      `updateRemoteSpaces` (remote) then pushes it to the owning daemon.
      */
      applySpaceEditorResult(event.data);
      return;
    }

    if (event.data.type === 'sidebarHudChanged') {
      applyHudChangedMessage(event.data);
      return;
    }

    if (event.data.type === 'sidebarCommandRunStateChanged') {
      applyCommandRunStateMessage(event.data);
      return;
    }

    if (event.data.type === 'sidebarCommandRunStateCleared') {
      applyCommandRunStateClearedMessage(event.data);
      return;
    }

    if (event.data.type === 'revealSidebarSession') {
      setSessionRevealRequest({
        requestId: event.data.requestId,
        sessionId: event.data.sessionId,
      });
      return;
    }

    if (event.data.type === 'sidebarOrderSyncResult') {
      postSidebarOrderReproLog(vscode, 'repro.sidebarOrder.webview.syncResultReceived', {
        itemIds: event.data.itemIds,
        kind: event.data.kind,
        requestId: event.data.requestId,
        status: event.data.status,
      });
      applyOrderSyncResultMessage(event.data);
      return;
    }

    if (event.data.type === 'daemonSessionsState') {
      setDaemonSessionsState(event.data);
      return;
    }

    if (event.data.type === 'promptGitCommit') {
      setGitCommitDraft(event.data);
      return;
    }

    if (event.data.type === 'sidebarGitFileDiff') {
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

    if (event.data.type === 'previousSessionsResult') {
      if (event.data.requestId !== latestSessionSearchPreviousRequestIdRef.current) {
        return;
      }
      setRemoteSessionSearchPreviousSessions(event.data.previousSessions);
      return;
    }

    if (event.data.type === 'remoteMachineStatus') {
      const remoteMachineStatus = event.data as RemoteMachineRuntimeStatus;
      setRemoteMachineRuntimeStatuses((current) => ({
        ...current,
        [remoteMachineStatus.machineId]: remoteMachineStatus.state,
      }));
      setRemoteMachineStatusMessages((current) => {
        const message = remoteMachineStatus.message?.trim();
        if (message) {
          return { ...current, [remoteMachineStatus.machineId]: message };
        }
        if (current[remoteMachineStatus.machineId] === undefined) {
          return current;
        }
        const next = { ...current };
        delete next[remoteMachineStatus.machineId];
        return next;
      });
      return;
    }

    if (event.data.type === 'showSessionRenameModal') {
      dismissAppModalForSidebarNavigation('SettingsDismissal:renameSession');
      openAppModal({
        initialTitle: event.data.initialTitle,
        modal: 'renameSession',
        sessionAgentIcon: event.data.sessionAgentIcon,
        sessionId: event.data.sessionId,
        type: 'open',
      });
      return;
    }

    if (event.data.type !== 'hydrate' && event.data.type !== 'sessionState') {
      return;
    }

    postSidebarOrderReproLog(vscode, 'repro.sidebarOrder.webview.messageReceived', {
      agentIds: event.data.hud.agents.map((agent) => agent.agentId),
      commandIds: event.data.hud.commands.map((command) => command.commandId),
      groupCount: event.data.groups.length,
      groupIds: event.data.groups.map((group) => group.groupId),
      messageType: event.data.type,
      revision: event.data.revision,
    });
    postSidebarStartupReproLog('messageReceived', {
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
      'native.sidebar.refresh'
    );
    const messageSidebarCollapseDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
      messageSettings.diagnosticLogging,
      'native.sidebar.collapse'
    );
    postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, 'messageReceived', {
      ...summarizeSidebarRefreshMessage(event.data, revision),
      hasHydrateBeforeMessage: hasAppliedHydrateRef.current,
      instanceId: refreshDebugInstanceIdRef.current,
    });
    const sidebarCollapseMessageSessionCount = countSidebarSessions(event.data.groups);
    const sidebarCollapseMessageShape = [
      event.data.type,
      event.data.groups.length,
      sidebarCollapseMessageSessionCount,
      event.data.revision < revision ? 'stale' : 'fresh',
    ].join(':');
    const shouldLogSidebarCollapseHydrateMessage =
      messageSidebarCollapseDiagnosticLoggingEnabled &&
      getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current) <= SIDEBAR_STARTUP_REPRO_WINDOW_MS &&
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
        'messageReceived',
        {
          collapsedGroupCount: Object.keys(collapsedGroupsById).length,
          groupCount: event.data.groups.length,
          isReferenceChatsCollapsed,
          messageRevision: event.data.revision,
          messageType: event.data.type,
          sessionCount: sidebarCollapseMessageSessionCount,
          stale: event.data.revision < revision,
        },
        { enabled: true }
      );
    }
    if (messageSidebarRefreshDiagnosticLoggingEnabled && !didLogRefreshInstanceObservedRef.current) {
      didLogRefreshInstanceObservedRef.current = true;
      postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, 'appInstanceObserved', {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        instanceId: refreshDebugInstanceIdRef.current,
        messageType: event.data.type,
        revision: event.data.revision,
      });
    }
    if (event.data.type === 'sessionState' && !hasAppliedHydrateRef.current) {
      postSidebarStartupReproLog('sessionStateBeforeHydrate', {
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
    postSidebarAgentIconBoundaryLog(vscode, 'sidebar.agentIcon.messageReceived', {
      messageType: event.data.type,
      revision: event.data.revision,
      summary: summarizeSidebarAgentIconsFromGroups(event.data.groups),
    });

    if (pendingCreateGroupRef.current) {
      const nextGroupId = findCreatedGroupId(
        groupOrder,
        event.data.groups.map((group) => group.groupId)
      );
      if (nextGroupId) {
        setAutoEditingGroupId(nextGroupId);
        pendingCreateGroupRef.current = false;
      }
    }

    applySidebarMessage(event.data);
    postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, 'messageApplied', {
      ...summarizeSidebarRefreshMessage(event.data, revision),
      hasHydrateAfterApply: hasAppliedHydrateRef.current,
      instanceId: refreshDebugInstanceIdRef.current,
      storeRevisionAfterApply: useSidebarStore.getState().revision,
      storeSessionCountAfterApply: Object.keys(useSidebarStore.getState().sessionsById).length,
    });
    postSidebarAgentIconBoundaryLog(vscode, 'sidebar.agentIcon.messageApplied', {
      messageType: event.data.type,
      revision: event.data.revision,
      summary: summarizeSidebarAgentIconsFromStore(useSidebarStore.getState().sessionsById),
    });
    if (event.data.type === 'hydrate' && !hasAppliedHydrateRef.current) {
      hasAppliedHydrateRef.current = true;
      firstHydrateRevisionRef.current = event.data.revision;
    }
    if (shouldLogSidebarCollapseHydrateMessage) {
      postSidebarCollapseStateLog(
        'messageApplied',
        {
          collapsedGroupCount: Object.keys(collapsedGroupsById).length,
          groupCount: event.data.groups.length,
          isReferenceChatsCollapsed,
          messageRevision: event.data.revision,
          messageType: event.data.type,
          sessionCount: sidebarCollapseMessageSessionCount,
          storeCollapsedGroupCount: Object.keys(collapsedGroupsById).length,
          storeRevisionAfterApply: useSidebarStore.getState().revision,
        },
        { enabled: true }
      );
    }
    postSidebarStartupReproLog('messageApplied', {
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

  useSidebarStartupDiagnosticEffects({
    collapsedGroupsById,
    didLogInitialUiCollapseStateReadRef,
    firstHydrateRevisionRef,
    groupOrder,
    hasAppliedHydrateRef,
    initialUiCollapseStateRead,
    isStartupInteractionBlocked,
    lastSidebarStartupRenderStateKeyRef,
    postSidebarCollapseStateLog,
    postSidebarRefreshLifecycleLog,
    postSidebarStartupReproLog,
    refreshDebugInstanceIdRef,
    revision,
    sessionsById,
    sidebarCollapseDiagnosticLoggingEnabled,
    sidebarRefreshDiagnosticLoggingEnabled,
    sidebarStartupStartedAtRef,
    vscode,
    workspaceGroupIds,
  });

  useSidebarHostMessageListeners({
    handleWindowMessage,
    messageSource,
    nativeHostEventSource,
  });

  useSidebarTimeoutCleanup({ completionFlashTimeoutBySessionIdRef });

  useSidebarStartupInteractionBlock({
    postSidebarStartupReproLog,
    setIsStartupInteractionBlocked,
    sidebarStartupStartedAtRef,
  });

  useSidebarDocumentChromeEffects({
    agentManagerZoomPercent,
    customThemeColor,
    effectiveSettings,
    theme,
  });

  const closeGitCommitModal = useEffectEvent((requestId: string) => {
    setGitCommitDraft(undefined);
    setGitFileDiffDraft(undefined);
    vscode.postMessage({
      requestId,
      type: 'cancelSidebarGitCommit',
    });
  });

  useEffect(() => {
    if (!sessionGroupsPanelRef.current) {
      return;
    }

    sessionGroupsPanelRef.current.inert = isSidebarInteractionBlocked;
  }, [isSidebarInteractionBlocked]);

  const isManualActiveSessionsSort = activeSessionsSortMode === 'manual';
  /**
   * CDXC:SidebarLayout 2026-05-13-08:11
   * The reference sidebar replaces the old visible Actions/Agents grids with
   * app-modal entries, titlebar modes, and project header controls. Do not
   * mount the obsolete hidden panels in the sidebar tree.
   */
  const { groupIds: effectiveGroupIds, sessionIdsByGroup: effectiveSessionIdsByGroup } = useMemo(
    () =>
      createDisplaySessionLayout({
        enableSessionParking: effectiveSettings.enableSessionParking,
        sessionIdsByGroup: createWorkspaceSessionIdsByGroup(workspaceGroupIds, authoritativeSessionIdsByGroup),
        sessionsById,
        sortMode: activeSessionsSortMode,
        workspaceGroupIds,
      }),
    [
      activeSessionsSortMode,
      authoritativeSessionIdsByGroup,
      effectiveSettings.enableSessionParking,
      sessionsById,
      workspaceGroupIds,
    ]
  );
  const normalizedSessionSearchQuery = sessionSearchQuery.trim();
  const isSessionSearchFiltering =
    isSessionSearchOpen && normalizedSessionSearchQuery.length >= MIN_SESSION_SEARCH_QUERY_LENGTH;
  const isSidebarSearchProjectGroupRenderedCollapsed = (groupId: string) =>
    !isSessionSearchFiltering && collapsedGroupsById[groupId] === true;
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
        type: 'requestPreviousSessions',
      });
    }, 200);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [isSessionSearchFiltering, normalizedSessionSearchQuery, vscode]);
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
    ]
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
      const collection = collectionState?.collections.find((candidate) => candidate.projectIds.includes(projectId));
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
  }, [effectiveGroupIds, groupsById, hiddenCollectionKeys, projectCollections, remoteProjectCollectionsByMachineId]);
  const displayedWorkspaceGroupIds = useMemo(
    () =>
      createDisplayedGroupIds(
        effectiveGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        isSessionSearchFiltering || activeSelectedSessionTagFilters.length > 0
      ).filter(
        (groupId) =>
          showHiddenSidebarItems || (!hiddenGroupIds.includes(groupId) && !hiddenCollectionMemberGroupIds.has(groupId))
      ),
    [
      activeSelectedSessionTagFilters.length,
      displayedWorkspaceSessionIdsByGroup,
      effectiveGroupIds,
      isSessionSearchFiltering,
      hiddenGroupIds,
      hiddenCollectionMemberGroupIds,
      showHiddenSidebarItems,
    ]
  );
  const displayedReferenceChatGroupIds = useMemo(
    () => displayedWorkspaceGroupIds.filter((groupId) => groupsById[groupId]?.isChatCollection),
    [displayedWorkspaceGroupIds, groupsById]
  );
  /*
   * CDXC:SidebarSearch 2026-06-28-06:29:
   * Search results must reveal matching live project sessions even when the
   * user's normal section or project collapse state would hide them. Treat
   * collapse as render-only while filtering so clearing search restores the
   * user's previous sidebar shape without persisting temporary expansion.
   *
   */
  const unfilteredReferenceProjectGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter(
        (groupId) => !groupsById[groupId]?.isChatCollection && !groupsById[groupId]?.remoteMachineContext
      ),
    [displayedWorkspaceGroupIds, groupsById]
  );
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * The Space filter is an intersection applied to each gxserver section's own
   * group list, which is what makes sidebar search Space-scoped for free: the
   * search pass above has already dropped non-matching groups, so filtering the
   * survivors by membership leaves exactly the matches that live in the selected
   * Space. Order does not matter for an intersection, so the empty-group-dropping
   * `shouldFilter` behaviour of `createDisplayedGroupIds` composes unchanged.
   *
   * Quick chats and the synthetic gxserver-unavailable row are excluded before
   * Space visibility is evaluated, so no Space rule ever applies to them.
   */
  const selectedLocalSpace = useMemo(
    () => resolveSelectedSidebarSpace(spacesState, selectedSpaceIdBySectionKey[LOCAL_SIDEBAR_SPACE_SECTION_KEY]),
    [selectedSpaceIdBySectionKey, spacesState]
  );
  const displayedReferenceProjectGroupIds = useMemo(() => {
    const visibleProjectGroupIds = unfilteredReferenceProjectGroupIds.filter(
      (groupId) => groupId !== SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID
    );
    if (!selectedLocalSpace) {
      return visibleProjectGroupIds;
    }
    const isVisibleInSpace = createSelectedSidebarSpaceVisibility({
      collectionState: projectCollections,
      groupIds: visibleProjectGroupIds,
      groupsById,
      resolveProjectId: (groupId) => groupsById[groupId]?.projectContext?.editor.projectId,
      selection: selectedLocalSpace,
    });
    return visibleProjectGroupIds.filter(isVisibleInSpace);
  }, [groupsById, projectCollections, selectedLocalSpace, unfilteredReferenceProjectGroupIds]);
  const groupIdsContainingActiveSession = useMemo(
    () =>
      new Set(
        effectiveGroupIds.filter(
          (groupId) =>
            groupsById[groupId]?.isActive === true &&
            (effectiveSessionIdsByGroup[groupId] ?? []).some((sessionId) => sessionsById[sessionId]?.isFocused === true)
        )
      ),
    [effectiveGroupIds, effectiveSessionIdsByGroup, groupsById, sessionsById]
  );
  /*
   * CDXC:SidebarMachineTabCounts 2026-09-03:
   * The machine tab reports the whole machine, not the sidebar body: every
   * group that belongs to the machine counts, whatever the selected Space,
   * search query, tag filter, or hidden state leaves visible below. Groups are
   * split by their remote machine context, so the local machine is everything
   * without one (minus the synthetic gxserver-unavailable row).
   */
  const machineWideSessionSummariesByMachineId = useMemo(() => {
    const groupIdsByMachineId: Record<string, string[]> = {};
    for (const groupId of effectiveGroupIds) {
      if (groupId === SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID) {
        continue;
      }
      const machineId = groupsById[groupId]?.remoteMachineContext?.machineId ?? LOCAL_SIDEBAR_MACHINE_TAB_ID;
      (groupIdsByMachineId[machineId] ??= []).push(groupId);
    }
    const next: Record<string, SidebarSectionSessionSummary> = {};
    for (const [machineId, groupIds] of Object.entries(groupIdsByMachineId)) {
      next[machineId] = getSidebarSectionSessionSummary(groupIds, effectiveSessionIdsByGroup, sessionsById);
    }
    return next;
  }, [effectiveGroupIds, effectiveSessionIdsByGroup, groupsById, sessionsById]);
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
      const parentCollectionId = parentProjectId ? next.get(parentProjectId) : undefined;
      if (projectId && parentCollectionId) {
        next.set(projectId, parentCollectionId);
      }
    }
    return next;
  }, [displayedWorkspaceGroupIds, groupsById, projectCollections]);
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
      groupsById[groupId]?.projectContext?.editor.projectId
  ): SidebarProjectCollectionRenderItem[] => {
    if (!enableProjectCollections) {
      return sectionGroupIds.map((groupId) => ({ groupId, kind: 'project' }));
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
      const inheritedCollectionId = parentProjectId ? collectionIdByProjectId.get(parentProjectId) : undefined;
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
      projectIdByGroupId.has(groupId) ? [] : [{ groupId, kind: 'project' as const }]
    );
    for (const collection of collectionState.collections) {
      const visibleProjectIds = collection.projectIds.filter((projectId) => groupIdByProjectId.has(projectId));
      const explicitlyOrderedProjectIds = new Set(visibleProjectIds);
      for (const candidateGroupId of sectionGroupIds) {
        const candidateProjectId = projectIdByGroupId.get(candidateGroupId);
        if (
          candidateProjectId &&
          !explicitlyOrderedProjectIds.has(candidateProjectId) &&
          collectionIdByProjectId.get(candidateProjectId) === collection.collectionId
        ) {
          explicitlyOrderedProjectIds.add(candidateProjectId);
          visibleProjectIds.push(candidateProjectId);
        }
      }
      if (visibleProjectIds.length === 0) {
        continue;
      }
      emittedCollectionIds.add(collection.collectionId);
      items.push({
        collection: { ...collection, projectIds: visibleProjectIds },
        groupIds: visibleProjectIds
          .map((candidate) => groupIdByProjectId.get(candidate))
          .filter((candidate): candidate is string => Boolean(candidate)),
        kind: 'collection',
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
      items.push({ groupId, kind: 'project' });
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
    ]
  );
  const createProjectCollectionForProject = useEffectEvent((projectId: string) => {
    const created = createSidebarProjectCollection(projectCollections, projectId);
    setProjectCollections(
      moveProjectsToSidebarCollection(
        created.state,
        getProjectCollectionFamilyProjectIds(projectId, displayedReferenceProjectGroupIds, groupsById),
        created.collectionId
      )
    );
    setAutoEditingProjectCollectionId(created.collectionId);
  });
  const moveProjectToCollection = useEffectEvent((projectId: string, collectionId: string | undefined) => {
    setProjectCollections((previous) =>
      moveProjectsToSidebarCollection(
        previous,
        getProjectCollectionFamilyProjectIds(projectId, displayedReferenceProjectGroupIds, groupsById),
        collectionId
      )
    );
  });
  const updateRemoteProjectCollections = useEffectEvent(
    (machineId: string, update: (state: SidebarProjectCollectionsState) => SidebarProjectCollectionsState) => {
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
        type: 'updateSidebarProjectCollections',
      });
    }
  );
  const createRemoteProjectCollectionForProject = useEffectEvent(
    (machineId: string, projectId: string, machineGroupIds: readonly string[]) => {
      const rawProjectIds = getRemoteProjectCollectionFamilyProjectIds(projectId, machineGroupIds, groupsById);
      const rawProjectId = rawProjectIds[0];
      if (!rawProjectId) {
        return;
      }
      let createdCollectionId: string | undefined;
      updateRemoteProjectCollections(machineId, (previous) => {
        const created = createSidebarProjectCollection(previous, rawProjectId);
        createdCollectionId = created.collectionId;
        return moveProjectsToSidebarCollection(created.state, rawProjectIds, created.collectionId);
      });
      if (createdCollectionId) {
        setAutoEditingProjectCollectionId(`${machineId}:${createdCollectionId}`);
      }
    }
  );
  const moveRemoteProjectToCollection = useEffectEvent(
    (machineId: string, projectId: string, collectionId: string | undefined, machineGroupIds: readonly string[]) => {
      const rawProjectIds = getRemoteProjectCollectionFamilyProjectIds(projectId, machineGroupIds, groupsById);
      updateRemoteProjectCollections(machineId, (previous) =>
        moveProjectsToSidebarCollection(previous, rawProjectIds, collectionId)
      );
    }
  );
  const displayedProjectCollectionGroupIds = useMemo(
    () => displayedProjectCollectionItems.flatMap((item) => (item.kind === 'project' ? [item.groupId] : item.groupIds)),
    [displayedProjectCollectionItems]
  );
  const unfilteredRemoteProjectGroupIdsByMachineId = useMemo(() => {
    const next: Record<string, string[]> = {};
    for (const groupId of displayedWorkspaceGroupIds) {
      const remoteMachineContext = groupsById[groupId]?.remoteMachineContext;
      if (!remoteMachineContext) {
        continue;
      }
      next[remoteMachineContext.machineId] ??= [];
      next[remoteMachineContext.machineId].push(groupId);
    }
    return next;
  }, [displayedWorkspaceGroupIds, groupsById]);
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * Each remote gxserver owns its own Spaces and its own selection, so a machine
   * whose daemon never delivered a Space state is Space-incapable and keeps its
   * full list. Remote membership is keyed by the RAW project id the remote
   * daemon knows (`remoteMachineContext.projectId`), not the sidebar's
   * machine-scoped editor project id.
   */
  const remoteProjectGroupIdsByMachineId = useMemo(() => {
    const next: Record<string, string[]> = {};
    for (const [machineId, machineGroupIds] of Object.entries(unfilteredRemoteProjectGroupIdsByMachineId)) {
      const selectedSpace = resolveSelectedSidebarSpace(
        remoteSpacesByMachineId[machineId],
        selectedSpaceIdBySectionKey[createRemoteSidebarSpaceSectionKey(machineId)]
      );
      if (!selectedSpace) {
        next[machineId] = machineGroupIds;
        continue;
      }
      const isVisibleInSpace = createSelectedSidebarSpaceVisibility({
        collectionState: remoteProjectCollectionsByMachineId[machineId] ?? {
          collections: [],
          nextCollectionNumber: 1,
        },
        groupIds: machineGroupIds,
        groupsById,
        resolveProjectId: (groupId) => groupsById[groupId]?.remoteMachineContext?.projectId,
        selection: selectedSpace,
      });
      next[machineId] = machineGroupIds.filter(isVisibleInSpace);
    }
    return next;
  }, [
    groupsById,
    remoteProjectCollectionsByMachineId,
    remoteSpacesByMachineId,
    selectedSpaceIdBySectionKey,
    unfilteredRemoteProjectGroupIdsByMachineId,
  ]);
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * The rendered group set once every section's Space filter has been applied.
   * The search empty state keys off THIS list rather than the unfiltered one: a
   * query that only matches projects outside the selected Space renders nothing
   * at all, so measuring "did search find anything" against the unfiltered list
   * left the user staring at empty sections instead of the no-results
   * placeholder. Quick chats and the synthetic gxserver-unavailable row are never
   * Space-filtered, so they pass through unconditionally.
   */
  const spaceFilteredDisplayedWorkspaceGroupIds = useMemo(() => {
    const visibleProjectGroupIds = new Set<string>(displayedReferenceProjectGroupIds);
    for (const machineGroupIds of Object.values(remoteProjectGroupIdsByMachineId)) {
      for (const groupId of machineGroupIds) {
        visibleProjectGroupIds.add(groupId);
      }
    }
    return displayedWorkspaceGroupIds.filter(
      (groupId) =>
        groupId === SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID ||
        groupsById[groupId]?.isChatCollection === true ||
        visibleProjectGroupIds.has(groupId)
    );
  }, [displayedReferenceProjectGroupIds, displayedWorkspaceGroupIds, groupsById, remoteProjectGroupIdsByMachineId]);
  const remoteMachines = (settings?.remoteMachines ?? []).filter(isRemoteMachineEnabledInSidebar);
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
  const visibleRemoteMachineIdsKey = settings
    ? JSON.stringify(settings.remoteMachines.filter(isRemoteMachineEnabledInSidebar).map((machine) => machine.id))
    : undefined;
  /*
   * CDXC:SidebarMachineTabs 2026-08-28:
   * A tab for the local machine plus one per saved remote machine, in the saved
   * order. Each remote tab carries its connection state so a machine that cannot
   * connect is visible as such without opening it, and the same working/
   * attention/awake counts its section header used to show while collapsed.
   */
  const reconnectRemoteMachine = useEffectEvent((machineId: string) => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:remoteReconnect');
    vscode.postMessage({
      remoteMachineId: machineId,
      type: 'reconnectRemoteMachine',
    });
  });
  /*
   * CDXC:SidebarProjectMenu 2026-09-02:
   * The remote machine header's connection control moved onto its tab
   * (CDXC:GPUIRemoteConnectFeedback 2026-07-21 describes the states): a
   * connected machine shows nothing extra, a busy one spins, and a
   * disconnected or failed one carries a Connect / retry glyph whose tooltip
   * is the host's sanitized failure reason. Native still owns the matching
   * viewport-level toast for progress and failures.
   */
  const machineTabItems = useMemo<readonly SidebarMachineTabItem[]>(() => {
    const localTab: SidebarMachineTabItem = {
      id: LOCAL_SIDEBAR_MACHINE_TAB_ID,
      label: 'Local',
      sessionSummary: machineWideSessionSummariesByMachineId[LOCAL_SIDEBAR_MACHINE_TAB_ID],
    };
    return [
      localTab,
      ...remoteMachines.map((machine): SidebarMachineTabItem => {
        const status = remoteMachineRuntimeStatuses[machine.id] ?? 'disconnected';
        const busyLabel = remoteMachineBusyLabel(status);
        const connectionState: SidebarMachineTabItem['connectionState'] =
          status === 'connected'
            ? 'connected'
            : busyLabel !== undefined
              ? 'busy'
              : status === 'disconnected'
                ? 'disconnected'
                : 'failed';
        return {
          connectionLabel:
            connectionState === 'connected'
              ? undefined
              : connectionState === 'busy'
                ? busyLabel
                : connectionState === 'disconnected'
                  ? 'Connect'
                  : `Error: ${remoteMachineStatusMessages[machine.id] ?? remoteMachineFailureLabel(status)}`,
          connectionState,
          id: machine.id,
          label: machine.name,
          onConnect:
            connectionState === 'disconnected' || connectionState === 'failed'
              ? () => reconnectRemoteMachine(machine.id)
              : undefined,
          sessionSummary: machineWideSessionSummariesByMachineId[machine.id],
        };
      }),
    ];
  }, [
    machineWideSessionSummariesByMachineId,
    remoteMachineRuntimeStatuses,
    remoteMachineStatusMessages,
    remoteMachines,
  ]);
  /*
   * A stored tab whose machine was removed (or was never saved on this host)
   * falls back to Local. Gate on the authoritative saved-machine signal for the
   * same reason the collapse map does: before host settings arrive, an empty
   * machine list means "not known yet", not "no machines".
   */
  useEffect(() => {
    if (visibleRemoteMachineIdsKey === undefined) {
      return;
    }
    const remoteMachineIds = new Set<string>(JSON.parse(visibleRemoteMachineIdsKey) as string[]);
    setSelectedMachineTabId((previous) =>
      previous === LOCAL_SIDEBAR_MACHINE_TAB_ID || remoteMachineIds.has(previous)
        ? previous
        : LOCAL_SIDEBAR_MACHINE_TAB_ID
    );
  }, [visibleRemoteMachineIdsKey]);
  useEffect(() => {
    writeSidebarSelectedMachineTabId(windowScopeId, selectedMachineTabId);
  }, [selectedMachineTabId, windowScopeId]);
  /*
   * CDXC:SidebarMachineTabs 2026-08-28:
   * The selected machine is only a remote one while that machine is actually in
   * the saved list. The stored id is read before host settings arrive, so the
   * render must not hide the local project list for a machine that has no
   * section to render yet (or any more) — that window would show an empty
   * sidebar body instead of the machine the tab strip reports as selected.
   */
  const selectedRemoteMachineId =
    selectedMachineTabId !== LOCAL_SIDEBAR_MACHINE_TAB_ID &&
    remoteMachines.some((machine) => machine.id === selectedMachineTabId)
      ? selectedMachineTabId
      : undefined;
  const isLocalMachineTabSelected = selectedRemoteMachineId === undefined;
  useEffect(() => {
    if (savedRemoteMachineIdsKey === undefined || !hasAppliedHydrateRef.current) {
      return;
    }
    const remoteMachineIds = new Set<string>(JSON.parse(savedRemoteMachineIdsKey) as string[]);
    /*
     * CDXC:SidebarSpaces 2026-08-27:
     * A forgotten machine's Space state and its remembered Space selection go
     * with it, on the authoritative saved-machine signal, so a re-added machine
     * never inherits a stale selection.
     */
    setRemoteSpacesByMachineId((previous) => {
      let next: Record<string, SidebarSpacesState> | undefined;
      for (const machineId of Object.keys(previous)) {
        if (!remoteMachineIds.has(machineId)) {
          next ??= { ...previous };
          delete next[machineId];
        }
      }
      return next ?? previous;
    });
    setSelectedSpaceIdBySectionKey((previous) => {
      let next: Record<string, string> | undefined;
      for (const sectionKey of Object.keys(previous)) {
        if (
          sectionKey !== LOCAL_SIDEBAR_SPACE_SECTION_KEY &&
          ![...remoteMachineIds].some((machineId) => createRemoteSidebarSpaceSectionKey(machineId) === sectionKey)
        ) {
          next ??= { ...previous };
          delete next[sectionKey];
        }
      }
      return next ?? previous;
    });
  }, [savedRemoteMachineIdsKey]);
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * Every Space selection change goes through here. The persisted map holds
   * either a real Space id or the reserved built-in `other` id; a section with
   * no entry has never been switched and resolves through the shared default
   * rule (first Space, else Other).
   */
  const selectSidebarSpace = useEffectEvent((sectionKey: string, spaceId: string) => {
    setSelectedSpaceIdBySectionKey((previous) => {
      if (previous[sectionKey] === spaceId) {
        return previous;
      }
      return { ...previous, [sectionKey]: spaceId };
    });
  });
  const reorderLocalSpaces = useEffectEvent((orderedSpaceIds: string[]) => {
    setSpacesState((previous) => (previous ? reorderSidebarSpaces(previous, orderedSpaceIds) : previous));
  });
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * Remote Space state has no write-through effect the way the local one does —
   * it is a per-machine map, and a single effect over it could not tell WHICH
   * machine changed — so every remote edit goes through here and posts its own
   * daemon-scoped update, exactly like `updateRemoteProjectCollections`.
   */
  const updateRemoteSpaces = useEffectEvent(
    (machineId: string, update: (state: SidebarSpacesState) => SidebarSpacesState) => {
      const current = storedRemoteSpacesByMachineId[machineId];
      if (!current) {
        return;
      }
      const updated = update(current);
      if (areSidebarSpacesStatesEqual(current, updated)) {
        return;
      }
      setRemoteSpacesByMachineId((previous) => ({
        ...previous,
        [machineId]: updated,
      }));
      vscode.postMessage({
        remoteMachineId: machineId,
        state: serializeSidebarSpacesForGxserver(updated),
        type: 'updateSidebarSpaces',
      });
    }
  );
  const reorderRemoteSpaces = useEffectEvent((machineId: string, orderedSpaceIds: string[]) => {
    updateRemoteSpaces(machineId, (previous) => reorderSidebarSpaces(previous, orderedSpaceIds));
  });
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * Membership toggles from the group / ungrouped-project context menus. The
   * member ids are always in the OWNING daemon's id space: a local project's own
   * project id, and for a remote machine the raw remote project/collection id
   * that machine's own documents use — never a sidebar-composed id.
   */
  const toggleLocalSpaceCollectionMembership = useEffectEvent((spaceId: string, collectionId: string) => {
    setSpacesState((previous) =>
      previous ? toggleSpaceCollectionMembership(previous, spaceId, collectionId) : previous
    );
  });
  const toggleLocalSpaceProjectMembership = useEffectEvent((spaceId: string, projectId: string) => {
    setSpacesState((previous) => (previous ? toggleSpaceProjectMembership(previous, spaceId, projectId) : previous));
  });
  const toggleRemoteSpaceCollectionMembership = useEffectEvent(
    (machineId: string, spaceId: string, collectionId: string) => {
      updateRemoteSpaces(machineId, (previous) => toggleSpaceCollectionMembership(previous, spaceId, collectionId));
    }
  );
  const toggleRemoteSpaceProjectMembership = useEffectEvent((machineId: string, spaceId: string, projectId: string) => {
    updateRemoteSpaces(machineId, (previous) => toggleSpaceProjectMembership(previous, spaceId, projectId));
  });
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * The New/Edit Space dialog's confirm and delete. The dialog carries only the
   * user's field values, so the mutation is composed HERE against the Space
   * document this sidebar holds right now — never against the snapshot the
   * dialog opened on.
   *
   * Deleting the selected Space drops its id from the selection map, so the
   * section falls back through the shared default rule — its first remaining
   * Space, or Other when it has none — exactly like a section that has never
   * been switched. `resolveSelectedSidebarSpace` already renders that for an
   * unresolvable id; dropping the id here additionally keeps the persisted
   * selection map free of dead ids.
   */
  const applySpaceEditorResult = useEffectEvent((result: ApplySidebarSpaceEditorResultMessage) => {
    if (result.mode === 'delete' && result.spaceId) {
      const sectionKey = result.remoteMachineId
        ? createRemoteSidebarSpaceSectionKey(result.remoteMachineId)
        : LOCAL_SIDEBAR_SPACE_SECTION_KEY;
      setSelectedSpaceIdBySectionKey((previous) => {
        if (previous[sectionKey] !== result.spaceId) {
          return previous;
        }
        const next = { ...previous };
        delete next[sectionKey];
        return next;
      });
    }
    if (result.remoteMachineId) {
      updateRemoteSpaces(result.remoteMachineId, (previous) => applySidebarSpaceEditorResult(previous, result));
      return;
    }
    setSpacesState((previous) => (previous ? applySidebarSpaceEditorResult(previous, result) : previous));
  });
  /*
   * CDXC:SidebarMachineTabMenu 2026-09-02:
   * Hide Machine from a tab's context menu is the same edit as switching off
   * "Show in sidebar" in Settings → Remote: the saved machine stays, only its
   * `disabled` flag flips, sent under the explicit remote-machine source so
   * the host accepts the machine-list change. The selected-tab effect then
   * falls back to Local on its own if the hidden machine was the open one.
   */
  const hideRemoteMachine = useEffectEvent((machineId: string) => {
    if (!settings) {
      return;
    }
    vscode.postMessage({
      baseRevision: revision,
      patch: {
        remoteMachines: settings.remoteMachines.map((machine) =>
          machine.id === machineId ? { ...machine, disabled: true } : machine
        ),
      },
      source: 'settings:remoteMachines',
      type: 'updateSettingsPatch',
    });
  });
  const moveRemoteMachineSection = useEffectEvent(
    (sourceRemoteMachineId: string, target: SidebarRemoteMachineDropTarget) => {
      if (!settings) {
        return;
      }
      const nextEnabledRemoteMachineIds = moveRemoteMachineIdToDropTarget(
        settings.remoteMachines.filter(isRemoteMachineEnabledInSidebar).map((machine) => machine.id),
        sourceRemoteMachineId,
        target
      );
      if (!nextEnabledRemoteMachineIds) {
        return;
      }
      const nextRemoteMachines = applyEnabledRemoteMachineOrder(settings.remoteMachines, nextEnabledRemoteMachineIds);
      if (!nextRemoteMachines) {
        return;
      }
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
        source: 'sidebar:remoteMachineOrder',
        type: 'updateSettingsPatch',
      });
    }
  );
  const filteredPreviousSessions = useMemo(() => {
    if (!isSessionSearchFiltering) {
      return [];
    }
    const searchResults =
      remoteSessionSearchPreviousSessions ?? filterPreviousSessions(previousSessions, normalizedSessionSearchQuery);
    return filterDefaultNamedSessionSearchItems(searchResults);
  }, [isSessionSearchFiltering, normalizedSessionSearchQuery, previousSessions, remoteSessionSearchPreviousSessions]);
  const hasExpandedReferenceProjects = useMemo(
    () => displayedReferenceProjectGroupIds.some((groupId) => collapsedGroupsById[groupId] !== true),
    [collapsedGroupsById, displayedReferenceProjectGroupIds]
  );
  const toggleReferenceProjectsBulk = () => {
    postSidebarCollapseStateLog('projectBulkCommand', {
      expandedProjectGroupCount:
        displayedReferenceProjectGroupIds.length -
        Object.keys(collapsedGroupsById).filter((groupId) => displayedReferenceProjectGroupIds.includes(groupId))
          .length,
      mode: hasExpandedReferenceProjects ? 'collapse-all' : 'expand-previous',
      previousExpandedGroupCount: previousExpandedReferenceProjectGroupIdsRef.current.length,
      projectGroupCount: displayedReferenceProjectGroupIds.length,
    });
    if (hasExpandedReferenceProjects) {
      previousExpandedReferenceProjectGroupIdsRef.current = displayedReferenceProjectGroupIds.filter(
        (groupId) => collapsedGroupsById[groupId] !== true
      );
      setGroupsCollapsed(displayedReferenceProjectGroupIds, true);
      return;
    }

    const previousExpandedProjectGroupIds = previousExpandedReferenceProjectGroupIdsRef.current.filter((groupId) =>
      displayedReferenceProjectGroupIds.includes(groupId)
    );
    setGroupsCollapsed(
      previousExpandedProjectGroupIds.length > 0 ? previousExpandedProjectGroupIds : displayedReferenceProjectGroupIds,
      false
    );
  };
  const toggleRemoteMachineProjectsBulk = (machineId: string) => {
    const machineProjectGroupIds = remoteProjectGroupIdsByMachineId[machineId] ?? [];
    const hasExpandedMachineProjects = machineProjectGroupIds.some((groupId) => collapsedGroupsById[groupId] !== true);
    if (hasExpandedMachineProjects) {
      previousExpandedRemoteProjectGroupIdsByMachineIdRef.current[machineId] = machineProjectGroupIds.filter(
        (groupId) => collapsedGroupsById[groupId] !== true
      );
      setGroupsCollapsed(machineProjectGroupIds, true);
      return;
    }
    const previousExpandedProjectGroupIds =
      previousExpandedRemoteProjectGroupIdsByMachineIdRef.current[machineId]?.filter((groupId) =>
        machineProjectGroupIds.includes(groupId)
      ) ?? [];
    setGroupsCollapsed(
      previousExpandedProjectGroupIds.length > 0 ? previousExpandedProjectGroupIds : machineProjectGroupIds,
      false
    );
  };
  const handleSidebarProjectJump = useEffectEvent((detail: SidebarProjectJumpEventDetail) => {
    const shouldRevealFocusedSession = detail.revealFocusedSession === true;
    const requestFocusedSessionReveal = () => {
      if (!shouldRevealFocusedSession) {
        return;
      }
      setFocusedSessionRevealRequestId((requestId) => requestId + 1);
    };

    if (!detail.expandCollapsedProject || !displayedReferenceProjectGroupIds.includes(detail.groupId)) {
      requestFocusedSessionReveal();
      return;
    }

    const wasProjectCollapsed = collapsedGroupsById[detail.groupId] === true;
    if (!wasProjectCollapsed) {
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
    postSidebarCollapseStateLog('projectJumpAutoExpand', {
      projectGroupCount: displayedReferenceProjectGroupIds.length,
      groupHash: hashSidebarCollapseDebugId(detail.groupId),
      revealFocusedSession: shouldRevealFocusedSession,
      showLessAfterExpand: detail.showLessAfterExpand,
      wasProjectCollapsed,
    });
    setGroupCollapsed(detail.groupId, false);
    if (detail.showLessAfterExpand) {
      setProjectSessionListCollapsed(detail.projectId, true);
    }
    requestFocusedSessionReveal();
  });
  const resolveGpuiProjectSlotHotkey = useEffectEvent((slotNumber: number) => {
    if (!Number.isInteger(slotNumber) || slotNumber < 1 || slotNumber > 9) {
      return;
    }

    const groupId = displayedReferenceProjectGroupIds[slotNumber - 1];
    const projectId = groupId ? groupsById[groupId]?.projectContext?.editor.projectId : undefined;
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
    const groupSessionIds = displayedWorkspaceSessionIdsByGroup[groupId] ?? [];
    const targetSessionId =
      groupSessionIds.find((sessionId) => sessionsById[sessionId]?.isFocused === true) ?? groupSessionIds[0];
    if (!targetSessionId) {
      return;
    }
    focusSidebarSessionFromNavigation(groupId, targetSessionId);
    vscode.postMessage({
      sessionId: targetSessionId,
      type: 'focusSession',
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
  }, [handleSidebarProjectJump]);
  const focusedSessionId = useMemo(
    () => Object.values(sessionsById).find((session) => session.isFocused)?.sessionId,
    [sessionsById]
  );
  const postMultiSelectSelectionDebugLog = useEffectEvent((event: string, details: Record<string, unknown>) => {
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
      scenarioId: 'native.sidebar.refresh',
      type: 'sidebarDebugLog',
    });
  });
  const handleSidebarSessionSelectionChange = useEffectEvent(
    (request: { groupId: string; mode: 'additive' | 'clear' | 'range'; reason?: string; sessionId: string }) => {
      if (request.mode === 'clear') {
        if (selectedSidebarSessionIds.length > 0) {
          postMultiSelectSelectionDebugLog('selectionCleared', {
            previousCount: selectedSidebarSessionIds.length,
            reason: request.reason ?? 'unknown',
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
      const visibleSessionIds = readRenderedSidebarSessionSlotIds(sessionGroupsContentRef.current ?? document, {
        skipPaneHiddenRows: false,
      });
      if (request.mode === 'range') {
        const nextSelection = resolveRenderedSidebarSessionRangeSelection({
          activeSessionId: focusedSessionId,
          clickedSessionId: request.sessionId,
          visibleSessionIds,
        });
        postMultiSelectSelectionDebugLog('selectionResolved', {
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
      postMultiSelectSelectionDebugLog('selectionResolved', {
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
    }
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
    const nextSelection = selectedSidebarSessionIds.filter((sessionId) => sessionsById[sessionId] !== undefined);
    if (nextSelection.length === selectedSidebarSessionIds.length) {
      return;
    }
    postMultiSelectSelectionDebugLog('selectionPruned', {
      previousCount: selectedSidebarSessionIds.length,
      prunedSessionIds: selectedSidebarSessionIds
        .filter((sessionId) => sessionsById[sessionId] === undefined)
        .slice(0, 30),
      remainingCount: nextSelection.length,
    });
    setSelectedSidebarSessionIds(nextSelection);
  }, [selectedSidebarSessionIds, sessionsById]);
  const postSidebarWakeScrollLog = useEffectEvent(
    (event: string, targetSessionId: string, details: Record<string, unknown>) => {
      postSidebarDebugLog('native.sidebar.refresh', `repro.sidebarWakeScroll.${event}`, {
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
        ...summarizeSidebarWakeScrollRenderedSlots(sessionGroupsContentRef.current ?? document, targetSessionId),
      });
    }
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
      type: 'focusSession',
    });
  });
  const runGhostexHotkeyAction = useEffectEvent((actionId: string) => {
    const action = getghostexHotkeyActionById(actionId);
    if (!action) {
      return;
    }

    if (action.kind === 'focusSessionSlot') {
      dismissAppModalForSidebarNavigation('SettingsDismissal:focusSessionHotkey');
      focusSidebarSessionSlot(action.slotNumber);
      return;
    }

    if (action.kind === 'createSession') {
      requestNewSession();
      return;
    }

    if (action.kind === 'openCommandPalette') {
      openCommandPalette();
      return;
    }

    if (action.kind === 'openSessionSearchPalette') {
      openPreviousSessions();
      return;
    }

    if (action.kind === 'openSettings') {
      openSidebarSettings();
      return;
    }

    if (action.kind === 'openHotkeys') {
      openHotkeys();
      return;
    }

    if (action.kind === 'moveSidebar') {
      moveSidebar();
      return;
    }

    if (action.kind === 'toggleSidebarCollapsed') {
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
      action.kind === 'focusAdjacentGroup' ||
      action.kind === 'focusDirection' ||
      action.kind === 'focusedPaneAction' ||
      action.kind === 'jumpToProject' ||
      /*
       * CDXC:NavigationHistory 2026-08-19:
       * Back/Forward is host-owned: gpui walks the trail through its native
       * titlebar route and the web shell through its sidebar runtime, so the
       * palette row forwards the action id and nothing else, exactly like the
       * other host-owned navigation rows here.
       */
      action.kind === 'navigateHistory' ||
      action.kind === 'openCommandsPanel' ||
      action.kind === 'renameActiveSession' ||
      action.kind === 'runActionSlot' ||
      action.kind === 'setViewMode' ||
      action.kind === 'splitFocusedPane' ||
      action.kind === 'switchWorkareaView' ||
      action.kind === 'terminalToolbarAction' ||
      action.kind === 'toggleCompanionPane'
    ) {
      vscode.postMessage({ actionId: action.id, type: 'runGhostexHotkeyAction' });
    }
  });
  useLayoutEffect(() => {
    if (didApplyStartupEmptyChatsCollapseRef.current || !hasAppliedHydrateRef.current) {
      return;
    }

    didApplyStartupEmptyChatsCollapseRef.current = true;
    const hasChatSessions = displayedReferenceChatGroupIds.some(
      (groupId) => (authoritativeSessionIdsByGroup[groupId] ?? []).length > 0
    );
    if (!hasChatSessions) {
      postSidebarCollapseStateLog('sectionAutoCollapsed', {
        reason: 'startup-empty-quick',
        section: 'quick',
      });
      /**
       * CDXC:SidebarReference 2026-05-10-15:51
       * Startup restores the user's section/group collapse state, except an empty
       * Combined Chats section must always begin collapsed so a project-only
       * workspace does not waste vertical space on an empty chat container.
       */
      setIsReferenceChatsCollapsed(true);
    }
  }, [authoritativeSessionIdsByGroup, displayedReferenceChatGroupIds]);

  useEffect(() => {
    /**
     * CDXC:SidebarReference 2026-05-10-15:51
     * Combined section headers and per-group collapse state are
     * UI navigation state. Persist them in the sidebar webview so restarting
     * ghostex keeps collapsed items collapsed and expanded items expanded.
     * CDXC:SidebarReference 2026-05-20-12:00
     * The first post-hydrate group-collapse reconcile seeds session-count baseline
     * without expand-on-count-increase so restored projects do not reopen on launch.
     */
    const nextCollapseState = {
      collapsedGroupsById,
      collapsedProjectCollectionsByKey,
      collapsedProjectSessionListsById,
      isReferenceChatsCollapsed,
      selectedSpaceIdBySectionKey,
    };
    const writeResult = writeSidebarUiCollapseState(windowScopeId, nextCollapseState);
    postSidebarCollapseStateLog('write', {
      ...summarizeSidebarUiCollapseState(nextCollapseState),
      groupCount: groupOrder.length,
      storedByteLength: writeResult.storedByteLength ?? 0,
      writeOk: writeResult.ok,
      writeReason: writeResult.reason ?? 'stored',
    });
  }, [
    collapsedGroupsById,
    collapsedProjectCollectionsByKey,
    collapsedProjectSessionListsById,
    isReferenceChatsCollapsed,
    selectedSpaceIdBySectionKey,
    windowScopeId,
  ]);

  const shouldShowSessionSearchEmptyState =
    isSessionSearchFiltering &&
    spaceFilteredDisplayedWorkspaceGroupIds.length === 0 &&
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
  const shouldShowFirstProjectEmptyState = !isSessionSearchOpen && !hasKnownProjectInventoryForEmptyState;
  /*
   * CDXC:SidebarProjectsEmptyState 2026-06-30-03:25:
   * Sidebar search must not flash first-project onboarding after any project is
   * known. Search filtering and transient group display updates can temporarily
   * remove all visible Projects rows, so decide the first-run copy from
   * authoritative project inventory and parked Recent Projects instead of the
   * current displayed group arrays.
   */
  const referenceProjectsEmptyState = showGxserverUnavailableEmptyState ? (
    <div className='reference-sidebar-empty-state'>
      Unable to load sessions.
      <br />
      {onStartGxserver ? (
        <button className='reference-sidebar-empty-state-action' onClick={onStartGxserver} type='button'>
          Load Sessions
        </button>
      ) : (
        'Restart Ghostex to try again.'
      )}
    </div>
  ) : hasGxserverUnavailablePlaceholder ? null : (
    <div className='reference-sidebar-empty-state'>
      {shouldShowFirstProjectEmptyState ? (
        <>
          No Projects Added.
          <br />
          <br />
          {'Open the More menu at the top of the sidebar and choose Add Project to get started!'}
        </>
      ) : (
        'No projects'
      )}
    </div>
  );
  const { hasOverflow: sessionGroupsHaveScrollableOverflow } = useScrollGlowState(sessionGroupsContentRef);
  const sidebarSessionSearchResults = useMemo(
    () =>
      createSidebarSessionSearchResults({
        displayedWorkspaceGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        filteredPreviousSessions,
      }),
    [displayedWorkspaceGroupIds, displayedWorkspaceSessionIdsByGroup, filteredPreviousSessions]
  );
  useEffect(() => {
    groupIdsRef.current = displayedProjectCollectionGroupIds;
  }, [displayedProjectCollectionGroupIds]);

  useEffect(() => {
    sessionIdsByGroupRef.current = displayedWorkspaceSessionIdsByGroup;
  }, [displayedWorkspaceSessionIdsByGroup]);

  useEffect(() => {
    const queryChanged = previousNormalizedSessionSearchQueryRef.current !== normalizedSessionSearchQuery;
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
        return createSidebarSessionSearchSelection(sidebarSessionSearchResults[0]);
      }

      if (!previous) {
        return undefined;
      }

      return sidebarSessionSearchResults.some((result) => isSidebarSessionSearchSelectionMatch(result, previous))
        ? previous
        : createSidebarSessionSearchSelection(sidebarSessionSearchResults[0]);
    });
  }, [isSessionSearchOpen, normalizedSessionSearchQuery, sidebarSessionSearchResults]);

  useEffect(() => {
    if (!isSessionSearchSelectionVisible || !selectedSessionSearchResult) {
      return;
    }

    const selectedElement =
      selectedSessionSearchResult.kind === 'session'
        ? document.querySelector<HTMLElement>(`[data-sidebar-session-id="${selectedSessionSearchResult.sessionId}"]`)
        : document.querySelector<HTMLElement>(`[data-sidebar-history-id="${selectedSessionSearchResult.historyId}"]`);
    selectedElement?.scrollIntoView({
      block: 'nearest',
    });
  }, [isSessionSearchSelectionVisible, selectedSessionSearchResult]);

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
        postSidebarWakeScrollLog('focusedRowScrollSkipped', focusedSessionId, {
          reason: 'missing-scroll-viewport',
          sequence,
        });
        return;
      }

      if (!isExplicitFocusedSessionRevealRequest) {
        const suppression = consumeFocusedSessionScrollSuppression();
        if (suppression) {
          postSidebarWakeScrollLog('focusedRowScrollSkipped', focusedSessionId, {
            reason: 'close-driven-focus-scroll-suppressed',
            sequence,
            suppressionReason: suppression.reason,
          });
          return;
        }
      }

      const focusedSessionElement = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${focusedSessionId}"]`
      );
      if (!focusedSessionElement) {
        postSidebarWakeScrollLog('focusedRowScrollSkipped', focusedSessionId, {
          reason: 'missing-focused-row',
          sequence,
        });
        return;
      }

      const beforeScrollTop = scrollViewport.scrollTop;
      const beforeGeometry = summarizeSidebarWakeScrollGeometry(focusedSessionElement, scrollViewport);
      const scrollIssued = scrollElementIntoViewIfNeeded(focusedSessionElement, scrollViewport);
      postSidebarWakeScrollLog('focusedRowScrollDecision', focusedSessionId, {
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
          `[data-sidebar-session-id="${focusedSessionId}"]`
        );
        postSidebarWakeScrollLog('focusedRowScrollAfterFrame', focusedSessionId, {
          afterGeometry:
            nextScrollViewport && nextFocusedSessionElement
              ? summarizeSidebarWakeScrollGeometry(nextFocusedSessionElement, nextScrollViewport)
              : undefined,
          scrollDeltaTop: nextScrollViewport ? nextScrollViewport.scrollTop - beforeScrollTop : undefined,
          sequence,
        });
      });
      afterSettledTimeoutId = window.setTimeout(() => {
        const settledScrollViewport = sessionGroupsContentRef.current;
        const settledFocusedSessionElement = document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${focusedSessionId}"]`
        );
        postSidebarWakeScrollLog('focusedRowScrollAfterSettled', focusedSessionId, {
          afterGeometry:
            settledScrollViewport && settledFocusedSessionElement
              ? summarizeSidebarWakeScrollGeometry(settledFocusedSessionElement, settledScrollViewport)
              : undefined,
          scrollDeltaTop: settledScrollViewport ? settledScrollViewport.scrollTop - beforeScrollTop : undefined,
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
  }, [consumeFocusedSessionScrollSuppression, focusedSessionId, focusedSessionRevealRequestId]);

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
        displayedWorkspaceSessionIdsByGroup[candidateGroupId]?.includes(sessionId)
      );
      if (!groupId) {
        // The row has not been published yet; this effect re-runs when it is.
        return;
      }
      handledSessionRevealRequestIdRef.current = requestId;
      pendingSessionRevealScrollRequestIdRef.current = requestId;

      const collectionItem = displayedProjectCollectionItems.find(
        (item) => item.kind === 'collection' && item.groupIds.includes(groupId)
      );
      if (collectionItem?.kind === 'collection') {
        setProjectCollectionCollapsed(
          createLocalProjectCollectionCollapseKey(collectionItem.collection.collectionId),
          false
        );
      }
      setGroupCollapsed(groupId, false);
      const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
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
      const revealedRow = document.querySelector<HTMLElement>(`[data-sidebar-session-id="${sessionId}"]`);
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
      const revealedRow = document.querySelector<HTMLElement>(`[data-sidebar-session-id="${sessionId}"]`);
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
      void Promise.allSettled(expandAnimations.map((expandAnimation) => expandAnimation.finished)).then(
        scrollRevealedRowIntoView
      );
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
    sessionRevealRequest,
  ]);

  const unlockCompletionSoundPlayback = useEffectEvent(() => {
    void prepareCompletionSoundPlayback((soundEvent, details) => {
      postSidebarDebugLog('native.agent.detection', soundEvent, details);
    });
  });

  const recordPointerDownSessionTarget = useEffectEvent((event: PointerEvent) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      pointerDownSessionTargetRef.current = undefined;
      return;
    }

    const sessionElement = target.closest<HTMLElement>('[data-sidebar-session-id]');
    const groupElement = target.closest<HTMLElement>('[data-sidebar-session-group-id], [data-sidebar-group-id]');
    const sessionId = sessionElement?.dataset.sidebarSessionId;
    const groupId = groupElement?.dataset.sidebarSessionGroupId ?? groupElement?.dataset.sidebarGroupId;
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

    if (sessionsById[sessionId]?.isPinned === true) {
      /*
       * CDXC:PinnedSessions 2026-06-02-19:53:
       * Pinned project-session reorder regressions can fail before dnd-kit
       * emits a session drag. Persist one pointer-down breadcrumb for pinned
       * rows so support can distinguish "drag never started" from "drop guard
       * skipped sync" without logging titles, paths, commands, or user text.
       */
      postPinnedSessionReorderLog('pointerDown', {
        groupCollapsed: collapsedGroupsById[groupId] === true,
        pointer: summarizePointerEventForPinnedReorder(event),
        state: createPinnedSessionReorderDebugState(
          { groupId, kind: 'session', sessionId },
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById
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

    window.addEventListener('pointerdown', handlePointerDown, true);
    window.addEventListener('keydown', handleKeyDown, true);

    return () => {
      window.removeEventListener('pointerdown', handlePointerDown, true);
      window.removeEventListener('keydown', handleKeyDown, true);
    };
  }, [recordPointerDownSessionTarget, unlockCompletionSoundPlayback]);

  const { handleDragEnd, handleDragMove, handleDragOver, handleDragStart } = useSidebarDragHandlers({
    authoritativeSessionIdsByGroup,
    collapsedGroupsById,
    displayedProjectCollectionItems,
    effectiveSessionIdsByGroup,
    enableProjectCollections,
    groupIdsRef,
    groupsById,
    isManualActiveSessionsSort,
    moveRemoteMachineSection,
    pinnedSessionDropTargetLogKeyRef,
    pointerDownSessionTargetRef,
    postPinnedSessionReorderLog,
    postSidebarDebugLog,
    projectCollectionIdByProjectId,
    projectCollections,
    remoteMachines,
    remoteProjectGroupIdsByMachineId,
    sessionIdsByGroupRef,
    sessionPointerDragStateRef,
    sessionsById,
    setGroupDragPreview,
    setGroupDropIndicator,
    setIsProjectReorderDragActive,
    setPinnedSessionDropIndicator,
    setProjectCollectionDragPreview,
    setProjectCollectionDropIndicator,
    setProjectCollections,
    setProjectUngroupDropIndicatorScopeId,
    setRemoteMachineDragPreview,
    setRemoteMachineDropIndicator,
    setSessionDropIndicator,
    vscode,
  });

  const {
    closeSessionSearch,
    openCommandPalette,
    openHotkeys,
    openKeepAwakePowerSettings,
    openSidebarSettings,
    startSidebarKeepAwake,
    stopSidebarKeepAwake,
  } = useSidebarOverlayActions({
    setIsPreviousSessionsOpen,
    setIsSessionSearchOpen,
    setIsSessionSearchSelectionVisible,
    setSessionSearchQuery,
    setSidebarKeepAwakeRuntime,
    settings,
    vscode,
  });

  const closeTopmostSidebarOverlay = useEffectEvent(() => {
    if (gitCommitDraft) {
      closeGitCommitModal(gitCommitDraft.requestId);
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

    if (isSessionSearchOpen) {
      closeSessionSearch();
      return true;
    }

    return false;
  });

  const restoreSearchedPreviousSession = (historyId: string) => {
    vscode.postMessage({
      historyId,
      type: 'restorePreviousSession',
    });
    closeSessionSearch();
  };

  const deleteSearchedPreviousSession = (historyId: string) => {
    vscode.postMessage({
      historyId,
      type: 'deletePreviousSession',
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
        isSessionSearchSelectionVisible && selectedSessionSearchResult?.kind === 'previous'
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

    if (selectedSessionSearchResult.kind === 'previous') {
      restoreSearchedPreviousSession(selectedSessionSearchResult.historyId);
      return true;
    }

    const selectedResult = sidebarSessionSearchResults.find((result) =>
      isSidebarSessionSearchSelectionMatch(result, selectedSessionSearchResult)
    );
    if (!selectedResult || selectedResult.kind !== 'session') {
      return false;
    }

    dismissAppModalForSidebarNavigation('SettingsDismissal:sessionSearchActivate');
    useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    applyLocalFocus(selectedResult.groupId, selectedResult.sessionId);
    vscode.postMessage({
      sessionId: selectedResult.sessionId,
      type: 'focusSession',
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

      if (event.key === 'Escape') {
        if (isSearchInputTarget && sessionSearchQuery.length > 0) {
          event.preventDefault();
          event.stopPropagation();
          setSessionSearchQuery('');
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

      const commandPaletteHotkeyActionId = getCommandPaletteHotkeyActionId(event, settings?.hotkeys);
      if (commandPaletteHotkeyActionId && !hasActiveSidebarHotkeyRecorder()) {
        event.preventDefault();
        event.stopPropagation();
        if (commandPaletteHotkeyActionId === 'openCommandPalette') {
          openCommandPalette();
        } else {
          openPreviousSessions();
        }
        return;
      }

      if (
        event.defaultPrevented ||
        gitCommitDraft !== undefined ||
        isPreviousSessionsOpen ||
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
        event.key === 'Enter' &&
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

    document.addEventListener('keydown', handleKeyDown, true);
    return () => {
      document.removeEventListener('keydown', handleKeyDown, true);
    };
  }, [
    activateSelectedSessionSearchResult,
    closeTopmostSidebarOverlay,
    gitCommitDraft,
    isPreviousSessionsOpen,
    isSessionSearchOpen,
    selectedSessionSearchResult,
    sessionSearchQuery,
    sidebarSessionSearchResults,
  ]);

  const {
    createReferenceAgentChat,
    moveSidebar,
    openAddProjectModal,
    openConfigureAgentsModal,
    openPreviousSessions,
    openReferenceAgentsHub,
    openReferenceAutomations,
    openReferenceRemoteSetup,
    searchPreviousSessionsByPrompt,
    setActiveSessionsSortMode,
    toggleActiveSessionsSortMode,
    toggleSessionTagFilter,
    toggleSidebarCollapsed,
  } = useSidebarActions({
    activeSessionsSortMode,
    dismissAppModalForSidebarNavigation,
    displayedReferenceChatGroupIds,
    effectiveSessionIdsByGroup,
    enabledVisibleSidebarSessionTagSet,
    setIsPreviousSessionsOpen,
    setIsSessionSearchOpen,
    setIsSessionSearchSelectionVisible,
    setPrimaryAgentLauncherId,
    setSelectedSessionTagFilters,
    setSessionSearchQuery,
    vscode,
    workspaceGroupIds,
  });
  /*
   * CDXC:SidebarProjectMenu 2026-09-02:
   * The Projects header and the remote machine headers no longer exist. Their
   * actions sit at the top of the More dropdown and follow the machine tab
   * strip: whichever machine is selected is the one Add Project, Sort & Filter,
   * and Collapse All / Expand Previous act on. Add Project is withheld for a
   * remote machine that is not connected, exactly as its header used to do.
   */
  const selectedMachineProjectMenu: SidebarReferenceProjectMenu = (() => {
    const shared = {
      activeSessionsSortMode,
      onSetActiveSessionsSortMode: setActiveSessionsSortMode,
      onToggleSessionTagFilter: toggleSessionTagFilter,
      selectedSessionTagFilters: activeSelectedSessionTagFilters,
      sessionTagListItems: sidebarSessionTagListItems,
    };
    if (selectedRemoteMachineId === undefined) {
      return {
        ...shared,
        bulkActionLabel:
          displayedReferenceProjectGroupIds.length > 0
            ? hasExpandedReferenceProjects
              ? 'Collapse All'
              : 'Expand Previous'
            : undefined,
        onAddProject: () => openAddProjectModal(),
        onBulkProjectToggle: displayedReferenceProjectGroupIds.length > 0 ? toggleReferenceProjectsBulk : undefined,
        onToggleShowHidden: () => setShowHiddenSidebarItems((current) => !current),
        showHidden: showHiddenSidebarItems,
      };
    }
    const machineId = selectedRemoteMachineId;
    const machineProjectGroupIds = remoteProjectGroupIdsByMachineId[machineId] ?? [];
    const isConnected = remoteMachineRuntimeStatuses[machineId] === 'connected';
    return {
      ...shared,
      bulkActionLabel:
        machineProjectGroupIds.length > 0
          ? machineProjectGroupIds.some((groupId) => collapsedGroupsById[groupId] !== true)
            ? 'Collapse All'
            : 'Expand Previous'
          : undefined,
      onAddProject: isConnected ? () => openAddProjectModal(machineId) : undefined,
      onBulkProjectToggle:
        machineProjectGroupIds.length > 0 ? () => toggleRemoteMachineProjectsBulk(machineId) : undefined,
      onEditRemoteMachine: () => {
        dismissAppModalForSidebarNavigation('SettingsDismissal:remoteEditSettings');
        openAppModal({
          initialRemoteMachineId: machineId,
          initialTab: 'remote',
          modal: 'settings',
          type: 'open',
        });
      },
    };
  })();

  const renderReferenceProjectGroup = (groupId: string) => {
    const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
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
        onToggleSpaceMembership={
          spacesState && projectId ? (spaceId) => toggleLocalSpaceProjectMembership(spaceId, projectId) : undefined
        }
        spaceMemberProjectId={projectId}
        spaces={spacesState}
        onHideGroup={() =>
          setHiddenGroupIds((current) =>
            current.includes(groupId) ? current.filter((id) => id !== groupId) : [...current, groupId]
          )
        }
        onSessionSelectionChange={handleSidebarSessionSelectionChange}
        orderedSessionIds={displayedWorkspaceSessionIdsByGroup[groupId] ?? []}
        pinnedSessionDropIndicator={pinnedSessionDropIndicator}
        projectCollectionId={projectId ? projectCollectionIdByProjectId.get(projectId) : undefined}
        projectCollectionOptions={enableProjectCollections ? projectCollections.collections : undefined}
        projectSessionListCollapsedState={collapsedProjectSessionListsById}
        revealSessionRequest={sessionRevealRequest}
        selectedSearchSessionId={
          isSessionSearchSelectionVisible && selectedSessionSearchResult?.kind === 'session'
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
    <SidebarTooltipDelayProvider delayMs={effectiveSettings.sidebarTooltipDelayMs}>
      <SidebarCollapseAnimationProvider durationMs={effectiveSettings.sidebarCollapseAnimationDurationMs}>
        <div
          className='sidebar-reference-layout'
          data-project-reorder-drag={String(isProjectReorderDragActive)}
          data-project-group-style={effectiveSettings.sidebarProjectGroupStyle}
          data-reference-sidebar='true'
          data-sidebar-machine-tabs={String(remoteMachines.length > 0)}
          data-sidebar-side={effectiveSettings.sidebarSide}
          data-session-agent-icon-color-mode={effectiveSettings.useColoredSessionAgentIcons ? 'colored' : 'monochrome'}
          ref={setReferenceLayoutElement}
          style={
            {
              '--sidebar-collapse-duration': `${effectiveSettings.sidebarCollapseAnimationDurationMs}ms`,
            } as CSSProperties
          }
        >
          {showCommandHotkeyOverlay ? <SidebarHotkeyOverlay hotkeys={settings?.hotkeys} /> : null}
          <SidebarReferenceTopChrome
            keepAwakeRuntime={sidebarKeepAwakeRuntime}
            onOpenAgentsHub={openReferenceAgentsHub}
            onOpenAutomations={openReferenceAutomations}
            onOpenDiscord={() => {
              vscode.postMessage({ type: 'openExternalUrl', url: GHOSTEX_DISCORD_URL });
            }}
            onOpenHotkeys={openHotkeys}
            onOpenRemoteSetup={openReferenceRemoteSetup}
            onOpenPowerSettings={openKeepAwakePowerSettings}
            onOpenPreviousSessions={openPreviousSessions}
            onOpenSettings={openSidebarSettings}
            onRunKeepAwake={startSidebarKeepAwake}
            onSearchPreviousSessionsByPrompt={searchPreviousSessionsByPrompt}
            onSearch={openPreviousSessions}
            onStopKeepAwake={stopSidebarKeepAwake}
            onTogglePetOverlay={() => {
              vscode.postMessage({ type: 'togglePetOverlay' });
            }}
            machineTabs={
              remoteMachines.length > 0 ? (
                <SidebarMachineTabs
                  items={machineTabItems}
                  onConfigureMachines={() => {
                    dismissAppModalForSidebarNavigation('SettingsDismissal:remoteConfigureMachines');
                    openAppModal({ initialTab: 'remote', modal: 'settings', type: 'open' });
                  }}
                  onHideMachine={hideRemoteMachine}
                  onSelectMachineTab={setSelectedMachineTabId}
                  selectedMachineTabId={selectedRemoteMachineId ?? LOCAL_SIDEBAR_MACHINE_TAB_ID}
                  vscode={vscode}
                />
              ) : undefined
            }
            projectMenu={selectedMachineProjectMenu}
            settings={effectiveSettings}
            showKeepAwakeButton={showSidebarKeepAwakeButton}
          />
          <div
            className='stack'
            data-dimmed={String(isStartupInteractionBlocked)}
            data-sidebar-custom-theme={String(Boolean(normalizeWorkspaceThemeColor(customThemeColor)))}
            data-sidebar-theme={theme}
            onClickCapture={handleSidebarClickCapture}
            onDoubleClick={handleSidebarDoubleClick}
          >
            <section
              className='session-groups-panel'
              data-sidebar-machine-tabs={String(remoteMachines.length > 0)}
              ref={sessionGroupsPanelRef}
            >
              {/*
               * CDXC:SidebarMachineTabs 2026-08-28:
               * The machine strip is pinned above the scrolling project list so
               * switching machines never depends on scrolling to the bottom of
               * the list. CDXC:SidebarMachineTabsEdge 2026-09-02: it renders in
               * the top chrome (see SidebarReferenceTopChrome) because this
               * panel clips to the sidebar gutters; the panel's own top row
               * stays empty and hidden.
               */}
              <div className='session-groups-top' />
              {/*
            CDXC:SidebarScroll 2026-06-30-01:59:
            The sidebar's project list must scroll as fast as the browser can move it.
            Do not apply the vertical scroll mask or sticky-header gradient geometry here; the user explicitly accepts losing those visual fades to remove scroll-linked paint work.
          */}
              <div
                className='session-groups-scroll-shell'
                data-scrollable-y={String(sessionGroupsHaveScrollableOverflow)}
              >
                <div
                  className='session-groups-content'
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
                    {
                      <>
                        {/*
                         * CDXC:SidebarSpaces 2026-08-27:
                         * The local gxserver's Space row sits between its section
                         * header and its project list. It renders only once the
                         * local daemon has delivered a Space state — a daemon that
                         * never does is Space-incapable, and gets no row and no
                         * filtering rather than an empty control that cannot work.
                         */}
                        {!shouldHideReferenceSectionsForSearchEmptyState && isLocalMachineTabSelected && spacesState ? (
                          <SpaceFilterRow
                            collapsed={false}
                            onReorderSpaces={reorderLocalSpaces}
                            onSelectSpace={(spaceId) => selectSidebarSpace(LOCAL_SIDEBAR_SPACE_SECTION_KEY, spaceId)}
                            sectionKey={LOCAL_SIDEBAR_SPACE_SECTION_KEY}
                            selectedSpaceId={selectedLocalSpace?.spaceId}
                            spaces={spacesState}
                            vscode={vscode}
                          />
                        ) : null}
                        {!shouldHideReferenceSectionsForSearchEmptyState && isLocalMachineTabSelected ? (
                          <div
                            className='group-list workspace-group-list reference-project-group-list'
                            data-sidebar-space-content-section={LOCAL_SIDEBAR_SPACE_SECTION_KEY}
                            data-sidebar-project-list-scope={LOCAL_PROJECT_LIST_SCOPE_ID}
                          >
                            {displayedReferenceProjectGroupIds.length > 0 ? (
                              <>
                                {displayedProjectCollectionItems.map((item, itemIndex) =>
                                  item.kind === 'project' ? (
                                    renderReferenceProjectGroup(item.groupId)
                                  ) : (
                                    <ProjectCollectionSection
                                      autoEdit={autoEditingProjectCollectionId === item.collection.collectionId}
                                      bulkProjectActionLabel={
                                        item.groupIds.some((groupId) => collapsedGroupsById[groupId] !== true)
                                          ? 'Collapse All'
                                          : 'Expand Previous'
                                      }
                                      collapsed={
                                        !isSessionSearchFiltering &&
                                        collapsedProjectCollectionsByKey[
                                          createLocalProjectCollectionCollapseKey(item.collection.collectionId)
                                        ] === true
                                      }
                                      collection={item.collection}
                                      containsActiveSession={item.groupIds.some((groupId) =>
                                        groupIdsContainingActiveSession.has(groupId)
                                      )}
                                      draggingDisabled={isSessionSearchOpen}
                                      dropIndicatorPosition={
                                        projectCollectionDropIndicator?.collectionId === item.collection.collectionId
                                          ? projectCollectionDropIndicator.position
                                          : undefined
                                      }
                                      index={itemIndex}
                                      isDragPreviewSource={
                                        projectCollectionDragPreview?.collectionId === item.collection.collectionId
                                      }
                                      isHidden={hiddenCollectionKeys.includes(`local:${item.collection.collectionId}`)}
                                      key={item.collection.collectionId}
                                      onAutoEditHandled={() => setAutoEditingProjectCollectionId(undefined)}
                                      onBulkProjectToggle={() => {
                                        const hasExpandedProjects = item.groupIds.some(
                                          (groupId) => collapsedGroupsById[groupId] !== true
                                        );
                                        if (hasExpandedProjects) {
                                          previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                            item.collection.collectionId
                                          ] = item.groupIds.filter((groupId) => collapsedGroupsById[groupId] !== true);
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
                                          false
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
                                            })
                                          )
                                        );
                                      }}
                                      onCollapsedChange={(collapsed) =>
                                        setProjectCollectionCollapsed(
                                          createLocalProjectCollectionCollapseKey(item.collection.collectionId),
                                          collapsed
                                        )
                                      }
                                      onDelete={() => {
                                        setProjectCollections((previous) =>
                                          removeSidebarProjectCollection(previous, item.collection.collectionId)
                                        );
                                      }}
                                      onHide={() => {
                                        const collectionKey = `local:${item.collection.collectionId}`;
                                        setHiddenCollectionKeys((current) =>
                                          current.includes(collectionKey)
                                            ? current.filter((key) => key !== collectionKey)
                                            : [...current, collectionKey]
                                        );
                                      }}
                                      onSelectSessions={setSelectedSidebarSessionIds}
                                      onToggleSpaceMembership={
                                        spacesState
                                          ? (spaceId) =>
                                              toggleLocalSpaceCollectionMembership(
                                                spaceId,
                                                item.collection.collectionId
                                              )
                                          : undefined
                                      }
                                      sessionIds={item.groupIds.flatMap(
                                        (groupId) => effectiveSessionIdsByGroup[groupId] ?? []
                                      )}
                                      sessionTagListItems={sidebarSessionTagListItems}
                                      sessionsById={sessionsById}
                                      spaces={spacesState}
                                      vscode={vscode}
                                    >
                                      {item.groupIds.map(renderReferenceProjectGroup)}
                                    </ProjectCollectionSection>
                                  )
                                )}
                                <ProjectListEndUngroupDropZone
                                  active={projectUngroupDropIndicatorScopeId === LOCAL_PROJECT_LIST_SCOPE_ID}
                                  scopeId={LOCAL_PROJECT_LIST_SCOPE_ID}
                                />
                              </>
                            ) : (
                              referenceProjectsEmptyState
                            )}
                          </div>
                        ) : null}
                        {!shouldHideReferenceSectionsForSearchEmptyState && selectedRemoteMachineId !== undefined ? (
                          <div className='reference-remote-section-list'>
                            {/*
                             * CDXC:SidebarMachineTabs 2026-08-28:
                             * Only the machine selected in the top tab strip
                             * renders. Machines are no longer stacked sections
                             * pinned under the local project list.
                             *
                             * CDXC:RemoteMachines 2026-06-02-23:47:
                             * Saved Remote machines render as peer sidebar sections beside local Projects. Until the SSH/gxserver connection is active, each machine remains visible and exposes Reload instead of Add Project.
                             *
                             * CDXC:RemoteMachines 2026-06-09-19:02:
                             * Remote machine section rows must collapse like Quick and Projects and use the same section-header styling, including the visible chevron and hover actions.
                             */}
                            {remoteMachines
                              .filter((machine) => machine.id === selectedRemoteMachineId)
                              .map((machine, index) => {
                                /*
                                 * CDXC:RemoteProjectCollections 2026-07-21:
                                 * Remote machine sections render the same collection
                                 * panels as local Projects. Assigning a remote project
                                 * to a group previously updated state with no visible
                                 * result because remote lists were always flat.
                                 */
                                const machineProjectGroupIds = remoteProjectGroupIdsByMachineId[machine.id] ?? [];
                                const machineProjectCollections = remoteProjectCollectionsByMachineId[machine.id] ?? {
                                  collections: [],
                                  nextCollectionNumber: 1,
                                };
                                const machineSpaces = remoteSpacesByMachineId[machine.id];
                                const machineSelectedSpace = resolveSelectedSidebarSpace(
                                  machineSpaces,
                                  selectedSpaceIdBySectionKey[createRemoteSidebarSpaceSectionKey(machine.id)]
                                );
                                const machineCollectionIdByProjectId = createProjectCollectionIdByProjectId(
                                  machineProjectCollections,
                                  machineProjectGroupIds,
                                  groupsById,
                                  (groupId) => groupsById[groupId]?.remoteMachineContext?.projectId
                                );
                                const machineCollectionItems = enableProjectCollections
                                  ? buildProjectCollectionRenderItems(
                                      machineProjectGroupIds,
                                      machineProjectCollections,
                                      (groupId) => groupsById[groupId]?.remoteMachineContext?.projectId
                                    )
                                  : undefined;
                                const renderRemoteProjectGroup = (groupId: string, groupIndex: number) => {
                                  /*
                                   * CDXC:SidebarSpaces 2026-08-27:
                                   * A remote daemon stores membership under ITS
                                   * own project id, which is the raw id on the
                                   * group's remote context — not the sidebar's
                                   * machine-scoped editor project id.
                                   */
                                  const rawProjectId = groupsById[groupId]?.remoteMachineContext?.projectId;
                                  return (
                                    <SessionGroupSection
                                      autoEdit={false}
                                      canClose={!groupsById[groupId]?.projectContext}
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
                                                machineProjectGroupIds
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
                                                machineProjectGroupIds
                                              )
                                          : undefined
                                      }
                                      onProjectSessionListCollapsedChange={setProjectSessionListCollapsed}
                                      onToggleSpaceMembership={
                                        machineSpaces && rawProjectId
                                          ? (spaceId) =>
                                              toggleRemoteSpaceProjectMembership(machine.id, spaceId, rawProjectId)
                                          : undefined
                                      }
                                      spaceMemberProjectId={rawProjectId}
                                      spaces={machineSpaces}
                                      onHideGroup={() =>
                                        setHiddenGroupIds((current) =>
                                          current.includes(groupId)
                                            ? current.filter((id) => id !== groupId)
                                            : [...current, groupId]
                                        )
                                      }
                                      onSessionSelectionChange={handleSidebarSessionSelectionChange}
                                      orderedSessionIds={displayedWorkspaceSessionIdsByGroup[groupId] ?? []}
                                      enableProjectSessionListToggle={!isSessionSearchFiltering}
                                      projectHeaderActions='all'
                                      projectSessionListCollapsedState={collapsedProjectSessionListsById}
                                      projectCollectionId={
                                        groupsById[groupId]?.remoteMachineContext?.projectId
                                          ? machineCollectionIdByProjectId.get(
                                              groupsById[groupId]!.remoteMachineContext!.projectId!
                                            )
                                          : undefined
                                      }
                                      projectCollectionOptions={
                                        enableProjectCollections ? machineProjectCollections.collections : undefined
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
                                };
                                return (
                                  <RemoteMachineSidebarSection
                                    index={index}
                                    key={machine.id}
                                    machine={machine}
                                    isDragPreviewSource={remoteMachineDragPreview?.machineId === machine.id}
                                    remoteMachineDropIndicatorPosition={
                                      remoteMachineDropIndicator?.remoteMachineId === machine.id
                                        ? remoteMachineDropIndicator.position
                                        : undefined
                                    }
                                    onReorderSpaces={(orderedSpaceIds) =>
                                      reorderRemoteSpaces(machine.id, orderedSpaceIds)
                                    }
                                    onSelectSpace={(spaceId) =>
                                      selectSidebarSpace(createRemoteSidebarSpaceSectionKey(machine.id), spaceId)
                                    }
                                    selectedSpaceId={machineSelectedSpace?.spaceId}
                                    spaces={machineSpaces}
                                    projectCollectionItems={machineCollectionItems}
                                    projectUngroupDropIndicatorScopeId={projectUngroupDropIndicatorScopeId}
                                    projectGroupIds={machineProjectGroupIds}
                                    renderProjectCollection={(item, itemIndex) => (
                                      <ProjectCollectionSection
                                        autoEdit={
                                          autoEditingProjectCollectionId ===
                                          `${machine.id}:${item.collection.collectionId}`
                                        }
                                        bulkProjectActionLabel={
                                          item.groupIds.some((groupId) => collapsedGroupsById[groupId] !== true)
                                            ? 'Collapse All'
                                            : 'Expand Previous'
                                        }
                                        collapsed={
                                          !isSessionSearchFiltering &&
                                          collapsedProjectCollectionsByKey[
                                            createRemoteProjectCollectionCollapseKey(
                                              machine.id,
                                              item.collection.collectionId
                                            )
                                          ] === true
                                        }
                                        collection={item.collection}
                                        containsActiveSession={item.groupIds.some((groupId) =>
                                          groupIdsContainingActiveSession.has(groupId)
                                        )}
                                        draggingDisabled={true}
                                        index={itemIndex}
                                        isHidden={hiddenCollectionKeys.includes(
                                          `remote:${machine.id}:${item.collection.collectionId}`
                                        )}
                                        key={`${machine.id}:${item.collection.collectionId}`}
                                        onAutoEditHandled={() => setAutoEditingProjectCollectionId(undefined)}
                                        onBulkProjectToggle={() => {
                                          const bulkToggleKey = `${machine.id}:${item.collection.collectionId}`;
                                          const hasExpandedProjects = item.groupIds.some(
                                            (groupId) => collapsedGroupsById[groupId] !== true
                                          );
                                          if (hasExpandedProjects) {
                                            previousExpandedProjectGroupIdsByCollectionIdRef.current[bulkToggleKey] =
                                              item.groupIds.filter((groupId) => collapsedGroupsById[groupId] !== true);
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
                                            false
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
                                              })
                                            )
                                          );
                                        }}
                                        onCollapsedChange={(collapsed) =>
                                          setProjectCollectionCollapsed(
                                            createRemoteProjectCollectionCollapseKey(
                                              machine.id,
                                              item.collection.collectionId
                                            ),
                                            collapsed
                                          )
                                        }
                                        onDelete={() => {
                                          updateRemoteProjectCollections(machine.id, (previous) =>
                                            removeSidebarProjectCollection(previous, item.collection.collectionId)
                                          );
                                        }}
                                        onHide={() => {
                                          const collectionKey = `remote:${machine.id}:${item.collection.collectionId}`;
                                          setHiddenCollectionKeys((current) =>
                                            current.includes(collectionKey)
                                              ? current.filter((key) => key !== collectionKey)
                                              : [...current, collectionKey]
                                          );
                                        }}
                                        onSelectSessions={setSelectedSidebarSessionIds}
                                        onToggleSpaceMembership={
                                          machineSpaces
                                            ? (spaceId) =>
                                                toggleRemoteSpaceCollectionMembership(
                                                  machine.id,
                                                  spaceId,
                                                  item.collection.collectionId
                                                )
                                            : undefined
                                        }
                                        sessionIds={item.groupIds.flatMap(
                                          (groupId) => effectiveSessionIdsByGroup[groupId] ?? []
                                        )}
                                        sessionTagListItems={sidebarSessionTagListItems}
                                        sessionsById={sessionsById}
                                        sortableId={`remote-project-collection:${machine.id}:${item.collection.collectionId}`}
                                        remoteMachineId={machine.id}
                                        spaces={machineSpaces}
                                        vscode={vscode}
                                      >
                                        {item.groupIds.map((groupId) =>
                                          renderRemoteProjectGroup(groupId, machineProjectGroupIds.indexOf(groupId))
                                        )}
                                      </ProjectCollectionSection>
                                    )}
                                    renderProjectGroup={renderRemoteProjectGroup}
                                    status={remoteMachineRuntimeStatuses[machine.id] ?? 'disconnected'}
                                    vscode={vscode}
                                  />
                                );
                              })}
                          </div>
                        ) : null}
                        {previousSessionsSearchGroup}
                        {shouldShowSessionSearchEmptyState ? (
                          <div
                            className='group-empty-drop-target session-search-empty-drop-target'
                            data-empty-space-blocking='true'
                          >
                            <div className='group-empty-state session-search-empty-state'>
                              No current sessions or sessions to reopen match that search.
                            </div>
                          </div>
                        ) : displayedWorkspaceGroupIds.every(
                            (groupId) => (displayedWorkspaceSessionIdsByGroup[groupId] ?? []).length === 0
                          ) && !isSessionSearchOpen ? (
                          <div className='empty' data-empty-space-blocking='true'></div>
                        ) : null}
                      </>
                    }
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
                      ? createPortal(<ProjectGroupDragGhost preview={groupDragPreview} />, referenceLayoutElement)
                      : null}
                    {projectCollectionDragPreview && referenceLayoutElement
                      ? createPortal(
                          <ProjectCollectionDragGhost preview={projectCollectionDragPreview} />,
                          referenceLayoutElement
                        )
                      : null}
                    {remoteMachineDragPreview && referenceLayoutElement
                      ? createPortal(
                          <RemoteMachineDragGhost preview={remoteMachineDragPreview} />,
                          referenceLayoutElement
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
                  confirmLabel: 'Commit',
                  description: '',
                  changedFiles: [],
                  requestId: '',
                  showCommitMessage: true,
                  suggestedBody: undefined,
                  suggestedSubject: '',
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
                  type: 'confirmSidebarGitCommit',
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
                  type: 'confirmSidebarGitDirectMerge',
                });
              }}
              onMultipleCommits={(requestId, agentId) => {
                setGitCommitDraft(undefined);
                setGitFileDiffDraft(undefined);
                vscode.postMessage({ agentId, requestId, type: 'runSidebarGitMultipleCommits' });
              }}
              onOpenFileDiff={(filePath, requestId) => {
                vscode.postMessage({ filePath, requestId, type: 'openSidebarGitChangedFileDiff' });
              }}
              theme={theme}
            />
            {buildStamp ? (
              <AppTooltip content='Copy build stamp'>
                <button
                  aria-label={`Copy build stamp ${buildStamp}`}
                  className='copy-cursor'
                  onClick={() => {
                    void navigator.clipboard.writeText(buildStamp).catch(() => {});
                  }}
                  style={DEBUG_BUILD_STAMP_STYLE}
                  type='button'
                >
                  {buildStamp}
                </button>
              </AppTooltip>
            ) : null}
          </div>
          <SidebarReferenceFooter
            commandPaletteHotkey={formatSidebarMenuHotkeyLabel(
              normalizeghostexHotkeySettings(effectiveSettings.hotkeys).openCommandPalette
            )}
            onOpenQuickAccess={openCommandPalette}
          />
        </div>
      </SidebarCollapseAnimationProvider>
    </SidebarTooltipDelayProvider>
  );
}
