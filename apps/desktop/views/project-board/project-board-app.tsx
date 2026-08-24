import {
  IconAdjustmentsHorizontal,
  IconLayoutColumns,
  IconLoader2,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconX,
} from '@tabler/icons-react';
import { DragDropProvider } from '@dnd-kit/react';
import { useCallback, useEffect, useMemo, useRef, useState, type ComponentProps } from 'react';
import { Toaster, toast } from 'sonner';
import { Button } from '@/packages/components/ui/button';
import { isDiagnosticLoggingScenarioEnabled } from '@/packages/shared/ghostex-settings';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/packages/components/ui/dropdown-menu';
import { Input } from '@/packages/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import {
  PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY,
  addBoardColumn,
  beginBoardColumnRename,
  beadsErrorMessage,
  beadsStatusToBoardStatus,
  boardStatusBeadsValue,
  boardTagFilterOptions,
  buildAgentWorkPrompt,
  buildBoardColumns,
  moveBoardColumn,
  removeBoardColumn,
  conversationLinkActionKind,
  ensureIssuePrefix,
  ensureWorkflowStatuses,
  extractPreviewableDescriptionImageReferences,
  filterBoardTickets,
  formatProjectBoardCommentText,
  getBlockedByIds,
  getBlockingIds,
  normalizeBeadsPayload,
  normalizeDisplayIssueKey,
  normalizeIssuePrefix,
  parseBeadsJson,
  parseBeadsRejection,
  prioritySelectValue,
  projectBoardRawProjectIdFromUrlParam,
  readWorkflowStatuses,
  resolveAssignedAgentId,
  resolveBoardTagFilter,
  sortBoardTickets,
  tshirtToEstimate,
  toBoardTickets,
  estimateToTshirt,
  type BeadsBridgeRequest,
  type BoardColumn,
  type BoardEstimateFilter,
  type BoardPriorityFilter,
  type BoardSortOption,
  type BoardTagFilter,
  type ProjectBoardCommentMetadata,
  type BeadsIssue,
  type BoardStatusKey,
  type BoardTicket,
} from '../project-board-shared';
import {
  indexBeadConversationLinksByBead,
  selectBeadConversationLinks,
  type ProjectBoardConversationLinkView,
  type ProjectBoardConversationState,
  type ProjectBoardStartLocation,
} from '@/packages/shared/bead-conversation-links';
import type { AppToastLevel } from '@/packages/shared/app-toast-contract';
import {
  type AutomationDefinition,
  type AutomationRun,
  type ProjectAutomationsBridgeState,
} from '@/packages/shared/automations';
import { createSidebarAgentSelectItems } from '@/packages/shared/sidebar-agents';
import {
  type ConversationActionState,
  type DetailDraft,
  type TicketFormDraft,
  type PendingBoardStatusMove,
  type ProjectBoardFocusOwnerEvent,
  type BoardRefreshOptions,
  type ProjectBoardCommandCompletedEventDetail,
  type ProjectBoardRunnableCommandAction,
  type RunnableBeadsMigrationOption,
  projectBoardCommandRunKey,
  type ProjectSurfaceTab,
  type TicketContextMenuState,
} from './types';
import {
  PROJECT_BOARD_COMMAND_COMPLETED_EVENT,
  PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS,
  PROJECT_BOARD_MAX_DEPENDENCY_OPTIONS,
  NATIVE_SETTINGS_STORAGE_KEY,
  readExperimentalFeaturesEnabled,
  readProjectBoardViewPreferences,
  PROJECT_BOARD_PRIORITY_FILTER_SELECT_ITEMS,
  PROJECT_BOARD_ESTIMATE_FILTER_SELECT_ITEMS,
  PROJECT_BOARD_SORT_SELECT_ITEMS,
} from './constants';
import { sendBeadsRequest, sendProjectBoardRequest, sendProjectBoardImageRequest } from './bridge';
import {
  isProjectBoardEditableFocusTarget,
  postProjectBoardFocusOwnerChanged,
  createEmptyDetailDraft,
  createEmptyTicketFormDraft,
  createProjectBoardDraftTitle,
  boardColumnsSignature,
  applyPendingBoardStatusMoves,
  upsertProjectBoardIssue,
  upsertProjectBoardTicket,
  scheduleProjectBoardGeneratedTitle,
  waitForProjectBoardRefreshIdle,
  toCreatedBoardTicket,
  resolveCreatedIssueFromRefresh,
  stringifyProjectBoardDebugDetails,
  projectBoardTitleGenerationFailureDetails,
  projectBoardPromptAgentKind,
  createIssuesSignature,
  mergeKnownLabels,
  deriveKnownLabelsFromIssues,
  prioritizeDependencyTickets,
} from './board-state';
import {
  getPrimaryUsableConversationLink,
  projectBoardCommentMetadataFromLink,
  compareConversationLinksNewestFirst,
} from './ticket-detail';
import { BoardLane, ProjectBoardTicketContextMenu } from './board-lane-card';
import {
  selectAutomationRunsForTriage,
  AutomationComingSoonOverlay,
  AutomationDefinitionList,
  AutomationRunList,
  AutomationDefinitionDetail,
  AutomationRunDetail,
} from './automations';
import {
  AUTOMATION_SCHEDULE_PRESETS,
  AUTOMATION_WEEKDAY_OPTIONS,
  AUTOMATION_TIMER_UNIT_OPTIONS,
  type AutomationDraft,
  createAutomationDraft,
  resolveAutomationDraftAgentId,
  resolveAutomationDraftProjectId,
  createAutomationDraftFromDefinition,
  createAutomationDefinitionFromDraft,
} from './automations-drafts';
import { beadsRejectionToastId, formatIssueIdList, ProjectBoardNotice } from './remote-migrate-gate';
import { BoardColumnsDialog } from './board-columns-dialog';
import { AutomationDialog } from './automation-dialog';
import { EditTicketDialog, NewTicketDialog } from './ticket-dialogs';
import {
  BOARD_CARD_VIEW_FIELDS,
  BOARD_CARD_VIEW_STORAGE_KEY,
  loadBoardCardViewOptions,
  saveBoardCardViewOptions,
  type BoardCardViewOptions,
} from './card-view-options';

export type LoadState = 'idle' | 'loading' | 'ready' | 'error';

export type TicketDetailSaveDraft = Omit<DetailDraft, 'isDeleting' | 'isSaving' | 'ticket'> & {
  commentMetadata: ProjectBoardCommentMetadata;
  ticket: BoardTicket;
};

export const PROJECT_BOARD_FOCUS_OWNER_MIN_INTERVAL_MS = 250;

export function ProjectBoardApp() {
  const urlSearchParams = new URLSearchParams(window.location.search);
  const projectName = urlSearchParams.get('projectName') || 'Project';
  const projectPath = urlSearchParams.get('projectPath') || '';
  const projectIdParam = urlSearchParams.get('projectId') || '';
  const projectId = projectBoardRawProjectIdFromUrlParam(projectIdParam);
  const projectEditorId = urlSearchParams.get('projectEditorId') || projectIdParam;
  const remoteMachineId = urlSearchParams.get('remoteMachineId') || '';
  const automationScope = urlSearchParams.get('scope') === 'all' ? 'all' : 'project';
  const isAutomationGlobalScope = automationScope === 'all';
  const initialSurfaceTab: ProjectSurfaceTab =
    urlSearchParams.get('surface') === 'automations' ? 'automations' : 'board';
  const automationIsExperimental = urlSearchParams.get('automationExperimental') !== 'false';
  const [experimentalFeaturesEnabled, setExperimentalFeaturesEnabled] = useState(() =>
    readExperimentalFeaturesEnabled(urlSearchParams)
  );
  const automationSurfaceName = isAutomationGlobalScope ? 'Automations Overview' : 'Automate';
  const displayKey = normalizeDisplayIssueKey(urlSearchParams.get('beadsDisplayKey') ?? projectName);
  const issuePrefix = normalizeIssuePrefix(projectName || projectPath.split('/').filter(Boolean).at(-1) || displayKey);
  /*
   * CDXC:ProjectBoardCustomColumns 2026-08-21:
   * Lanes are the board's own bd statuses, which are only known once ensureWorkflowStatuses has read
   * the config, so the board opens on the six built-in lanes and adopts the extras on the first load.
   * The columns are mirrored into a ref because the refresh callback maps issue statuses onto them:
   * reading them from state instead would rebuild loadTickets, and with it restart the auto-refresh
   * interval, every time a refresh produced a fresh columns array.
   */
  const boardColumnsRef = useRef<BoardColumn[]>(buildBoardColumns(''));
  const [boardColumns, setBoardColumns] = useState<BoardColumn[]>(boardColumnsRef.current);
  /*
   * CDXC:ProjectBoardColumnManagement 2026-08-21:
   * Column management writes the same config string it read, so the raw value is kept rather than
   * rebuilt from the derived columns: the derived list has already dropped each entry's bd category
   * suffix, and regenerating the config from it would silently strip categories the board relies on.
   */
  const boardColumnConfigRef = useRef('');
  const [boardColumnConfig, setBoardColumnConfig] = useState('');
  const [columnsDialogOpen, setColumnsDialogOpen] = useState(false);
  /*
   * CDXC:ProjectBoardRedesign 2026-08-24:
   * Card-detail visibility is one app-wide preference shared by every
   * project's board: it loads from localStorage on mount, saves on every
   * toggle, and follows cross-window storage events so all open boards match.
   */
  const [cardView, setCardView] = useState<BoardCardViewOptions>(loadBoardCardViewOptions);
  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === BOARD_CARD_VIEW_STORAGE_KEY) {
        setCardView(loadBoardCardViewOptions());
      }
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);
  const toggleCardViewField = useCallback((key: keyof BoardCardViewOptions, value: boolean) => {
    setCardView((current) => {
      const next = { ...current, [key]: value };
      saveBoardCardViewOptions(next);
      return next;
    });
  }, []);
  const [tickets, setTickets] = useState<BoardTicket[]>([]);
  const [allIssues, setAllIssues] = useState<BeadsIssue[]>([]);
  const [knownLabels, setKnownLabels] = useState<string[]>([]);
  const [conversationState, setConversationState] = useState<ProjectBoardConversationState>({
    agents: [],
    debuggingMode: false,
    links: [],
    sessions: [],
  });
  const [selectedAgentId, setSelectedAgentId] = useState('');
  const pickedAgentIdByBeadIdRef = useRef(new Map<string, string>());
  const [loadState, setLoadState] = useState<LoadState>('idle');
  const [hasCompletedInitialBoardLoad, setHasCompletedInitialBoardLoad] = useState(false);
  const [runningProjectBoardCommand, setRunningProjectBoardCommand] = useState('');
  const [errorMessage, setErrorMessage] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const searchInputRef = useRef<HTMLInputElement>(null);
  const storedViewPreferences = useMemo(() => readProjectBoardViewPreferences(), []);
  const [priorityFilter, setPriorityFilter] = useState<BoardPriorityFilter>(storedViewPreferences.priorityFilter);
  const [estimateFilter, setEstimateFilter] = useState<BoardEstimateFilter>(storedViewPreferences.estimateFilter);
  const [tagFilter, setTagFilter] = useState<BoardTagFilter>(storedViewPreferences.tagFilter);
  const [sortOption, setSortOption] = useState<BoardSortOption>(storedViewPreferences.sortOption);
  const [detail, setDetail] = useState<DetailDraft>(createEmptyDetailDraft);
  const [newTicketOpen, setNewTicketOpen] = useState(false);
  const [newTicket, setNewTicket] = useState<TicketFormDraft>(createEmptyTicketFormDraft);
  const [newTicketStartLocation, setNewTicketStartLocation] = useState<ProjectBoardStartLocation>('currentProject');
  const createInFlightRef = useRef(false);
  const pendingStatusMovesRef = useRef(new Map<string, PendingBoardStatusMove>());
  const pendingStatusMoveSerialRef = useRef(0);
  const [deleteConfirmingTicketId, setDeleteConfirmingTicketId] = useState('');
  const [ticketContextMenu, setTicketContextMenu] = useState<TicketContextMenuState>();
  const [contextMenuDeletingTicketId, setContextMenuDeletingTicketId] = useState('');
  const [imagePreviewDataUrls, setImagePreviewDataUrls] = useState<Record<string, string>>({});
  const pendingImagePreviewPathsRef = useRef(new Set<string>());
  const failedImagePreviewPathsRef = useRef(new Set<string>());
  const agentSelectItems = useMemo(
    () =>
      conversationState.agents.map((agent) => ({
        label: agent.label,
        value: agent.agentId,
      })),
    [conversationState.agents]
  );
  /*
   * CDXC:ProjectBoard 2026-05-26-05:38:
   * The Project page must observe Beads changes made by the user's app actions or nearby bd CLI commands without forcing manual Refresh.
   * Poll only while the page is visible, coalesce overlapping refreshes, refresh labels less often than issues, and cap mounted lane/dependency rows so thousand-bead projects do not repeatedly rebuild an unbounded DOM.
   *
   * CDXC:ProjectBoard 2026-05-26-10:08:
   * Creating a ticket with an empty title must not keep the modal blocked while the selected/default prompt agent generates a title.
   * Create the Beads issue first with an explicit "Generating title..." card title, close the modal, refresh the board, then replace that temporary title after generation finishes.
   *
   * CDXC:ProjectBoard 2026-05-26-10:08:
   * Users need to delete tickets from the Project board UI. Keep deletion in the edit dialog, require a second destructive click for confirmation, and refresh from Beads after bd deletes the issue.
   *
   * CDXC:ProjectBoard 2026-06-13-13:37:
   * Right-clicking a Kanban bead card must open a compact context menu with Start work and Delete actions.
   * Start work uses the same Project Board bridge route as the edit dialog, and Delete keeps the existing confirmed Beads delete path so the card menu does not create a second mutation route.
   *
   * CDXC:ProjectBoard 2026-05-26-10:16:
   * A bead can be linked to the agent conversation that is working on it, and multiple beads may point at one conversation.
   * Refresh conversation links alongside Beads data so card jump buttons keep tracking captured session metadata while the Project page stays open.
   *
   * CDXC:ProjectBoard 2026-05-26-10:20:
   * Conversation actions can launch terminals, focus sessions, or mutate persisted links.
   * Track the active action so duplicate clicks do not create duplicate agent sessions or race link/archive state while the board is responding.
   *
   * CDXC:ProjectBoard 2026-05-28-12:32:
   * New tickets need an explicit Create & Start path with agent selection and a current-project versus new-worktree start location.
   * The ticket is still created on the board project first so the agent prompt carries the real bead id, and project-page diagnostics are emitted only when the native.project.board scenario is enabled.
   *
   * CDXC:ProjectBoard 2026-05-28-16:21:
   * Ticket primary actions should reopen existing work before creating new work.
   * Treat live and previous-session-restorable conversation links as usable so "Start work" changes to "Go to Session" once a ticket already owns an openable agent conversation.
   * Keep the edit dialog open after Go to Session; focusing/restoring the session should reveal the workarea without discarding the user's ticket-editing context.
   *
   * CDXC:ProjectBoard 2026-05-31-07:30:
   * Create, Create & Start, and edit-ticket Start work must dismiss their dialog immediately on click so async Beads/create/start work never blocks the board behind the modal.
   * Do not swap Create button labels to "Creating…" while the dialog is open; that footer layout shift is visible before close.
   * Go to Session still keeps the edit dialog open; Start work closes it on click.
   *
   * CDXC:ProjectBoard 2026-05-31-08:05:
   * New-ticket start location is a dropdown beside the agent dropdown, matching its height and sitting to the right (not centered radio buttons).
   *
   * CDXC:ProjectBoard 2026-05-30-07:46:
   * Collapsed macOS Project-page selects must show friendly labels for agents and ticket priority while preserving the raw Beads-compatible values used by bridge requests.
   * Provide select item metadata at the root because the popup is not mounted before the collapsed value renders.
   *
   * CDXC:ProjectBoard 2026-05-30-08:59:
   * The edit-ticket Status select follows the same collapsed-label rule as Priority: show board status labels to users while keeping the stored board status key for Beads updates.
   *
   * CDXC:ProjectBoardFilters 2026-05-30-08:31:
   * The board toolbar should place the search icon inside the input at the left edge and replace the status dropdown with Priority and Estimate filters.
   * Toolbar selects use root item metadata so collapsed controls show friendly labels instead of raw filter values.
   *
   * CDXC:ProjectBoardFilters 2026-05-30-09:13:
   * The top Project-page filter controls and + Ticket action should share the search input height so the toolbar reads as one aligned control row.
   *
   * CDXC:ProjectBoardLaneCreation 2026-05-30-09:15:
   * Lane headers should expose a hover/focus + action in place of the ticket count so users can create a ticket directly in that workflow status.
   * Beads creates issues in Todo first, so non-Todo lane creation must immediately update the new issue status before refreshing the board or starting work.
   *
   * CDXC:ProjectBoardLaneHeader 2026-06-05-14:30:
   * The lane header action slot should sit 4px in from the right edge, keep ticket counts right-aligned within that slot, and place the hover + action 3px farther right than the count alignment.
   *
   * CDXC:ProjectBoard 2026-05-30-08:54:
   * Create & Start must launch the selected agent session from the created bead before optional label hydration or auto-title generation runs.
   * A generated title improves the board card later, but terminal creation and prompt submission must not wait on or be canceled by board refreshes or title generation failures.
   *
   * CDXC:ProjectBoard 2026-05-30-09:36:
   * The gxserver Beads create action can persist the issue while the board web surface still lacks a usable created-issue id.
   * Resolve the newly persisted bead from refreshed gxserver Beads data before dependency/status/label updates, title generation, or Create & Start so the terminal session is keyed to the real board ticket instead of silently skipping start.
   *
   * CDXC:ProjectBoard 2026-06-02-15:10:
   * Project Board Beads commands are gxserver-owned after the split. This React surface owns modal/form sequencing and bridge requests only; do not move bd command construction or subprocess execution back into the macOS sidebar.
   *
   * CDXC:ProjectBoard 2026-05-30-09:45:
   * Create & Start should hand the created bead to native session launch as soon as the bead id is available.
   * Board refresh, lane hydration, labels, dependencies, and generated title updates are secondary work and must not sit in front of terminal creation.
   *
   * CDXC:ProjectBoardBeads 2026-06-10-20:27:
   * New tickets must be created under the active project prefix, not a stale Beads issue-prefix value left by earlier gxserver-focused work.
   * Keep Beads issue_prefix reconciled from the project identity before initial/manual board reads and before create so new cards return in the same project board scope.
   *
   * CDXC:ProjectBoardForms 2026-06-09-15:36:
   * Typing in New automation, edit-ticket, or new-ticket fields must never blank the Project/Kanban page.
   * Snapshot input values before functional state updates because React clears event currentTarget after dispatch and delayed updaters cannot safely read from the event object.
   *
   * CDXC:ProjectBoardLocalFirst 2026-06-16-13:16:
   * Kanban create and drag/drop interactions must update the board from local React state as soon as Beads returns a bead id or the user drops a card.
   * Keep status moves, generated titles, dependency/label mutations, and full Beads refreshes as background reconciliation so the page stays responsive while durable storage catches up.
   *
   * CDXC:ProjectBoardLocalFirst 2026-06-16-20:01:
   * Generated titles must not run as immediate follow-up work after local ticket insertion.
   * Defer generated-title prompt-agent work to idle time, and keep label vocabulary local-first instead of starting global label reads after create.
   *
   * CDXC:ProjectBoardLabels 2026-06-19-09:35:
   * Kanban create, drag, initial render, manual refresh, and auto-refresh must not call Beads `label list-all`.
   * The board derives label suggestions from the already-loaded issue rows and merges labels from successful local mutations so global label inventory cannot block scrolling or card movement.
   *
   * CDXC:ProjectBoardLoading 2026-06-20-18:21:
   * The first time the macOS Kanban board opens, the lane strip should stay mounted but covered by a spinner overlay until the initial Beads load finishes.
   * Later refreshes should keep the already-loaded board visible and interactive instead of replaying the first-open loading mask.
   *
   * CDXC:ProjectBoardLocalFirst 2026-06-27-18:02:
   * Edit-ticket Save must close immediately and treat Beads persistence as background reconciliation.
   * Optimistically patch the local card, show an error toast, and reopen the same draft only if persistence fails so slow storage does not hold the modal open or lose the user's edits.
   */
  const isRefreshingRef = useRef(false);
  const issuesSignatureRef = useRef('');
  const labelsSignatureRef = useRef('');
  const newPromptRef = useRef<HTMLTextAreaElement>(null);
  const detailSaveSerialRef = useRef(0);
  const automationProjectsRef = useRef<ProjectAutomationsBridgeState['projects']>([]);
  const [conversationAction, setConversationAction] = useState<ConversationActionState>();
  /*
   * CDXC:ProjectBoard 2026-06-09-19:25:
   * The Project surface opens on Board by default so ticket work stays primary.
   *
   * CDXC:ProjectBoard 2026-06-29-03:49:
   * The Kanban board should not render the Board, Automations, Runs, or Triage tab strip; the visible toolbar is project title plus ticket actions.
   *
   * CDXC:Automations 2026-06-29-15:55:
   * Opening the dedicated Automation page should enter the existing automation
   * surface directly from the URL while ordinary Project/Kanban launches keep
   * the tab strip hidden and start on the board.
   */
  const [activeSurfaceTab, setActiveSurfaceTab] = useState<ProjectSurfaceTab>(initialSurfaceTab);
  const showAutomationComingSoonOverlay =
    activeSurfaceTab !== 'board' && automationIsExperimental && !experimentalFeaturesEnabled;
  const [automationState, setAutomationState] = useState<ProjectAutomationsBridgeState>({
    agents: [],
    automations: [],
    projectCanUseWorktrees: false,
    projectId,
    projectName,
    projectPath,
    projects: [],
    runs: [],
  });
  const [automationConversationState, setAutomationConversationState] = useState<ProjectBoardConversationState>({
    agents: [],
    debuggingMode: false,
    links: [],
    sessions: [],
  });
  const [automationDialogOpen, setAutomationDialogOpen] = useState(false);
  const [automationDraft, setAutomationDraft] = useState<AutomationDraft>(() => createAutomationDraft());
  const [automationActionId, setAutomationActionId] = useState('');
  const [automationTargetProjectId, setAutomationTargetProjectId] = useState(projectId);
  const [selectedAutomationId, setSelectedAutomationId] = useState('');
  const [selectedAutomationRunId, setSelectedAutomationRunId] = useState('');

  useEffect(() => {
    let lastPostedAt = 0;
    const postFocusOwnerChanged = (event: ProjectBoardFocusOwnerEvent, target: EventTarget | null) => {
      if (event !== 'pointerdown' && !isProjectBoardEditableFocusTarget(target)) {
        return;
      }
      const now = performance.now();
      if (now - lastPostedAt < PROJECT_BOARD_FOCUS_OWNER_MIN_INTERVAL_MS) {
        return;
      }
      lastPostedAt = now;
      postProjectBoardFocusOwnerChanged({
        event,
        projectEditorId,
        projectId,
        remoteMachineId,
      });
    };
    /*
     * CDXC:ProjectBoardFocus 2026-06-12-08:44:
     * Typing in Kanban must own keyboard focus over sidebar hydration and delayed companion-session focus repairs.
     * Report only sanitized focus-owner events from the Project WKWebView so native can protect active board input without logging field text, ticket titles, paths, URLs, or command content.
     */
    const handlePointerDown = (event: globalThis.PointerEvent) => {
      postFocusOwnerChanged('pointerdown', event.target);
    };
    const handleFocusIn = (event: globalThis.FocusEvent) => {
      postFocusOwnerChanged('focusin', event.target);
    };
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      postFocusOwnerChanged('keydown', event.target);
    };
    window.addEventListener('pointerdown', handlePointerDown, true);
    window.addEventListener('focusin', handleFocusIn, true);
    window.addEventListener('keydown', handleKeyDown, true);
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown, true);
      window.removeEventListener('focusin', handleFocusIn, true);
      window.removeEventListener('keydown', handleKeyDown, true);
    };
  }, [projectEditorId, projectId, remoteMachineId]);

  useEffect(() => {
    const syncExperimentalFeaturesEnabled = () => {
      setExperimentalFeaturesEnabled(readExperimentalFeaturesEnabled(new URLSearchParams(window.location.search)));
    };
    const handleStorage = (event: StorageEvent) => {
      if (event.key === null || event.key === NATIVE_SETTINGS_STORAGE_KEY) {
        syncExperimentalFeaturesEnabled();
      }
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        syncExperimentalFeaturesEnabled();
      }
    };
    window.addEventListener('storage', handleStorage);
    window.addEventListener('focus', syncExperimentalFeaturesEnabled);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      window.removeEventListener('storage', handleStorage);
      window.removeEventListener('focus', syncExperimentalFeaturesEnabled);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);

  useEffect(() => {
    try {
      window.localStorage.setItem(
        PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY,
        JSON.stringify({ estimateFilter, priorityFilter, sortOption, tagFilter })
      );
    } catch {
      // Keep the current in-memory preferences when localStorage is unavailable.
    }
  }, [estimateFilter, priorityFilter, sortOption, tagFilter]);

  const openNewTicket = useCallback((status: BoardStatusKey = 'todo') => {
    setNewTicket((current) => ({ ...current, status }));
    setNewTicketOpen(true);
  }, []);

  const runBeads = useCallback(
    async (request: Omit<BeadsBridgeRequest, 'cwd' | 'requestId'>) => {
      if (!projectPath) {
        throw new Error('No active project path is available.');
      }
      /*
       * CDXC:ProjectBoardRouting 2026-06-04-23:51:
       * Beads CRUD must address gxserver by the raw project id when the Project pane has one, not only by the URL path. Project paths in restored WKWebView URLs can be stale, while gxserver project ids are the canonical board scope.
       */
      const response = await sendBeadsRequest({
        ...request,
        cwd: projectPath,
        ...(projectId ? { projectId } : {}),
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (response.exitCode !== 0) {
        throw new Error(beadsErrorMessage(response.stderr || response.stdout));
      }
      return parseBeadsJson(response.stdout);
    },
    [projectId, projectPath, remoteMachineId]
  );

  const loadConversationState = useCallback(async () => {
    try {
      const response = await sendProjectBoardRequest({
        action: 'getState',
        projectId,
        projectEditorId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not load linked conversations.');
      }
      const payload = response.payload ?? { agents: [], links: [], sessions: [] };
      setConversationState(payload);
      setAutomationConversationState((current) => (automationTargetProjectId === projectId ? payload : current));
      setSelectedAgentId((current) => current || payload.defaultAgentId || payload.agents[0]?.agentId || '');
    } catch (error) {
      console.warn('Project board conversation state unavailable.', error);
    }
  }, [automationTargetProjectId, projectEditorId, projectId, projectPath, remoteMachineId]);

  const applyAutomationState = useCallback(
    (payload: ProjectAutomationsBridgeState) => {
      automationProjectsRef.current = payload.projects;
      setAutomationState(payload);
      setAutomationTargetProjectId(
        isAutomationGlobalScope ? (payload.projects[0]?.projectId ?? payload.projectId) : payload.projectId
      );
    },
    [isAutomationGlobalScope]
  );

  const loadAutomationState = useCallback(
    async (targetProjectId?: string) => {
      if (!experimentalFeaturesEnabled) {
        return;
      }
      const requestedProjectId = targetProjectId?.trim() || automationTargetProjectId || projectId;
      const targetProject = automationProjectsRef.current.find(
        (candidate) => candidate.projectId === requestedProjectId
      );
      try {
        const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
          action: isAutomationGlobalScope ? 'automationGetAllState' : 'automationGetState',
          projectEditorId,
          projectId: isAutomationGlobalScope ? projectId : requestedProjectId,
          projectPath: isAutomationGlobalScope
            ? undefined
            : (targetProject?.path ?? (requestedProjectId === projectId ? projectPath : undefined)),
          ...(remoteMachineId ? { remoteMachineId } : {}),
        });
        if (!response.ok) {
          throw new Error(response.error || 'Could not load automations.');
        }
        if (response.payload) {
          applyAutomationState(response.payload);
          setAutomationDraft((current) =>
            current.agentId
              ? current
              : {
                  ...current,
                  agentId: resolveAutomationDraftAgentId(
                    response.payload?.agents ?? [],
                    response.payload?.defaultAgentId
                  ),
                  projectId: isAutomationGlobalScope
                    ? resolveAutomationDraftProjectId(
                        response.payload?.projects ?? [],
                        current.projectId,
                        response.payload?.projectId || projectId
                      )
                    : current.projectId || response.payload?.projectId || projectId,
                  executionKind: response.payload?.projectCanUseWorktrees === false ? 'local' : current.executionKind,
                }
          );
        }
      } catch (error) {
        console.warn('Project automations state unavailable.', error);
      }
    },
    [
      applyAutomationState,
      automationTargetProjectId,
      experimentalFeaturesEnabled,
      isAutomationGlobalScope,
      projectEditorId,
      projectId,
      projectPath,
      remoteMachineId,
    ]
  );

  const loadAutomationConversationState = useCallback(
    async (targetProjectId?: string) => {
      if (!experimentalFeaturesEnabled) {
        setAutomationConversationState({ agents: [], debuggingMode: false, links: [], sessions: [] });
        return;
      }
      const requestedProjectId = targetProjectId?.trim() || automationTargetProjectId || projectId;
      if (
        isAutomationGlobalScope &&
        !automationProjectsRef.current.some((candidate) => candidate.projectId === requestedProjectId)
      ) {
        setAutomationConversationState({ agents: [], debuggingMode: false, links: [], sessions: [] });
        return;
      }
      const targetProject = automationProjectsRef.current.find(
        (candidate) => candidate.projectId === requestedProjectId
      );
      try {
        const response = await sendProjectBoardRequest({
          action: 'getState',
          projectEditorId,
          projectId: requestedProjectId,
          projectPath: targetProject?.path ?? (requestedProjectId === projectId ? projectPath : undefined),
          ...(remoteMachineId ? { remoteMachineId } : {}),
        });
        if (!response.ok) {
          throw new Error(response.error || 'Could not load automation sessions.');
        }
        setAutomationConversationState(response.payload ?? { agents: [], links: [], sessions: [] });
      } catch (error) {
        console.warn('Project automation sessions unavailable.', error);
        setAutomationConversationState({ agents: [], debuggingMode: false, links: [], sessions: [] });
      }
    },
    [
      automationTargetProjectId,
      experimentalFeaturesEnabled,
      isAutomationGlobalScope,
      projectEditorId,
      projectId,
      projectPath,
      remoteMachineId,
    ]
  );

  const logProjectBoardDebug = useCallback(
    (event: string, details?: Record<string, unknown>) => {
      if (!isDiagnosticLoggingScenarioEnabled(conversationState.diagnosticLogging, 'native.project.board')) {
        return;
      }
      void sendProjectBoardRequest({
        action: 'appendDebugLog',
        details: stringifyProjectBoardDebugDetails(details),
        event,
        projectId,
        projectEditorId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
      }).catch((error) => {
        console.warn('Project board debug log unavailable.', error);
      });
    },
    [conversationState.diagnosticLogging, projectEditorId, projectId, projectPath, remoteMachineId]
  );

  const showProjectBoardToast = useCallback(
    (level: AppToastLevel, title: string, description?: string) => {
      void sendProjectBoardRequest({
        action: 'showToast',
        projectEditorId,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        toastDescription: description,
        toastLevel: level,
        toastTitle: title,
      }).catch((error) => {
        console.warn('Project board toast unavailable.', error);
      });
    },
    [projectEditorId, projectId, projectPath, remoteMachineId]
  );

  const setLocalTicketStatus = useCallback((ticketId: string, statusKey: BoardStatusKey, beadsStatus: string) => {
    setAllIssues((current) =>
      current.map((candidate) => (candidate.id === ticketId ? { ...candidate, status: beadsStatus } : candidate))
    );
    setTickets((current) =>
      current.map((candidate) =>
        candidate.id === ticketId ? { ...candidate, boardStatus: statusKey, status: beadsStatus } : candidate
      )
    );
    setDetail((current) =>
      current.ticket?.id === ticketId
        ? {
            ...current,
            status: statusKey,
            ticket: { ...current.ticket, boardStatus: statusKey, status: beadsStatus },
          }
        : current
    );
  }, []);

  const upsertLocalIssue = useCallback(
    (issue: BeadsIssue) => {
      setAllIssues((current) => upsertProjectBoardIssue(current, issue));
      setTickets((current) => {
        const localTicket = toCreatedBoardTicket(issue, current, displayKey, boardColumns);
        return localTicket ? upsertProjectBoardTicket(current, localTicket) : current;
      });
      setDetail((current) =>
        current.ticket?.id === issue.id
          ? {
              ...current,
              description: issue.description ?? current.description,
              labels: issue.labels ?? current.labels,
              priority: issue.priority === undefined ? current.priority : prioritySelectValue(issue.priority),
              status: beadsStatusToBoardStatus(issue.status, boardColumns),
              title: issue.title,
              tshirt: estimateToTshirt(issue.estimate),
              ticket: {
                ...current.ticket,
                ...issue,
                boardStatus: beadsStatusToBoardStatus(issue.status, boardColumns),
                displayId: current.ticket.displayId,
              },
            }
          : current
      );
    },
    [boardColumns, displayKey]
  );

  const setLocalTicketTitle = useCallback((ticketId: string, title: string) => {
    setAllIssues((current) =>
      current.map((candidate) => (candidate.id === ticketId ? { ...candidate, title } : candidate))
    );
    setTickets((current) =>
      current.map((candidate) => (candidate.id === ticketId ? { ...candidate, title } : candidate))
    );
    setDetail((current) =>
      current.ticket?.id === ticketId ? { ...current, title, ticket: { ...current.ticket, title } } : current
    );
  }, []);

  const loadTickets = useCallback(
    async (options: BoardRefreshOptions = {}) => {
      const mode = options.mode ?? 'manual';
      if (isRefreshingRef.current) {
        if (mode === 'background') {
          return;
        }
        await waitForProjectBoardRefreshIdle(() => isRefreshingRef.current);
      }
      isRefreshingRef.current = true;
      if (mode !== 'background') {
        setLoadState('loading');
        setErrorMessage('');
      }
      try {
        let customStatusConfig: string;
        if (mode === 'initial' || mode === 'manual') {
          await ensureIssuePrefix(runBeads, issuePrefix);
          customStatusConfig = await ensureWorkflowStatuses(runBeads);
        } else {
          customStatusConfig = await readWorkflowStatuses(runBeads);
        }
        if (customStatusConfig !== boardColumnConfigRef.current) {
          boardColumnConfigRef.current = customStatusConfig;
          setBoardColumnConfig(customStatusConfig);
        }
        const nextColumns = buildBoardColumns(customStatusConfig);
        if (boardColumnsSignature(nextColumns) !== boardColumnsSignature(boardColumnsRef.current)) {
          boardColumnsRef.current = nextColumns;
          setBoardColumns(nextColumns);
        }
        const payload = await runBeads({ action: 'listIssues' });
        const rawIssues = normalizeBeadsPayload<BeadsIssue[]>(payload, Array.isArray(payload) ? payload : []);
        const issues = applyPendingBoardStatusMoves(rawIssues, pendingStatusMovesRef.current);
        /*
         * CDXC:ProjectBoardCustomColumns 2026-08-21:
         * The lane a bead belongs to is resolved against the column list, so the signature carries the
         * columns too. A status added to the board config while beads already sit in it changes no
         * issue, and remapping only on an issue change would leave the new lane empty and those beads
         * drawn in Todo until something unrelated moved.
         */
        const issuesSignature = `${displayKey}:${boardColumnsSignature(boardColumnsRef.current)}:${createIssuesSignature(issues)}`;
        if (issuesSignature !== issuesSignatureRef.current) {
          issuesSignatureRef.current = issuesSignature;
          setAllIssues(issues);
          setTickets(toBoardTickets(issues, displayKey, boardColumnsRef.current));
        }
        const labels = deriveKnownLabelsFromIssues(issues);
        const labelsSignature = labels.join('\u001f');
        if (labelsSignature !== labelsSignatureRef.current) {
          labelsSignatureRef.current = labelsSignature;
          setKnownLabels(labels);
        }
        if (mode !== 'background') {
          setLoadState('ready');
        } else {
          setErrorMessage('');
          setLoadState((current) => (current === 'loading' ? current : 'ready'));
        }
      } catch (error) {
        if (mode !== 'background') {
          setLoadState('error');
          setErrorMessage(error instanceof Error ? error.message : 'Could not load Beads issues.');
        } else {
          console.warn('Project board auto refresh failed.', error);
        }
      } finally {
        isRefreshingRef.current = false;
        if (mode === 'initial') {
          setHasCompletedInitialBoardLoad(true);
        }
      }
    },
    [displayKey, issuePrefix, runBeads]
  );

  const runProjectBoardCommand = useCallback(
    async (action: ProjectBoardRunnableCommandAction, migrationOption?: RunnableBeadsMigrationOption) => {
      if (runningProjectBoardCommand) {
        return;
      }
      const runKey = projectBoardCommandRunKey(action, migrationOption);
      setRunningProjectBoardCommand(runKey);
      try {
        const response = await sendProjectBoardRequest({
          action,
          ...(migrationOption ? { migrationOption } : {}),
          projectId,
        });
        if (!response.ok) {
          throw new Error(response.error || 'Could not start the Beads command.');
        }
      } catch (error) {
        setRunningProjectBoardCommand('');
        showProjectBoardToast(
          'error',
          'Could not run Beads command',
          error instanceof Error ? error.message : 'Could not start the Beads command.'
        );
      }
    },
    [projectId, runningProjectBoardCommand, showProjectBoardToast]
  );

  const initializeBeads = useCallback(() => {
    void runProjectBoardCommand('initializeBeads');
  }, [runProjectBoardCommand]);

  const installOrUpdateBeads = useCallback(() => {
    void runProjectBoardCommand('installOrUpdateBeads');
  }, [runProjectBoardCommand]);

  const runBeadsMigration = useCallback(
    (migrationOption: RunnableBeadsMigrationOption) => {
      void runProjectBoardCommand('runBeadsMigration', migrationOption);
    },
    [runProjectBoardCommand]
  );

  useEffect(() => {
    const handleCommandCompleted = (event: Event) => {
      const detail = (event as CustomEvent<ProjectBoardCommandCompletedEventDetail>).detail;
      if (
        detail?.action !== 'initializeBeads' &&
        detail?.action !== 'installOrUpdateBeads' &&
        detail?.action !== 'runBeadsMigration'
      ) {
        return;
      }
      setRunningProjectBoardCommand('');
      void loadTickets({ mode: 'manual' });
    };
    window.addEventListener(PROJECT_BOARD_COMMAND_COMPLETED_EVENT, handleCommandCompleted);
    return () => {
      window.removeEventListener(PROJECT_BOARD_COMMAND_COMPLETED_EVENT, handleCommandCompleted);
    };
  }, [loadTickets]);

  useEffect(() => {
    if (activeSurfaceTab === 'board' || (experimentalFeaturesEnabled && !isAutomationGlobalScope)) {
      void loadConversationState();
    }
    if (experimentalFeaturesEnabled) {
      void loadAutomationState();
    }
  }, [
    activeSurfaceTab,
    experimentalFeaturesEnabled,
    isAutomationGlobalScope,
    loadAutomationState,
    loadConversationState,
  ]);

  /*
   * CDXC:ProjectBoardStartWork 2026-08-07-07:01:
   * A ticket assigned to a configured agent must start work with that agent, so a
   * bead assigned to Dobby does not open a Claude session.
   * An agent the user picked for that ticket this session outranks the assignee,
   * and a ticket without a matching assignee keeps the board's default agent.
   */
  const assignedAgentIdForTicket = (ticket: BoardTicket): string | undefined =>
    pickedAgentIdByBeadIdRef.current.get(ticket.id) ??
    resolveAssignedAgentId(ticket.assignee, conversationState.agents);

  /*
   * CDXC:ProjectBoardStartWork 2026-08-07-07:01:
   * Re-resolve the open ticket's agent whenever the ticket or the configured agent
   * list changes, because the ticket refresh and the conversation state both land
   * after the ticket dialog opens.
   */
  useEffect(() => {
    const ticket = detail.ticket;
    if (!ticket) {
      return;
    }
    const nextAgentId = assignedAgentIdForTicket(ticket);
    if (nextAgentId) {
      setSelectedAgentId(nextAgentId);
    }
  }, [conversationState.agents, detail.ticket]);

  useEffect(() => {
    if (!ticketContextMenu) {
      return;
    }
    if (!tickets.some((ticket) => ticket.id === ticketContextMenu.ticketId)) {
      setTicketContextMenu(undefined);
    }
  }, [ticketContextMenu, tickets]);

  useEffect(() => {
    if (activeSurfaceTab !== 'board') {
      return;
    }
    void loadTickets({ mode: 'initial' });
  }, [activeSurfaceTab, loadTickets]);

  useEffect(() => {
    const imageSources = [
      ...extractPreviewableDescriptionImageReferences(detail.description),
      ...extractPreviewableDescriptionImageReferences(newTicket.description),
    ].map((image) => image.src);
    for (const imageSource of imageSources) {
      if (imageSource.startsWith('data:image/')) {
        setImagePreviewDataUrls((current) =>
          current[imageSource] ? current : { ...current, [imageSource]: imageSource }
        );
        continue;
      }
      if (
        imagePreviewDataUrls[imageSource] ||
        pendingImagePreviewPathsRef.current.has(imageSource) ||
        failedImagePreviewPathsRef.current.has(imageSource)
      ) {
        continue;
      }
      pendingImagePreviewPathsRef.current.add(imageSource);
      void sendProjectBoardImageRequest({ action: 'loadPreview', path: imageSource })
        .then((response) => {
          if (response.dataUrl?.startsWith('data:image/')) {
            setImagePreviewDataUrls((current) => ({
              ...current,
              [imageSource]: response.dataUrl ?? '',
            }));
            return;
          }
          failedImagePreviewPathsRef.current.add(imageSource);
          console.warn(response.error || `Could not load image preview for ${imageSource}.`);
        })
        .catch((error) => {
          failedImagePreviewPathsRef.current.add(imageSource);
          console.warn(error instanceof Error ? error.message : String(error));
        })
        .finally(() => {
          pendingImagePreviewPathsRef.current.delete(imageSource);
        });
    }
  }, [detail.description, imagePreviewDataUrls, newTicket.description]);

  useEffect(() => {
    const refreshIfVisible = () => {
      if (document.visibilityState !== 'visible') {
        return;
      }
      if (activeSurfaceTab === 'board') {
        void loadTickets({ mode: 'background' });
        void loadConversationState();
      }
      if (experimentalFeaturesEnabled) {
        void loadAutomationState();
      }
    };
    const intervalId = window.setInterval(() => refreshIfVisible(), PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS);
    const handleVisible = () => refreshIfVisible();
    document.addEventListener('visibilitychange', handleVisible);
    window.addEventListener('focus', handleVisible);
    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener('visibilitychange', handleVisible);
      window.removeEventListener('focus', handleVisible);
    };
  }, [activeSurfaceTab, experimentalFeaturesEnabled, loadAutomationState, loadConversationState, loadTickets]);

  const tagFilterSelectItems = useMemo(
    () =>
      boardTagFilterOptions(tickets).map((tag) => ({
        label: tag === 'all' ? 'All tags' : tag,
        value: tag,
      })),
    [tickets]
  );
  const activeTagFilter = useMemo(
    () =>
      resolveBoardTagFilter(
        tagFilter,
        tagFilterSelectItems.map((item) => item.value)
      ),
    [tagFilter, tagFilterSelectItems]
  );
  const filteredTickets = useMemo(
    () => filterBoardTickets(tickets, searchQuery, priorityFilter, estimateFilter, activeTagFilter),
    [activeTagFilter, estimateFilter, priorityFilter, searchQuery, tickets]
  );

  const ticketsByColumn = useMemo(() => {
    return boardColumns.reduce<Record<string, BoardTicket[]>>((result, column) => {
      result[column.key] = sortBoardTickets(
        filteredTickets.filter((ticket) => ticket.boardStatus === column.key),
        sortOption,
        column.key
      );
      return result;
    }, {});
  }, [boardColumns, filteredTickets, sortOption]);
  const showInitialBoardLoadingOverlay =
    activeSurfaceTab === 'board' && loadState === 'loading' && !hasCompletedInitialBoardLoad;

  const linksByBeadKey = useMemo(
    () => indexBeadConversationLinksByBead([...conversationState.links].sort(compareConversationLinksNewestFirst)),
    [conversationState.links]
  );

  const ticketOptions = useMemo(
    () =>
      prioritizeDependencyTickets(tickets)
        .slice(0, PROJECT_BOARD_MAX_DEPENDENCY_OPTIONS)
        .map((ticket) => ({
          id: ticket.id,
          label: `${ticket.displayId} · ${ticket.title}`,
        })),
    [tickets]
  );

  const openTicket = async (ticket: BoardTicket) => {
    setDeleteConfirmingTicketId('');
    setDetail({
      blockedByIds: getBlockedByIds(ticket),
      blockingIds: getBlockingIds(ticket.id, allIssues),
      comment: '',
      description: ticket.description ?? '',
      isDeleting: false,
      isSaving: false,
      labels: ticket.labels ?? [],
      priority: prioritySelectValue(ticket.priority),
      status: ticket.boardStatus,
      title: ticket.title,
      tshirt: estimateToTshirt(ticket.estimate),
      ticket,
    });
    try {
      const payload = await runBeads({ action: 'show', issueId: ticket.id });
      const issue = normalizeBeadsPayload<BeadsIssue>(payload, ticket);
      const mergedIssue = allIssues.find((candidate) => candidate.id === ticket.id) ?? issue;
      const nextTicket: BoardTicket = {
        ...ticket,
        ...issue,
        ...mergedIssue,
        boardStatus: beadsStatusToBoardStatus(issue.status ?? ticket.status, boardColumns),
        displayId: ticket.displayId,
      };
      setDetail({
        blockedByIds: getBlockedByIds(mergedIssue),
        blockingIds: getBlockingIds(ticket.id, allIssues),
        comment: '',
        description: nextTicket.description ?? '',
        isDeleting: false,
        isSaving: false,
        labels: nextTicket.labels ?? [],
        priority: prioritySelectValue(nextTicket.priority),
        status: nextTicket.boardStatus,
        title: nextTicket.title,
        tshirt: estimateToTshirt(nextTicket.estimate),
        ticket: nextTicket,
      });
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not load the ticket.');
    }
  };

  const moveTicket = async (ticketId: string, statusKey: BoardStatusKey) => {
    const column = boardColumns.find((candidate) => candidate.key === statusKey);
    const ticket = tickets.find((candidate) => candidate.id === ticketId);
    if (!column || !ticket || ticket.boardStatus === statusKey) {
      return;
    }
    const token = pendingStatusMoveSerialRef.current + 1;
    pendingStatusMoveSerialRef.current = token;
    pendingStatusMovesRef.current.set(ticketId, {
      beadsStatus: column.beadsStatus,
      statusKey,
      token,
    });
    setLocalTicketStatus(ticketId, statusKey, column.beadsStatus);
    try {
      await runBeads({
        action: 'updateStatus',
        issueId: ticketId,
        status: column.beadsStatus,
      });
      if (pendingStatusMovesRef.current.get(ticketId)?.token !== token) {
        return;
      }
      pendingStatusMovesRef.current.delete(ticketId);
      void loadTickets({ mode: 'background' });
    } catch (error) {
      if (pendingStatusMovesRef.current.get(ticketId)?.token !== token) {
        return;
      }
      pendingStatusMovesRef.current.delete(ticketId);
      setLocalTicketStatus(ticketId, ticket.boardStatus, ticket.status);
      if (reportBeadsRejection(error, ticketId, statusKey)) {
        return;
      }
      setErrorMessage(error instanceof Error ? error.message : 'Could not move the ticket.');
    }
  };

  /*
   * CDXC:ProjectBoardBeadsRejection 2026-08-20:
   * Beads refuses some board operations for domain reasons rather than failing:
   * a guarded close, an impossible dependency edge, an id that names no issue.
   * None of those is a board-wide failure, so none belongs in the inline notice
   * — the board's own background refresh clears that notice within a second, and
   * its copy tells the operator to reinstall Beads, which can never resolve any
   * of them. Raise them as persistent board toasts that state what was refused
   * and, where one exists, carry the actual remedy as an action.
   */
  function reportBeadsRejection(error: unknown, ticketId: string, statusKey: BoardStatusKey): boolean {
    const rejection = parseBeadsRejection(error instanceof Error ? error.message : '');
    if (!rejection) {
      return false;
    }
    const toastId = beadsRejectionToastId(ticketId);
    switch (rejection.kind) {
      case 'close-blocked': {
        const blockerList = formatIssueIdList(rejection.blockerIds);
        const isSingle = rejection.blockerIds.length === 1;
        toast.error(`${rejection.issueId} is blocked`, {
          action: {
            label: isSingle ? `Move ${rejection.blockerIds[0]} to Done` : 'Move blockers to Done',
            onClick: () => {
              void closeIssuesThenRetry(rejection.blockerIds, rejection.issueId, ticketId, statusKey);
            },
          },
          description: isSingle
            ? `Beads will not close it while ${blockerList} is still open. Move ${blockerList} to Done first, or remove the dependency if it no longer applies.`
            : `Beads will not close it while ${blockerList} are still open. Move them to Done first, or remove the dependencies if they no longer apply.`,
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
      case 'close-open-children': {
        /*
         * CDXC:ProjectBoardBeadsRejection 2026-08-20:
         * Beads reports only the open-child COUNT, so resolve the ids from the
         * board's own graph. Offer the action only when that lookup actually
         * finds them: a button that closes an unknown set would be guessing.
         */
        const childIds = openChildIdsOf(rejection.issueId);
        const isSingle = rejection.openChildren === 1;
        toast.error(`${rejection.issueId} has open children`, {
          ...(childIds.length > 0
            ? {
                action: {
                  label: childIds.length === 1 ? `Move ${childIds[0]} to Done` : 'Move children to Done',
                  onClick: () => {
                    void closeIssuesThenRetry(childIds, rejection.issueId, ticketId, statusKey);
                  },
                },
              }
            : {}),
          description: `Beads will not close it while ${rejection.openChildren} child issue${isSingle ? '' : 's'} ${isSingle ? 'is' : 'are'} still open${childIds.length > 0 ? `: ${formatIssueIdList(childIds)}` : ''}. Move ${isSingle ? 'it' : 'them'} to Done first.`,
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
      case 'dependency-cycle': {
        toast.error('That dependency would create a cycle', {
          description:
            'The two tickets would end up waiting on each other, so neither could ever close. Remove the opposite dependency first if this is the direction you want.',
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
      case 'dependency-hierarchy': {
        toast.error(`${rejection.issueId} cannot be blocked by ${rejection.blockerId}`, {
          description:
            rejection.relation === 'ancestor'
              ? `${rejection.blockerId} is its parent, and a parent cannot close until its children finish, so the block would never clear.`
              : `${rejection.blockerId} is its child, and a block cascades down to children, so ${rejection.blockerId} would inherit it and never close.`,
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
      case 'dependency-self': {
        toast.error('A ticket cannot depend on itself', {
          description: rejection.issueId
            ? `${rejection.issueId} was listed as its own blocker. Remove it from that field and save again.`
            : 'Remove the ticket from its own blocked-by or blocking field and save again.',
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
      case 'dependency-type-conflict': {
        toast.error(`${rejection.issueId} already depends on ${rejection.dependsOnId}`, {
          description: `That link is a "${rejection.existingType}" dependency, not "${rejection.requestedType}". Remove the existing one before adding it as a blocker: bd dep remove ${rejection.issueId} ${rejection.dependsOnId}`,
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
      case 'issue-missing': {
        toast.error(rejection.issueId ? `${rejection.issueId} no longer exists` : 'That ticket no longer exists', {
          action: {
            label: 'Refresh board',
            onClick: () => {
              toast.dismiss(toastId);
              void loadTickets({ mode: 'manual' });
            },
          },
          description: 'It was deleted or renamed outside this board. Refresh to pick up the current tickets.',
          duration: Infinity,
          id: toastId,
        });
        return true;
      }
    }
  }

  async function closeIssuesThenRetry(
    issueIds: string[],
    refusedIssueId: string,
    ticketId: string,
    statusKey: BoardStatusKey
  ) {
    const toastId = beadsRejectionToastId(ticketId);
    toast.loading(`Moving ${formatIssueIdList(issueIds)} to Done…`, {
      duration: Infinity,
      id: toastId,
    });
    for (const issueId of issueIds) {
      try {
        await runBeads({ action: 'updateStatus', issueId, status: 'closed' });
      } catch (error) {
        toast.dismiss(toastId);
        /*
         * CDXC:ProjectBoardBeadsRejection 2026-08-20:
         * A blocker or child can be guarded in turn. Re-report that refusal
         * against the issue that raised it so the operator walks the chain one
         * toast at a time instead of hitting a dead end.
         */
        if (!reportBeadsRejection(error, issueId, 'done')) {
          toast.error(`Could not move ${issueId} to Done`, {
            description: error instanceof Error ? error.message : 'The Beads command failed.',
            id: beadsRejectionToastId(issueId),
          });
        }
        void loadTickets({ mode: 'background' });
        return;
      }
    }
    toast.success(`Moved ${formatIssueIdList(issueIds)} to Done`, {
      description: `Retrying the move for ${refusedIssueId}.`,
      id: toastId,
    });
    await loadTickets({ mode: 'background' });
    await moveTicket(ticketId, statusKey);
  }

  function openChildIdsOf(parentId: string): string[] {
    return allIssues
      .filter((issue) =>
        (issue.dependencies ?? []).some(
          (dependency) => dependency.depends_on_id === parentId && dependency.type === 'parent-child'
        )
      )
      .filter((issue) => beadsStatusToBoardStatus(issue.status, boardColumns) !== 'done')
      .map((issue) => issue.id);
  }

  const handleDragEnd: ComponentProps<typeof DragDropProvider>['onDragEnd'] = (event) => {
    if (event.canceled) {
      return;
    }
    const ticketId = String(event.operation.source?.id ?? '');
    const statusKey = event.operation.target?.id as BoardStatusKey | undefined;
    if (ticketId && statusKey) {
      void moveTicket(ticketId, statusKey);
    }
  };

  /*
   * CDXC:ProjectBoardColumnManagement 2026-08-21:
   * Every column edit is one config write followed by a manual reload, so the lanes the user sees
   * always come back from bd rather than from optimistic local state — a board config write is rare
   * and cheap, and guessing at the result would drift from whatever bd actually stored.
   */
  const writeBoardColumnConfig = async (value: string) => {
    await runBeads({ action: 'configSet', value });
    boardColumnConfigRef.current = value;
    setBoardColumnConfig(value);
    await loadTickets({ mode: 'manual' });
  };

  const createBoardColumn = async (name: string) => {
    await writeBoardColumnConfig(addBoardColumn(boardColumnConfigRef.current, name));
  };

  const reorderBoardColumn = async (name: string, delta: -1 | 1) => {
    await writeBoardColumnConfig(moveBoardColumn(boardColumnConfigRef.current, name, delta));
  };

  /*
   * CDXC:ProjectBoardColumnManagement 2026-08-21:
   * Deleting is refused while the column still holds beads, so this never has to decide where an
   * orphan goes. That is deliberate: an unconfigured status resolves to Todo, so silently emptying a
   * parked lane would make parked work read as fresh work, which is the bug custom columns fixed.
   */
  const deleteBoardColumn = async (name: string) => {
    if (tickets.some((ticket) => ticket.boardStatus === name)) {
      return;
    }
    await writeBoardColumnConfig(removeBoardColumn(boardColumnConfigRef.current, name));
  };

  /*
   * CDXC:ProjectBoardColumnManagement 2026-08-21:
   * Renaming is not a config edit alone: every bead still holding the old status has to move, and a
   * bead may not carry a status the config does not list. So the new name is added alongside the old
   * one first, the beads are moved onto it, and only then is the old entry dropped — no point in the
   * sequence leaves a bead on a status the board does not know.
   */
  const applyBoardColumnRename = async (from: string, to: string) => {
    const nextName = to.trim();
    const bothConfig = beginBoardColumnRename(boardColumnConfigRef.current, from, nextName);
    await runBeads({ action: 'configSet', value: bothConfig });
    boardColumnConfigRef.current = bothConfig;
    setBoardColumnConfig(bothConfig);
    const ticketsToMove = tickets.filter((candidate) => candidate.boardStatus === from);
    for (const [index, ticket] of ticketsToMove.entries()) {
      try {
        await runBeads({ action: 'updateStatus', issueId: ticket.id, status: nextName });
      } catch (error) {
        const unmovedIds = ticketsToMove.slice(index).map((ticket) => ticket.id);
        await loadTickets({ mode: 'manual' });
        const failure = beadsErrorMessage(error instanceof Error ? error.message : '');
        throw new Error(
          `Could not finish renaming ${from} to ${nextName}. ${unmovedIds.length === 1 ? 'Ticket' : 'Tickets'} ${unmovedIds.join(', ')} did not move. ${failure}`
        );
      }
    }
    await writeBoardColumnConfig(removeBoardColumn(bothConfig, from));
  };

  const syncDependencies = async (issueId: string, blockedByIds: string[], blockingIds: string[]) => {
    const issue = allIssues.find((candidate) => candidate.id === issueId);
    const currentBlockedBy = issue ? getBlockedByIds(issue) : [];
    const currentBlocking = issue ? getBlockingIds(issueId, allIssues) : [];
    for (const dependencyId of currentBlockedBy.filter((id) => !blockedByIds.includes(id))) {
      await runBeads({ action: 'depRemove', dependsOnId: dependencyId, issueId });
    }
    for (const dependencyId of blockedByIds.filter((id) => !currentBlockedBy.includes(id))) {
      await runBeads({ action: 'depAdd', dependsOnId: dependencyId, issueId, depType: 'blocks' });
    }
    for (const dependentId of currentBlocking.filter((id) => !blockingIds.includes(id))) {
      await runBeads({ action: 'depRemove', dependsOnId: issueId, issueId: dependentId });
    }
    for (const dependentId of blockingIds.filter((id) => !currentBlocking.includes(id))) {
      await runBeads({ action: 'depAdd', dependsOnId: issueId, issueId: dependentId, depType: 'blocks' });
    }
  };

  const persistTicketDetail = async (draft: TicketDetailSaveDraft) => {
    const trimmedComment = draft.comment.trim();
    await runBeads({
      action: 'updateTitle',
      issueId: draft.ticket.id,
      title: draft.title.trim(),
    });
    await runBeads({
      action: 'updateDescription',
      description: draft.description,
      issueId: draft.ticket.id,
    });
    await runBeads({
      action: 'updatePriority',
      issueId: draft.ticket.id,
      priority: draft.priority,
    });
    const estimate = tshirtToEstimate(draft.tshirt);
    if (estimate !== undefined) {
      await runBeads({
        action: 'updateEstimate',
        estimate,
        issueId: draft.ticket.id,
      });
    }
    if (draft.labels.length > 0) {
      await runBeads({
        action: 'setLabels',
        issueId: draft.ticket.id,
        labels: draft.labels,
      });
    }
    await syncDependencies(draft.ticket.id, draft.blockedByIds, draft.blockingIds);
    if (draft.status !== draft.ticket.boardStatus) {
      await runBeads({
        action: 'updateStatus',
        issueId: draft.ticket.id,
        status: boardStatusBeadsValue(draft.status, boardColumns),
      });
    }
    if (trimmedComment) {
      await runBeads({
        action: 'addComment',
        comment: formatProjectBoardCommentText(trimmedComment, draft.commentMetadata),
        issueId: draft.ticket.id,
      });
    }
    await loadTickets({ mode: 'background' });
  };

  const saveTicketDetail = () => {
    if (!detail.ticket) {
      return;
    }
    const draft: TicketDetailSaveDraft = {
      blockedByIds: [...detail.blockedByIds],
      blockingIds: [...detail.blockingIds],
      comment: detail.comment,
      commentMetadata: projectBoardCommentMetadataFromLink(detailCommentMetadataLink),
      description: detail.description,
      labels: [...detail.labels],
      priority: detail.priority,
      status: detail.status,
      title: detail.title,
      tshirt: detail.tshirt,
      ticket: detail.ticket,
    };
    const saveToken = detailSaveSerialRef.current + 1;
    detailSaveSerialRef.current = saveToken;
    const parsedPriority = Number.parseInt(draft.priority, 10);
    const estimate = tshirtToEstimate(draft.tshirt);
    const optimisticIssue: BeadsIssue = {
      ...draft.ticket,
      description: draft.description,
      ...(estimate !== undefined ? { estimate } : {}),
      labels: draft.labels.length > 0 ? draft.labels : draft.ticket.labels,
      priority: Number.isFinite(parsedPriority) ? parsedPriority : draft.ticket.priority,
      status: boardStatusBeadsValue(draft.status, boardColumns),
      title: draft.title.trim(),
    };
    setDeleteConfirmingTicketId('');
    setDetail(createEmptyDetailDraft());
    setErrorMessage('');
    upsertLocalIssue(optimisticIssue);
    setKnownLabels((current) => mergeKnownLabels(current, draft.labels));
    void persistTicketDetail(draft).catch((error) => {
      const message = error instanceof Error ? error.message : 'Could not save the ticket.';
      if (!reportBeadsRejection(error, draft.ticket.id, draft.status)) {
        setErrorMessage(message);
        showProjectBoardToast('error', 'Ticket save failed', message);
      }
      if (detailSaveSerialRef.current === saveToken) {
        setDetail({
          ...draft,
          isDeleting: false,
          isSaving: false,
        });
      }
      void loadTickets({ mode: 'background' });
    });
  };

  const createTicket = async (options: { startAfterCreate?: boolean } = {}) => {
    if (createInFlightRef.current) {
      return;
    }
    const startAfterCreate = options.startAfterCreate === true;
    const startLocation = newTicketStartLocation;
    const draft = {
      ...newTicket,
      blockedByIds: [...newTicket.blockedByIds],
      blockingIds: [...newTicket.blockingIds],
      labels: [...newTicket.labels],
    };
    const prompt = draft.description.trim();
    if (!prompt) {
      return;
    }
    if (startAfterCreate && conversationState.agents.length === 0) {
      return;
    }
    createInFlightRef.current = true;
    setNewTicket(createEmptyTicketFormDraft());
    setNewTicketStartLocation('currentProject');
    setNewTicketOpen(false);
    logProjectBoardDebug('projectBoard.createTicket.started', {
      blockedByCount: draft.blockedByIds.length,
      blockingCount: draft.blockingIds.length,
      hasRequestedTitle: Boolean(draft.title.trim()),
      labelCount: draft.labels.length,
      promptLength: prompt.length,
      startAfterCreate,
      startLocation,
      targetStatus: draft.status,
    });
    try {
      await ensureIssuePrefix(runBeads, issuePrefix);
      const requestedTitle = draft.title.trim();
      const shouldGenerateTitle = !requestedTitle;
      const title = shouldGenerateTitle ? createProjectBoardDraftTitle(prompt) : requestedTitle;
      const estimate = tshirtToEstimate(draft.tshirt);
      const issueIdsBeforeCreate = new Set(allIssues.map((issue) => issue.id));
      const createdPayload = await runBeads({
        action: 'create',
        description: prompt,
        dependsOnId: draft.blockedByIds[0],
        estimate,
        labels: draft.labels,
        priority: draft.priority,
        title,
      });
      const created = normalizeBeadsPayload<BeadsIssue | BeadsIssue[]>(createdPayload, []);
      let createdIssue: BeadsIssue | undefined = Array.isArray(created) ? created[0] : created;
      let didStartCreatedTicket = false;
      logProjectBoardDebug('projectBoard.createTicket.beadCreated', {
        beadId: createdIssue?.id ?? '',
        shouldGenerateTitle,
        startAfterCreate,
        targetStatus: draft.status,
      });
      if (!createdIssue?.id) {
        const createdIssueLookupPayload = await runBeads({ action: 'listIssues' });
        const createdIssueLookupIssues = normalizeBeadsPayload<BeadsIssue[]>(
          createdIssueLookupPayload,
          Array.isArray(createdIssueLookupPayload) ? createdIssueLookupPayload : []
        );
        createdIssue = resolveCreatedIssueFromRefresh(createdIssueLookupIssues, issueIdsBeforeCreate, {
          description: prompt,
          title,
        });
      }
      const generateCreatedTicketTitle = async (issueId: string) => {
        /*
         * CDXC:ProjectBoardDiagnostics 2026-06-21-03:56:
         * Empty-title ticket creation is a two-step flow: Beads persists the
         * ticket first, then the Project board asks the selected/default prompt
         * agent to generate a display title. Log only built-in agent categories,
         * counts, booleans, lengths, and classification enums so a repro can
         * line up webview and native title-generation failures without
         * persisting prompt text, command text, paths, custom agent ids, stdout,
         * stderr, or raw error output.
         */
        const promptAgentId = selectedAgentId || conversationState.defaultAgentId || '';
        const promptAgent = conversationState.agents.find((agent) => agent.agentId === promptAgentId);
        const titleGenerationDebugDetails = {
          agentCount: conversationState.agents.length,
          beadId: issueId,
          defaultAgentKind: projectBoardPromptAgentKind(conversationState.defaultAgentId || ''),
          hasDefaultAgentId: Boolean(conversationState.defaultAgentId),
          hasPromptAgent: Boolean(promptAgent),
          hasPromptAgentCommand: Boolean(promptAgent?.command?.trim()),
          hasSelectedAgentId: Boolean(selectedAgentId),
          promptAgentCommandLength: promptAgent?.command?.length ?? 0,
          promptLength: prompt.length,
          resolvedAgentKind: projectBoardPromptAgentKind(promptAgentId),
          selectedAgentKind: projectBoardPromptAgentKind(selectedAgentId || ''),
          startAfterCreate,
        };
        try {
          logProjectBoardDebug('projectBoard.createTicket.titleGeneration.started', {
            ...titleGenerationDebugDetails,
          });
          const generated = normalizeBeadsPayload<{ title?: string }>(
            await runBeads({
              action: 'generateTitle',
              agentCommand: promptAgent?.command,
              agentId: promptAgentId,
              issueId,
              prompt,
            }),
            {}
          );
          const generatedTitle = generated.title?.trim();
          logProjectBoardDebug('projectBoard.createTicket.titleGeneration.bridgeResponse', {
            ...titleGenerationDebugDetails,
            generatedTitleLength: generatedTitle?.length ?? 0,
            hasGeneratedTitle: Boolean(generatedTitle),
          });
          if (!generatedTitle) {
            throw new Error('Prompt-agent title generation returned an empty title.');
          }
          await runBeads({
            action: 'updateTitle',
            issueId,
            title: generatedTitle,
          });
          /*
           * CDXC:ProjectBoardTitleGeneration 2026-06-21-16:56:
           * Generated title completion is background polish for one card.
           * Patch the local ticket title after the durable Beads update and do not reload the full board, so a slow prompt-agent title cannot hitch Kanban scrolling, drag/drop, or follow-up ticket creation.
           */
          setLocalTicketTitle(issueId, generatedTitle);
          logProjectBoardDebug('projectBoard.createTicket.titleGeneration.completed', {
            beadId: issueId,
            generatedTitleLength: generatedTitle.length,
            startAfterCreate,
          });
        } catch (error) {
          logProjectBoardDebug('projectBoard.createTicket.titleGeneration.failed', {
            ...titleGenerationDebugDetails,
            ...projectBoardTitleGenerationFailureDetails(error),
          });
        }
      };

      if (!createdIssue?.id) {
        throw new Error('Created ticket was not found after create.');
      }

      const targetBeadsStatus = boardStatusBeadsValue(draft.status, boardColumns);
      const parsedPriority = Number.parseInt(draft.priority, 10);
      let pendingCreateStatusToken: number | undefined;
      if (!startAfterCreate && draft.status !== 'todo') {
        pendingCreateStatusToken = pendingStatusMoveSerialRef.current + 1;
        pendingStatusMoveSerialRef.current = pendingCreateStatusToken;
        pendingStatusMovesRef.current.set(createdIssue.id, {
          beadsStatus: targetBeadsStatus,
          statusKey: draft.status,
          token: pendingCreateStatusToken,
        });
      }
      createdIssue = {
        ...createdIssue,
        description: createdIssue.description ?? prompt,
        ...(estimate !== undefined ? { estimate } : {}),
        labels: draft.labels.length > 0 ? draft.labels : createdIssue.labels,
        priority: Number.isFinite(parsedPriority) ? parsedPriority : createdIssue.priority,
        status: targetBeadsStatus,
        title,
      };
      upsertLocalIssue(createdIssue);
      setKnownLabels((current) => mergeKnownLabels(current, createdIssue?.labels ?? draft.labels));

      if (startAfterCreate) {
        const createdTicket = toCreatedBoardTicket(createdIssue, allIssues, displayKey, boardColumns);
        if (!createdTicket) {
          throw new Error('Created ticket was not available for start.');
        }
        logProjectBoardDebug('projectBoard.createTicket.startAfterCreate.requested', {
          beadId: createdTicket.id,
          displayId: createdTicket.displayId,
          startLocation,
        });
        const didStart = await startTicketWork(createdTicket, { startLocation });
        if (didStart) {
          didStartCreatedTicket = true;
        }
      }

      const createdIssueId = createdIssue.id;
      const reconcileCreatedTicket = async () => {
        try {
          await syncDependencies(createdIssueId, draft.blockedByIds, draft.blockingIds);
          if (draft.status !== 'todo' && !didStartCreatedTicket) {
            await runBeads({
              action: 'updateStatus',
              issueId: createdIssueId,
              status: targetBeadsStatus,
            });
            if (
              pendingCreateStatusToken !== undefined &&
              pendingStatusMovesRef.current.get(createdIssueId)?.token === pendingCreateStatusToken
            ) {
              pendingStatusMovesRef.current.delete(createdIssueId);
            }
          }
          if (draft.labels.length > 0) {
            await runBeads({
              action: 'setLabels',
              issueId: createdIssueId,
              labels: draft.labels,
            });
            setKnownLabels((current) => mergeKnownLabels(current, draft.labels));
          }
          await loadTickets({ mode: 'background' });
        } catch (error) {
          if (
            pendingCreateStatusToken !== undefined &&
            pendingStatusMovesRef.current.get(createdIssueId)?.token === pendingCreateStatusToken
          ) {
            pendingStatusMovesRef.current.delete(createdIssueId);
          }
          const message = error instanceof Error ? error.message : 'Could not finish creating the ticket.';
          setErrorMessage(message);
          showProjectBoardToast('error', 'Ticket update failed', message);
          void loadTickets({ mode: 'background' });
        }
      };

      /*
       * CDXC:ProjectBoardTitleGeneration 2026-06-21-16:56:
       * Ticket reconciliation owns dependency/status/label durability, while prompt-agent title generation owns only replacing the deterministic draft title.
       * Start both as detached background work so title generation is not serialized behind board reconciliation and the create flow can return immediately.
       */
      void reconcileCreatedTicket();
      if (shouldGenerateTitle) {
        scheduleProjectBoardGeneratedTitle(() => {
          void generateCreatedTicketTitle(createdIssueId);
        });
      }
      logProjectBoardDebug('projectBoard.createTicket.completed', {
        beadId: createdIssueId,
        startAfterCreate,
        startLocation,
      });
    } catch (error) {
      logProjectBoardDebug('projectBoard.createTicket.failed', {
        error: error instanceof Error ? error.message : String(error),
        startAfterCreate,
        startLocation,
      });
      const message = error instanceof Error ? error.message : 'Could not create the ticket.';
      setErrorMessage(message);
      showProjectBoardToast('error', 'Ticket creation failed', message);
    } finally {
      createInFlightRef.current = false;
    }
  };

  const deleteTicket = async (targetTicket?: BoardTicket) => {
    const ticket = targetTicket ?? detail.ticket;
    if (!ticket) {
      return;
    }
    const deletingFromDialog = detail.ticket?.id === ticket.id;
    if ((deletingFromDialog && detail.isDeleting) || contextMenuDeletingTicketId === ticket.id) {
      return;
    }
    if (deletingFromDialog) {
      setDetail((current) => ({ ...current, isDeleting: true }));
    } else {
      setContextMenuDeletingTicketId(ticket.id);
    }
    setTickets((current) => current.filter((candidate) => candidate.id !== ticket.id));
    try {
      await runBeads({ action: 'delete', issueId: ticket.id });
      setDeleteConfirmingTicketId('');
      setTicketContextMenu(undefined);
      if (deletingFromDialog) {
        setDetail(createEmptyDetailDraft());
      }
      await loadTickets({ mode: 'mutation' });
    } catch (error) {
      setTickets((current) =>
        current.some((candidate) => candidate.id === ticket.id) ? current : [...current, ticket]
      );
      setErrorMessage(error instanceof Error ? error.message : 'Could not delete the ticket.');
      if (deletingFromDialog) {
        setDetail((current) => ({ ...current, isDeleting: false }));
      }
    } finally {
      if (!deletingFromDialog) {
        setContextMenuDeletingTicketId((current) => (current === ticket.id ? '' : current));
      }
    }
  };

  const startTicketWork = async (
    ticket: BoardTicket | undefined = detail.ticket,
    options: { startLocation?: ProjectBoardStartLocation } = {}
  ) => {
    if (!ticket) {
      return false;
    }
    const startLocation = options.startLocation ?? 'currentProject';
    const startAgentId = assignedAgentIdForTicket(ticket) || selectedAgentId || conversationState.defaultAgentId;
    setConversationAction({ beadId: ticket.id, kind: 'start' });
    logProjectBoardDebug('projectBoard.createStart.startWork.requested', {
      agentId: startAgentId || '',
      beadId: ticket.id,
      displayId: ticket.displayId,
      startLocation,
    });
    try {
      const prompt = buildAgentWorkPrompt(ticket);
      const response = await sendProjectBoardRequest({
        action: 'startWork',
        agentId: startAgentId,
        beadDisplayId: ticket.displayId,
        beadId: ticket.id,
        projectId,
        projectPath,
        prompt,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        startLocation,
        ticketTitle: ticket.title,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not start ticket work.');
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      const token = pendingStatusMoveSerialRef.current + 1;
      pendingStatusMoveSerialRef.current = token;
      pendingStatusMovesRef.current.set(ticket.id, {
        beadsStatus: 'in_progress',
        statusKey: 'in_progress',
        token,
      });
      setLocalTicketStatus(ticket.id, 'in_progress', 'in_progress');
      void runBeads({
        action: 'updateStatus',
        issueId: ticket.id,
        status: 'in_progress',
      })
        .then(() => {
          if (pendingStatusMovesRef.current.get(ticket.id)?.token !== token) {
            return;
          }
          pendingStatusMovesRef.current.delete(ticket.id);
          void loadTickets({ mode: 'background' });
        })
        .catch((error) => {
          if (pendingStatusMovesRef.current.get(ticket.id)?.token !== token) {
            return;
          }
          pendingStatusMovesRef.current.delete(ticket.id);
          setErrorMessage(error instanceof Error ? error.message : 'Could not move the ticket.');
          void loadTickets({ mode: 'background' });
        });
      setErrorMessage('');
      logProjectBoardDebug('projectBoard.createStart.startWork.completed', {
        beadId: ticket.id,
        startLocation,
      });
      return true;
    } catch (error) {
      logProjectBoardDebug('projectBoard.createStart.startWork.failed', {
        beadId: ticket.id,
        error: error instanceof Error ? error.message : String(error),
        startLocation,
      });
      setErrorMessage(error instanceof Error ? error.message : 'Could not start ticket work.');
      return false;
    } finally {
      setConversationAction((current) =>
        current?.kind === 'start' && current.beadId === ticket.id ? undefined : current
      );
    }
  };

  const selectTicketAgent = (agentId: string) => {
    if (detail.ticket) {
      pickedAgentIdByBeadIdRef.current.set(detail.ticket.id, agentId);
    }
    setSelectedAgentId(agentId);
  };

  const associateFocusedSession = async () => {
    if (!detail.ticket) {
      return;
    }
    const ticket = detail.ticket;
    setConversationAction({ beadId: ticket.id, kind: 'associate' });
    try {
      const response = await sendProjectBoardRequest({
        action: 'associateFocusedSession',
        beadDisplayId: ticket.displayId,
        beadId: ticket.id,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        ticketTitle: ticket.title,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not associate the focused session.');
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      setErrorMessage('');
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not associate the focused session.');
    } finally {
      setConversationAction((current) =>
        current?.kind === 'associate' && current.beadId === ticket.id ? undefined : current
      );
    }
  };

  const jumpToConversation = async (link: ProjectBoardConversationLinkView) => {
    setConversationAction({ kind: 'jump', linkId: link.id });
    try {
      const response = await sendProjectBoardRequest({
        action: 'jumpToConversation',
        beadId: link.beadId,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: link.ghostexSessionId,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not jump to the linked conversation.');
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      setErrorMessage('');
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not jump to the linked conversation.');
    } finally {
      setConversationAction((current) =>
        current?.kind === 'jump' && current.linkId === link.id ? undefined : current
      );
    }
  };

  const unlinkConversation = async (link: ProjectBoardConversationLinkView) => {
    setConversationAction({ kind: 'unlink', linkId: link.id });
    try {
      const response = await sendProjectBoardRequest({
        action: 'unlinkConversation',
        beadId: link.beadId,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: link.ghostexSessionId,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not unlink the conversation.');
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      setErrorMessage('');
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not unlink the conversation.');
    } finally {
      setConversationAction((current) =>
        current?.kind === 'unlink' && current.linkId === link.id ? undefined : current
      );
    }
  };

  const openNewAutomationDialog = () => {
    const targetProjectId = isAutomationGlobalScope
      ? resolveAutomationDraftProjectId(
          automationState.projects,
          automationTargetProjectId,
          automationState.projectId || projectId
        )
      : automationState.projectId;
    const targetProject = automationProjectsById.get(targetProjectId);
    void loadAutomationConversationState(targetProjectId);
    setAutomationDraft(
      createAutomationDraft({
        agentId: resolveAutomationDraftAgentId(automationState.agents, automationState.defaultAgentId),
        executionKind: (
          isAutomationGlobalScope ? targetProject?.canUseWorktrees : automationState.projectCanUseWorktrees
        )
          ? 'worktree'
          : 'local',
        projectId: targetProjectId,
      })
    );
    setAutomationDialogOpen(true);
  };

  const openEditAutomationDialog = (automation: AutomationDefinition) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    void loadAutomationConversationState(targetProjectId);
    setAutomationDraft(createAutomationDraftFromDefinition(automation, targetProjectId));
    setAutomationDialogOpen(true);
  };

  const automationProjectPathForId = (targetProjectId: string): string | undefined =>
    automationProjectsById.get(targetProjectId)?.path ??
    (targetProjectId === projectId ? projectPath : undefined) ??
    automationState.projectPath;

  const applyAutomationMutationState = async (payload: ProjectAutomationsBridgeState | undefined) => {
    if (isAutomationGlobalScope) {
      await loadAutomationState();
      return;
    }
    if (payload) {
      applyAutomationState(payload);
    }
  };

  const saveAutomation = async () => {
    const targetProjectId = isAutomationGlobalScope
      ? resolveAutomationDraftProjectId(automationState.projects, automationDraft.projectId, '')
      : automationDraft.projectId || automationState.projectId || projectId;
    if (!targetProjectId) {
      setErrorMessage('Choose a project before saving automation.');
      return;
    }
    const definition = createAutomationDefinitionFromDraft(automationDraft, {
      fallbackAgentId: resolveAutomationDraftAgentId(automationState.agents, automationState.defaultAgentId),
      projectId: targetProjectId,
    });
    if (!definition) {
      setErrorMessage('Name, agent, prompt, and schedule are required.');
      return;
    }
    if (definition.executionMode.kind === 'worktree' && !automationDraftCanUseWorktrees) {
      setErrorMessage(automationDraftWorktreeUnavailableReason || 'Worktree mode is unavailable for this project.');
      return;
    }
    setAutomationActionId(definition.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationSave',
        payloadJson: JSON.stringify(definition),
        projectEditorId,
        projectId: definition.projectIds[0] ?? projectId,
        projectPath: automationProjectPathForId(definition.projectIds[0] ?? projectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not save automation.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
      setAutomationDialogOpen(false);
      setErrorMessage('');
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not save automation.');
    } finally {
      setAutomationActionId('');
    }
  };

  const deleteAutomation = async (automation: AutomationDefinition) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    setAutomationActionId(automation.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationDelete',
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: automation.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not delete automation.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not delete automation.');
    } finally {
      setAutomationActionId('');
    }
  };

  const setAutomationEnabled = async (automation: AutomationDefinition, enabled: boolean) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    setAutomationActionId(automation.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationSetEnabled',
        payloadJson: JSON.stringify({ enabled }),
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: automation.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not update automation.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
      setErrorMessage('');
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not update automation.');
    } finally {
      setAutomationActionId('');
    }
  };

  const runAutomationNow = async (automation: AutomationDefinition) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    setAutomationActionId(automation.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationRunNow',
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: automation.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not run automation.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
      setActiveSurfaceTab('runs');
      setErrorMessage('');
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not run automation.');
    } finally {
      setAutomationActionId('');
    }
  };

  const archiveAutomationRun = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const removeWorktree =
        Boolean(run.worktree) &&
        window.confirm(
          `Archive this run and remove its worktree?\n\nPath: ${run.worktree?.path ?? ''}\nBranch: ${run.worktree?.branch ?? ''}`
        );
      if (removeWorktree) {
        const confirmation = window.prompt(`Type the exact worktree path to remove it:\n\n${run.worktree?.path ?? ''}`);
        if (confirmation !== run.worktree?.path) {
          setErrorMessage('Worktree removal was not confirmed. The run was not archived.');
          return;
        }
      }
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationArchiveRun',
        payloadJson: JSON.stringify({ removeWorktree }),
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not archive automation run.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not archive automation run.');
    } finally {
      setAutomationActionId('');
    }
  };

  const markAutomationRunRead = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationMarkRunRead',
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not mark automation run read.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not mark automation run read.');
    } finally {
      setAutomationActionId('');
    }
  };

  const openAutomationRunSession = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationOpenRunSession',
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not open automation session.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not open automation session.');
    } finally {
      setAutomationActionId('');
    }
  };

  const openAutomationRunWorktree = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: 'automationOpenWorktree',
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || 'Could not open automation worktree.');
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Could not open automation worktree.');
    } finally {
      setAutomationActionId('');
    }
  };

  const detailConversationLinks = detail.ticket ? selectBeadConversationLinks(linksByBeadKey, detail.ticket.id) : [];
  const detailPrimaryConversationLink = getPrimaryUsableConversationLink(detailConversationLinks);
  const detailCommentMetadataLink = detailPrimaryConversationLink ?? detailConversationLinks[0];
  const detailPrimaryActionKind = conversationLinkActionKind(detailPrimaryConversationLink);
  const detailPrimaryActionLabel =
    conversationAction?.kind === 'jump' && conversationAction.linkId === detailPrimaryConversationLink?.id
      ? detailPrimaryActionKind === 'resume'
        ? 'Resuming'
        : 'Opening'
      : detailPrimaryConversationLink
        ? detailPrimaryActionKind === 'resume'
          ? 'Resume Session'
          : 'Go to Session'
        : conversationAction?.kind === 'start' && conversationAction.beadId === detail.ticket?.id
          ? 'Starting'
          : 'Start work';
  const detailPrimaryActionDisabled =
    detail.isDeleting ||
    detail.isSaving ||
    Boolean(conversationAction) ||
    (!detailPrimaryConversationLink && conversationState.agents.length === 0);
  const visibleAutomationRuns = automationState.runs.filter((run) => !run.isArchived);
  const triageAutomationRuns = selectAutomationRunsForTriage(visibleAutomationRuns);
  const selectedAutomation =
    automationState.automations.find((automation) => automation.id === selectedAutomationId) ??
    automationState.automations[0];
  const selectedTriageRun =
    triageAutomationRuns.find((run) => run.id === selectedAutomationRunId) ?? triageAutomationRuns[0];
  const selectedVisibleRun =
    visibleAutomationRuns.find((run) => run.id === selectedAutomationRunId) ?? visibleAutomationRuns[0];
  const automationProjectsById = useMemo(
    () => new Map(automationState.projects.map((project) => [project.projectId, project])),
    [automationState.projects]
  );
  const automationProjectSelectItems = useMemo(
    () => automationState.projects.map((project) => ({ label: project.label, value: project.projectId })),
    [automationState.projects]
  );
  const selectedAutomationDraftProject = automationProjectsById.get(automationDraft.projectId);
  const automationDraftCanUseWorktrees = isAutomationGlobalScope
    ? selectedAutomationDraftProject?.canUseWorktrees === true
    : automationState.projectCanUseWorktrees;
  const automationDraftWorktreeUnavailableReason = isAutomationGlobalScope
    ? selectedAutomationDraftProject?.worktreeUnavailableReason
    : automationState.worktreeUnavailableReason;
  const automationProjectNameById = useMemo(
    () => new Map(automationState.projects.map((project) => [project.projectId, project.label])),
    [automationState.projects]
  );
  /*
   * CDXC:ProjectAutomations 2026-06-09-15:38:
   * Automation agents come from the Project Board bridge as label/icon options, while shared select metadata expects sidebar-agent names.
   * Adapt only the root select items so the automation bridge contract stays focused on user-facing labels.
   */
  const automationAgentSelectItems = useMemo(
    () =>
      createSidebarAgentSelectItems(
        automationState.agents.map((agent) => ({
          agentId: agent.agentId,
          name: agent.label,
        }))
      ),
    [automationState.agents]
  );
  const automationScheduleSelectItems = useMemo(
    () => AUTOMATION_SCHEDULE_PRESETS.map((option) => ({ label: option.label, value: option.value })),
    []
  );
  const automationWeekdaySelectItems = useMemo(
    () =>
      AUTOMATION_WEEKDAY_OPTIONS.map((day, index) => ({
        label: day,
        value: String(index),
      })),
    []
  );
  const automationTimerUnitSelectItems = useMemo(() => AUTOMATION_TIMER_UNIT_OPTIONS, []);
  const automationSessionSelectItems = useMemo(
    () =>
      automationConversationState.sessions.map((session) => ({
        label: session.label,
        value: session.sessionId,
      })),
    [automationConversationState.sessions]
  );
  const contextMenuTicket = ticketContextMenu
    ? tickets.find((ticket) => ticket.id === ticketContextMenu.ticketId)
    : undefined;
  const contextMenuPrimaryLink = contextMenuTicket
    ? getPrimaryUsableConversationLink(selectBeadConversationLinks(linksByBeadKey, contextMenuTicket.id))
    : undefined;
  const contextMenuPrimaryActionLabel = contextMenuPrimaryLink
    ? conversationLinkActionKind(contextMenuPrimaryLink) === 'resume'
      ? 'Resume Session'
      : 'Go to Session'
    : 'Start work';
  const contextMenuPrimaryActionDisabled =
    Boolean(conversationAction) || (!contextMenuPrimaryLink && conversationState.agents.length === 0);

  useEffect(() => {
    if (!newTicketOpen) {
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      newPromptRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [newTicketOpen]);

  return (
    <main className='project-board-shell'>
      {/*
       * CDXC:ProjectBoard 2026-06-09-14:35:
       * The Project surface header is one row: project name, then refresh and create actions. Drop the eyebrow plus generic "Project" title so the board opens directly on the active project name.
       *
       * CDXC:ProjectBoard 2026-06-29-03:49:
       * Remove the Board, Automations, Runs, and Triage tabs from the Kanban board header so disabled future surfaces do not occupy board chrome.
       *
       * CDXC:Automations 2026-06-30-12:51:
       * The Quick-level all-project page is named Automations Overview and should not repeat "Automations" in both the eyebrow and page title. Keep the project-scoped Automate surface eyebrow explicit while the overview uses only "Experimental" above the title.
       */}
      {/*
       * CDXC:ProjectBoardRedesign 2026-08-23:
       * Codex-style header shared by the Kanban and Automate surfaces: quiet
       * eyebrow + regular-weight project title on the left, plain text tabs
       * centered (Automate only), default-size (h-8) actions on the right.
       */}
      <section
        className='grid shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-4'
        data-surface={activeSurfaceTab === 'board' ? 'board' : 'automations'}
      >
        <div className='min-w-0 justify-self-start'>
          <div className='text-xs font-normal text-muted-foreground'>
            {activeSurfaceTab !== 'board'
              ? automationIsExperimental
                ? isAutomationGlobalScope
                  ? 'Experimental'
                  : 'Automations (Experimental)'
                : 'Automations'
              : 'Project'}
          </div>
          <h1 className='m-0 truncate text-[15px] font-normal text-foreground'>{projectName}</h1>
        </div>
        {activeSurfaceTab !== 'board' && !showAutomationComingSoonOverlay ? (
          <nav className='flex items-center gap-1 justify-self-center' aria-label='Automation sections'>
            {(['automations', 'runs', 'triage'] as const).map((tab) => (
              <button
                aria-current={activeSurfaceTab === tab ? 'page' : undefined}
                className={`h-8 cursor-pointer rounded-lg border-0 px-3 text-sm font-normal transition-colors ${
                  activeSurfaceTab === tab
                    ? 'bg-white/[0.06] text-foreground'
                    : 'bg-transparent text-muted-foreground hover:text-foreground/80'
                }`}
                data-active={activeSurfaceTab === tab ? 'true' : 'false'}
                key={tab}
                onClick={() => setActiveSurfaceTab(tab)}
                type='button'
              >
                {tab === 'automations' ? 'Automations' : tab === 'runs' ? 'Runs' : 'Triage'}
              </button>
            ))}
          </nav>
        ) : (
          <div />
        )}
        {/*
         * CDXC:ProjectBoardRedesign 2026-08-23:
         * On the board surface the refresh and + Ticket actions live at the
         * right end of the filter row below, so the header is just the title.
         * The Automate surfaces keep their actions up here beside the tabs.
         */}
        {activeSurfaceTab !== 'board' && !showAutomationComingSoonOverlay ? (
          <div className='flex items-center gap-1.5 justify-self-end'>
            <Button
              aria-label='Refresh project'
              disabled={loadState === 'loading'}
              onClick={() => {
                void loadAutomationState();
              }}
              size='icon'
              variant='ghost'
            >
              <IconRefresh />
            </Button>
            <Button onClick={openNewAutomationDialog} variant='secondary'>
              <IconPlus data-icon='inline-start' />
              Automation
            </Button>
          </div>
        ) : (
          <div />
        )}
      </section>

      {showAutomationComingSoonOverlay ? (
        <AutomationComingSoonOverlay surfaceName={automationSurfaceName} />
      ) : (
        <>
          {activeSurfaceTab === 'board' ? (
            <section className='flex shrink-0 flex-wrap items-center gap-2' aria-label='Ticket filters'>
              <div className='relative w-64'>
                {/*
                 * CDXC:SearchInputs 2026-06-04-03:11:
                 * Project Board ticket search is hosted by the native tasks bundle,
                 * so mirror the sidebar search affordance locally: keep the search
                 * icon on the right while empty, replace it with an X button after
                 * typing, and let Escape clear the focused non-empty field.
                 */}
                <Input
                  aria-label='Search tickets'
                  className='h-8 border-border pr-8'
                  onChange={(event) => setSearchQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key !== 'Escape' || searchQuery.length === 0) {
                      return;
                    }
                    event.preventDefault();
                    event.stopPropagation();
                    setSearchQuery('');
                    searchInputRef.current?.focus();
                  }}
                  placeholder='Search tickets'
                  ref={searchInputRef}
                  value={searchQuery}
                />
                {searchQuery.length > 0 ? (
                  <button
                    aria-label='Clear ticket search'
                    className='absolute right-1.5 top-1/2 flex size-5 -translate-y-1/2 cursor-pointer items-center justify-center rounded-md border-0 bg-transparent text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground [&_svg]:size-4'
                    onClick={() => {
                      setSearchQuery('');
                      searchInputRef.current?.focus();
                    }}
                    type='button'
                  >
                    <IconX aria-hidden='true' />
                  </button>
                ) : (
                  <IconSearch
                    aria-hidden='true'
                    className='pointer-events-none absolute right-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground'
                  />
                )}
              </div>
              <Select
                items={PROJECT_BOARD_PRIORITY_FILTER_SELECT_ITEMS}
                onValueChange={(value) => setPriorityFilter(value as BoardPriorityFilter)}
                value={priorityFilter}
              >
                <SelectTrigger aria-label='Filter by priority'>
                  <SelectValue placeholder='All priorities' />
                </SelectTrigger>
                <SelectContent>
                  {PROJECT_BOARD_PRIORITY_FILTER_SELECT_ITEMS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                items={PROJECT_BOARD_ESTIMATE_FILTER_SELECT_ITEMS}
                onValueChange={(value) => setEstimateFilter(value as BoardEstimateFilter)}
                value={estimateFilter}
              >
                <SelectTrigger aria-label='Filter by estimate'>
                  <SelectValue placeholder='All estimates' />
                </SelectTrigger>
                <SelectContent>
                  {PROJECT_BOARD_ESTIMATE_FILTER_SELECT_ITEMS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {/*
               * CDXC:ProjectBoardRedesign 2026-08-23:
               * Tag and sort move from native <select>s onto the shared shadcn
               * Select so every control on this row has the same 32px height,
               * font, and popup styling.
               */}
              <Select
                items={tagFilterSelectItems}
                onValueChange={(value) => setTagFilter(value as BoardTagFilter)}
                value={activeTagFilter}
              >
                <SelectTrigger aria-label='Filter by tag'>
                  <SelectValue placeholder='All tags' />
                </SelectTrigger>
                <SelectContent>
                  {tagFilterSelectItems.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                items={PROJECT_BOARD_SORT_SELECT_ITEMS}
                onValueChange={(value) => setSortOption(value as BoardSortOption)}
                value={sortOption}
              >
                <SelectTrigger aria-label='Sort tickets'>
                  <SelectValue placeholder='Sort' />
                </SelectTrigger>
                <SelectContent>
                  {PROJECT_BOARD_SORT_SELECT_ITEMS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {/*
               * CDXC:ProjectBoardColumnManagement 2026-08-21:
               * Columns sits with the filters because it changes what the board shows, and it is the only
               * control here that writes to the board rather than to this client's view preferences.
               */}
              <Button
                aria-label='Board columns'
                onClick={() => setColumnsDialogOpen(true)}
                size='icon'
                title='Columns'
                variant='outline'
              >
                <IconLayoutColumns />
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button aria-label='Card details' size='icon' title='View' variant='outline'>
                      <IconAdjustmentsHorizontal />
                    </Button>
                  }
                />
                <DropdownMenuContent align='start'>
                  <DropdownMenuGroup>
                    <DropdownMenuLabel>Card details</DropdownMenuLabel>
                    {BOARD_CARD_VIEW_FIELDS.map((field) => (
                      <DropdownMenuCheckboxItem
                        checked={cardView[field.key]}
                        closeOnClick={false}
                        key={field.key}
                        onCheckedChange={(checked: boolean) => toggleCardViewField(field.key, checked)}
                      >
                        {field.label}
                      </DropdownMenuCheckboxItem>
                    ))}
                  </DropdownMenuGroup>
                </DropdownMenuContent>
              </DropdownMenu>
              <div className='ml-auto flex items-center gap-1.5'>
                <Button
                  aria-label='Refresh project'
                  disabled={loadState === 'loading'}
                  onClick={() => {
                    void loadTickets({ mode: 'manual' });
                    void loadConversationState();
                    void loadAutomationState();
                  }}
                  size='icon'
                  variant='ghost'
                >
                  <IconRefresh />
                </Button>
                <Button onClick={() => openNewTicket()} variant='secondary'>
                  <IconPlus data-icon='inline-start' />
                  Ticket
                </Button>
              </div>
            </section>
          ) : null}

          {activeSurfaceTab === 'triage' ? (
            triageAutomationRuns.length === 0 ? (
              /*
               * CDXC:Automations 2026-06-30-15:35:
               * Empty Runs and Triage tabs should show one centered empty state in a single panel, not the split view with a second "No run selected" placeholder on the right. Match the Automations tab pattern.
               */
              <section className='flex min-h-0 flex-1 flex-col border-t border-border/60 pt-1'>
                <AutomationRunList
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automations={automationState.automations}
                  emptyTitle='No automation results need triage'
                  onArchive={archiveAutomationRun}
                  onMarkRead={markAutomationRunRead}
                  onOpenSession={openAutomationRunSession}
                  onOpenWorktree={openAutomationRunWorktree}
                  onSelect={setSelectedAutomationRunId}
                  projectName={automationState.projectName}
                  runs={triageAutomationRuns}
                  selectedRunId={selectedTriageRun?.id ?? ''}
                />
              </section>
            ) : (
              <section className='grid min-h-0 flex-1 grid-cols-[minmax(280px,0.9fr)_minmax(320px,1.1fr)] border-t border-border/60 pt-1 [&>*:first-child]:border-r [&>*:first-child]:border-border/60'>
                <AutomationRunList
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automations={automationState.automations}
                  emptyTitle='No automation results need triage'
                  onArchive={archiveAutomationRun}
                  onMarkRead={markAutomationRunRead}
                  onOpenSession={openAutomationRunSession}
                  onOpenWorktree={openAutomationRunWorktree}
                  onSelect={setSelectedAutomationRunId}
                  projectName={automationState.projectName}
                  runs={triageAutomationRuns}
                  selectedRunId={selectedTriageRun?.id ?? ''}
                />
                <AutomationRunDetail
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automation={
                    selectedTriageRun
                      ? automationState.automations.find((candidate) => candidate.id === selectedTriageRun.automationId)
                      : undefined
                  }
                  onArchive={archiveAutomationRun}
                  onMarkRead={markAutomationRunRead}
                  onOpenSession={openAutomationRunSession}
                  onOpenWorktree={openAutomationRunWorktree}
                  projectName={automationState.projectName}
                  run={selectedTriageRun}
                />
              </section>
            )
          ) : null}

          {activeSurfaceTab === 'automations' ? (
            automationState.automations.length === 0 ? (
              <section className='flex min-h-0 flex-1 flex-col border-t border-border/60 pt-1'>
                {/*
                 * CDXC:Automations 2026-06-30-09:36:
                 * An empty Automation page should show one centered empty state, not the empty list plus the "No automation selected" detail placeholder. Only restore the split view after at least one automation exists.
                 */}
                <AutomationDefinitionList
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automations={automationState.automations}
                  onCreate={openNewAutomationDialog}
                  onDelete={deleteAutomation}
                  onEdit={openEditAutomationDialog}
                  onRunNow={runAutomationNow}
                  onSelect={setSelectedAutomationId}
                  onSetEnabled={setAutomationEnabled}
                  projectNameById={automationProjectNameById}
                  runs={automationState.runs}
                  selectedAutomationId={selectedAutomation?.id ?? ''}
                  showProjectLabels={isAutomationGlobalScope}
                />
              </section>
            ) : (
              <section className='grid min-h-0 flex-1 grid-cols-[minmax(280px,0.9fr)_minmax(320px,1.1fr)] border-t border-border/60 pt-1 [&>*:first-child]:border-r [&>*:first-child]:border-border/60'>
                <AutomationDefinitionList
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automations={automationState.automations}
                  onCreate={openNewAutomationDialog}
                  onDelete={deleteAutomation}
                  onEdit={openEditAutomationDialog}
                  onRunNow={runAutomationNow}
                  onSelect={setSelectedAutomationId}
                  onSetEnabled={setAutomationEnabled}
                  projectNameById={automationProjectNameById}
                  runs={automationState.runs}
                  selectedAutomationId={selectedAutomation?.id ?? ''}
                  showProjectLabels={isAutomationGlobalScope}
                />
                <AutomationDefinitionDetail
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automation={selectedAutomation}
                  onDelete={deleteAutomation}
                  onEdit={openEditAutomationDialog}
                  onRunNow={runAutomationNow}
                  onSetEnabled={setAutomationEnabled}
                  projectNameById={automationProjectNameById}
                  runs={automationState.runs}
                  showProjectLabels={isAutomationGlobalScope}
                />
              </section>
            )
          ) : null}

          {activeSurfaceTab === 'runs' ? (
            visibleAutomationRuns.length === 0 ? (
              <section className='flex min-h-0 flex-1 flex-col border-t border-border/60 pt-1'>
                <AutomationRunList
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automations={automationState.automations}
                  emptyTitle='No automation runs yet'
                  onArchive={archiveAutomationRun}
                  onMarkRead={markAutomationRunRead}
                  onOpenSession={openAutomationRunSession}
                  onOpenWorktree={openAutomationRunWorktree}
                  onSelect={setSelectedAutomationRunId}
                  projectName={automationState.projectName}
                  runs={visibleAutomationRuns}
                  selectedRunId={selectedVisibleRun?.id ?? ''}
                />
              </section>
            ) : (
              <section className='grid min-h-0 flex-1 grid-cols-[minmax(280px,0.9fr)_minmax(320px,1.1fr)] border-t border-border/60 pt-1 [&>*:first-child]:border-r [&>*:first-child]:border-border/60'>
                <AutomationRunList
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automations={automationState.automations}
                  emptyTitle='No automation runs yet'
                  onArchive={archiveAutomationRun}
                  onMarkRead={markAutomationRunRead}
                  onOpenSession={openAutomationRunSession}
                  onOpenWorktree={openAutomationRunWorktree}
                  onSelect={setSelectedAutomationRunId}
                  projectName={automationState.projectName}
                  runs={visibleAutomationRuns}
                  selectedRunId={selectedVisibleRun?.id ?? ''}
                />
                <AutomationRunDetail
                  actionId={automationActionId}
                  agents={automationState.agents}
                  automation={
                    selectedVisibleRun
                      ? automationState.automations.find(
                          (candidate) => candidate.id === selectedVisibleRun.automationId
                        )
                      : undefined
                  }
                  onArchive={archiveAutomationRun}
                  onMarkRead={markAutomationRunRead}
                  onOpenSession={openAutomationRunSession}
                  onOpenWorktree={openAutomationRunWorktree}
                  projectName={automationState.projectName}
                  run={selectedVisibleRun}
                />
              </section>
            )
          ) : null}

          {activeSurfaceTab === 'board' ? (
            <>
              {errorMessage ? (
                <ProjectBoardNotice
                  canRunBeadsCommands={!remoteMachineId}
                  message={errorMessage}
                  onInstallOrUpdateBeads={installOrUpdateBeads}
                  onInitializeBeads={initializeBeads}
                  onRunBeadsMigration={runBeadsMigration}
                  runningCommand={runningProjectBoardCommand}
                />
              ) : null}
              <div className='project-board-board-region'>
                <DragDropProvider onDragEnd={handleDragEnd}>
                  {/*
                CDXC:ScrollFades 2026-06-19-14:16:
                Kanban uses a horizontal board scroller plus vertical lane
                scrollers. Apply the shared Codex-style masks to the scroll
                containers themselves so lane headers and custom scrollbars stay
                crisp while overflowing cards fade at the edges.

                CDXC:BoardScrollbars 2026-08-07:
                Both bars are now the browser's own scrollbars on those same
                masked scrollers, so their ends fade with the content instead of
                staying crisp.
              */}
                  <section
                    className='project-board-lanes horizontal-scroll-fade-mask grid min-h-0 flex-1 auto-cols-[minmax(230px,1fr)] grid-flow-col items-stretch gap-2.5 overflow-x-auto overflow-y-hidden [--edge-fade-distance:18px]'
                    aria-label='Project issue board'
                  >
                    {boardColumns.map((column) => (
                      <BoardLane
                        cardView={cardView}
                        column={column}
                        conversationAction={conversationAction}
                        key={column.key}
                        linksByBeadKey={linksByBeadKey}
                        onAddTicket={openNewTicket}
                        onJumpToConversation={jumpToConversation}
                        onOpenContextMenu={(ticket, point) =>
                          setTicketContextMenu({
                            confirmingDelete: false,
                            ticketId: ticket.id,
                            x: point.x,
                            y: point.y,
                          })
                        }
                        onOpenTicket={openTicket}
                        tickets={ticketsByColumn[column.key]}
                      />
                    ))}
                  </section>
                </DragDropProvider>
                {showInitialBoardLoadingOverlay ? (
                  <div
                    aria-label='Loading board'
                    aria-live='polite'
                    className='project-board-loading-overlay'
                    role='status'
                  >
                    <IconLoader2 aria-hidden='true' className='project-board-loading-spinner' size={32} stroke={1.8} />
                  </div>
                ) : null}
              </div>
              {ticketContextMenu && contextMenuTicket ? (
                <ProjectBoardTicketContextMenu
                  confirmingDelete={ticketContextMenu.confirmingDelete}
                  deleting={contextMenuDeletingTicketId === contextMenuTicket.id}
                  onDelete={() => {
                    if (!ticketContextMenu.confirmingDelete) {
                      setTicketContextMenu((current) =>
                        current?.ticketId === contextMenuTicket.id ? { ...current, confirmingDelete: true } : current
                      );
                      return;
                    }
                    void deleteTicket(contextMenuTicket);
                  }}
                  onDismiss={() => setTicketContextMenu(undefined)}
                  onPrimaryAction={() => {
                    setTicketContextMenu(undefined);
                    if (contextMenuPrimaryLink) {
                      void jumpToConversation(contextMenuPrimaryLink);
                      return;
                    }
                    void startTicketWork(contextMenuTicket);
                  }}
                  position={ticketContextMenu}
                  primaryActionDisabled={contextMenuPrimaryActionDisabled}
                  primaryActionLabel={contextMenuPrimaryActionLabel}
                />
              ) : null}
            </>
          ) : errorMessage ? (
            <ProjectBoardNotice
              canRunBeadsCommands={!remoteMachineId}
              message={errorMessage}
              onInstallOrUpdateBeads={installOrUpdateBeads}
              onInitializeBeads={initializeBeads}
              onRunBeadsMigration={runBeadsMigration}
              runningCommand={runningProjectBoardCommand}
            />
          ) : null}
        </>
      )}

      {experimentalFeaturesEnabled ? (
        <AutomationDialog
          automationActionId={automationActionId}
          automationAgentSelectItems={automationAgentSelectItems}
          automationConversationState={automationConversationState}
          automationDraft={automationDraft}
          automationDraftCanUseWorktrees={automationDraftCanUseWorktrees}
          automationDraftWorktreeUnavailableReason={automationDraftWorktreeUnavailableReason}
          automationProjectSelectItems={automationProjectSelectItems}
          automationScheduleSelectItems={automationScheduleSelectItems}
          automationSessionSelectItems={automationSessionSelectItems}
          automationState={automationState}
          automationTimerUnitSelectItems={automationTimerUnitSelectItems}
          automationWeekdaySelectItems={automationWeekdaySelectItems}
          isAutomationGlobalScope={isAutomationGlobalScope}
          onOpenChange={setAutomationDialogOpen}
          onProjectChange={(value) => {
            const selectedProject = automationProjectsById.get(value);
            setAutomationDraft((current) => ({
              ...current,
              executionKind:
                current.executionKind === 'worktree' && selectedProject?.canUseWorktrees !== true
                  ? 'local'
                  : current.executionKind,
              projectId: value,
              threadSessionId: '',
              threadAgentSessionId: '',
            }));
            void loadAutomationConversationState(value);
          }}
          onSave={() => void saveAutomation()}
          open={automationDialogOpen}
          projectName={projectName}
          setAutomationDraft={setAutomationDraft}
        />
      ) : null}

      <BoardColumnsDialog
        columns={boardColumns}
        config={boardColumnConfig}
        onClose={() => setColumnsDialogOpen(false)}
        onCreate={createBoardColumn}
        onDelete={deleteBoardColumn}
        onRename={applyBoardColumnRename}
        onReorder={reorderBoardColumn}
        open={columnsDialogOpen}
        tickets={tickets}
      />
      <EditTicketDialog
        boardColumns={boardColumns}
        conversationAction={conversationAction}
        conversationState={conversationState}
        deleteConfirmingTicketId={deleteConfirmingTicketId}
        detail={detail}
        detailCommentMetadataLink={detailCommentMetadataLink}
        detailConversationLinks={detailConversationLinks}
        detailPrimaryActionDisabled={detailPrimaryActionDisabled}
        detailPrimaryActionKind={detailPrimaryActionKind}
        detailPrimaryActionLabel={detailPrimaryActionLabel}
        detailPrimaryConversationLink={detailPrimaryConversationLink}
        imagePreviewDataUrls={imagePreviewDataUrls}
        knownLabels={knownLabels}
        onAssociateFocusedSession={() => void associateFocusedSession()}
        onClose={() => {
          setDeleteConfirmingTicketId('');
          setDetail(createEmptyDetailDraft());
        }}
        onDeleteTicket={() => void deleteTicket()}
        onJumpToConversation={(link) => void jumpToConversation(link)}
        onSaveTicketDetail={() => void saveTicketDetail()}
        onSelectedAgentChange={selectTicketAgent}
        onStartTicketWork={() => {
          setDeleteConfirmingTicketId('');
          setDetail(createEmptyDetailDraft());
          void startTicketWork();
        }}
        onUnlinkConversation={(link) => void unlinkConversation(link)}
        selectedAgentId={selectedAgentId}
        setDeleteConfirmingTicketId={setDeleteConfirmingTicketId}
        setDetail={setDetail}
        setErrorMessage={setErrorMessage}
        ticketOptions={ticketOptions}
        tickets={tickets}
      />

      <NewTicketDialog
        agentSelectItems={agentSelectItems}
        boardColumns={boardColumns}
        conversationAction={conversationAction}
        conversationState={conversationState}
        imagePreviewDataUrls={imagePreviewDataUrls}
        knownLabels={knownLabels}
        newPromptRef={newPromptRef}
        newTicket={newTicket}
        newTicketStartLocation={newTicketStartLocation}
        onCreateTicket={(options) => void createTicket(options)}
        onOpenChange={(open) => {
          setNewTicketOpen(open);
          if (!open) {
            setNewTicketStartLocation('currentProject');
          }
        }}
        onSelectedAgentChange={setSelectedAgentId}
        open={newTicketOpen}
        selectedAgentId={selectedAgentId}
        setErrorMessage={setErrorMessage}
        setNewTicket={setNewTicket}
        setNewTicketStartLocation={setNewTicketStartLocation}
        ticketOptions={ticketOptions}
      />
      {/*
       * CDXC:ProjectBoardBlockedMove 2026-08-20:
       * The board's own toaster lives in this surface rather than the native
       * app-modal host because board rejections need an inline action button,
       * which the native showToast bridge carries no room for. Match the modal
       * host's dark toast chrome using the board's surface tokens.
       */}
      <Toaster
        closeButton
        position='bottom-center'
        richColors
        theme='dark'
        toastOptions={{
          style: {
            background: 'var(--project-board-panel)',
            border: '1px solid var(--project-board-border-strong)',
            color: '#f4f4f5',
          },
        }}
      />
    </main>
  );
}
